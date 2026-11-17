//! Biblioteca SQLite: índice de fondos (sin copiar archivos), favoritos y estado.

use nekowall_backend::{id_for_path, Wallpaper, WallpaperKind};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum LibraryError {
    Sqlite(rusqlite::Error),
    Io(io::Error),
    NotFound(PathBuf),
    Unsupported(PathBuf),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "error de base de datos: {e}"),
            Self::Io(e) => write!(f, "error de E/S: {e}"),
            Self::NotFound(p) => write!(f, "no existe: {}", p.display()),
            Self::Unsupported(p) => write!(f, "formato no soportado: {}", p.display()),
        }
    }
}
impl std::error::Error for LibraryError {}
impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl From<io::Error> for LibraryError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub wallpaper: Wallpaper,
    pub favorite: bool,
    pub missing: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub kind: Option<WallpaperKind>,
    pub favorites_only: bool,
    pub query: Option<String>,
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub added: usize,
    pub duplicates: usize,
    pub unsupported: usize,
    /// Solo en la revisión de carpetas: archivos dejados para luego por ser muy recientes.
    pub recent: usize,
    pub errors: Vec<(PathBuf, String)>,
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub added: usize,
    /// Archivos recién modificados (quizá aún copiándose): se añadirán en la próxima revisión.
    pub recent: usize,
    /// Quitados porque se borraron de una carpeta revisada.
    pub removed: usize,
    pub missing: usize,
    /// Carpetas y subcarpetas revisadas (para vigilarlas desde la interfaz).
    pub dirs: Vec<PathBuf>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub favorites: usize,
    pub missing: usize,
}

pub struct Library {
    conn: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS wallpapers (
    id        TEXT PRIMARY KEY,
    path      TEXT NOT NULL UNIQUE,
    kind      TEXT NOT NULL,
    favorite  INTEGER NOT NULL DEFAULT 0,
    missing   INTEGER NOT NULL DEFAULT 0,
    size      INTEGER NOT NULL DEFAULT 0,
    added_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Carpetas añadidas por el usuario: se vuelven a revisar para encontrar fondos nuevos.
CREATE TABLE IF NOT EXISTS folders (
    path TEXT PRIMARY KEY
);
-- Archivos que el usuario quitó: la revisión de carpetas no los vuelve a añadir.
CREATE TABLE IF NOT EXISTS ignored (
    path TEXT PRIMARY KEY
);
";

impl Library {
    pub fn open(path: &Path) -> Result<Self, LibraryError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Self::from_conn(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, LibraryError> {
        Self::from_conn(Connection::open_in_memory()?)
    }

    fn from_conn(conn: Connection) -> Result<Self, LibraryError> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Indexa un archivo o una carpeta (recursivo) sin copiar nada. Las carpetas quedan recordadas
    /// para `sync_folders`; un archivo importado a mano deja de estar en la lista de ignorados.
    pub fn import_path(&mut self, path: &Path) -> Result<ImportReport, LibraryError> {
        self.import_inner(path, false)
    }

    /// `skip_recent`: deja para luego los archivos modificados hace menos de 3 s (aún copiándose).
    fn import_inner(&mut self, path: &Path, skip_recent: bool) -> Result<ImportReport, LibraryError> {
        let path = fs::canonicalize(path).map_err(|_| LibraryError::NotFound(path.to_path_buf()))?;
        let mut report = ImportReport::default();
        let mut files = Vec::new();

        let tx = self.conn.transaction()?;
        if path.is_dir() {
            collect_files(&path, &mut files, &mut report);
            files.sort();
            if let Some(p) = path.to_str() {
                tx.execute("INSERT OR IGNORE INTO folders (path) VALUES (?1)", params![p])?;
            }
            let ignored: std::collections::HashSet<String> = {
                let mut st = tx.prepare("SELECT path FROM ignored")?;
                let it = st.query_map([], |r| r.get::<_, String>(0))?;
                it.collect::<Result<_, _>>()?
            };
            files.retain(|f| f.to_str().is_none_or(|s| !ignored.contains(s)));
            if skip_recent {
                let now = SystemTime::now();
                let before = files.len();
                files.retain(|f| {
                    fs::metadata(f)
                        .and_then(|m| m.modified())
                        .map_or(true, |t| now.duration_since(t).is_ok_and(|d| d.as_secs() >= 3))
                });
                report.recent = before - files.len();
            }
        } else {
            if WallpaperKind::from_path(&path).is_none() {
                return Err(LibraryError::Unsupported(path));
            }
            if let Some(p) = path.to_str() {
                tx.execute("DELETE FROM ignored WHERE path = ?1", params![p])?;
            }
            files.push(path);
        }

        for file in files {
            let Some(kind) = WallpaperKind::from_path(&file) else {
                report.unsupported += 1;
                continue;
            };
            let Some(path_str) = file.to_str() else {
                report.errors.push((file.clone(), "ruta no UTF-8".into()));
                continue;
            };
            let size = fs::metadata(&file).map(|m| m.len() as i64).unwrap_or(0);
            let changed = tx.execute(
                "INSERT OR IGNORE INTO wallpapers (id, path, kind, size, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id_for_path(&file), path_str, kind.as_str(), size, now()],
            )?;
            if changed == 0 {
                report.duplicates += 1;
            } else {
                report.added += 1;
            }
        }
        tx.commit()?;
        Ok(report)
    }

    pub fn list(&self, filter: &Filter) -> Result<Vec<Entry>, LibraryError> {
        let mut sql = String::from("SELECT id, path, kind, favorite, missing FROM wallpapers");
        let mut conds: Vec<&str> = Vec::new();
        let mut args: Vec<Value> = Vec::new();

        if let Some(kind) = filter.kind {
            conds.push("kind = ?");
            args.push(Value::Text(kind.as_str().into()));
        }
        if filter.favorites_only {
            conds.push("favorite = 1");
        }
        if let Some(q) = filter.query.as_deref().filter(|q| !q.trim().is_empty()) {
            conds.push("path LIKE ? ESCAPE '\\'");
            let esc = q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            args.push(Value::Text(format!("%{esc}%")));
        }
        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        sql.push_str(" ORDER BY path");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), row_to_entry)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<Option<Entry>, LibraryError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, path, kind, favorite, missing FROM wallpapers WHERE id = ?1",
                params![id],
                row_to_entry,
            )
            .optional()?)
    }

    pub fn get_by_path(&self, path: &Path) -> Result<Option<Entry>, LibraryError> {
        self.get(&id_for_path(path))
    }

    /// Devuelve `false` si el id no existe.
    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<bool, LibraryError> {
        let n = self
            .conn
            .execute("UPDATE wallpapers SET favorite = ?1 WHERE id = ?2", params![favorite as i64, id])?;
        Ok(n > 0)
    }

    /// Marca las entradas cuyo archivo ya no existe. Devuelve cuántas faltan.
    pub fn refresh_missing(&mut self) -> Result<usize, LibraryError> {
        let all: Vec<(String, String)> = {
            let mut stmt = self.conn.prepare("SELECT id, path FROM wallpapers")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        let tx = self.conn.transaction()?;
        let mut missing = 0;
        for (id, path) in all {
            let gone = !Path::new(&path).exists();
            missing += gone as usize;
            tx.execute("UPDATE wallpapers SET missing = ?1 WHERE id = ?2", params![gone as i64, id])?;
        }
        tx.commit()?;
        Ok(missing)
    }

    /// Quita de la biblioteca (nunca borra el archivo original). El archivo queda «ignorado» para que
    /// la revisión de carpetas no lo vuelva a añadir.
    pub fn remove(&self, id: &str) -> Result<bool, LibraryError> {
        let path: Option<String> = self
            .conn
            .query_row("SELECT path FROM wallpapers WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        let Some(path) = path else { return Ok(false) };
        self.conn.execute("INSERT OR IGNORE INTO ignored (path) VALUES (?1)", params![path])?;
        Ok(self.conn.execute("DELETE FROM wallpapers WHERE id = ?1", params![id])? > 0)
    }

    /// Carpetas recordadas (las que se añadieron con «Añadir carpeta…»).
    pub fn folders(&self) -> Result<Vec<PathBuf>, LibraryError> {
        let mut st = self.conn.prepare("SELECT path FROM folders ORDER BY path")?;
        let it = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(it.collect::<Result<Vec<_>, _>>()?.into_iter().map(PathBuf::from).collect())
    }

    /// Deja de revisar una carpeta (sus fondos siguen en la biblioteca). `false` si no estaba.
    pub fn forget_folder(&self, path: &Path) -> Result<bool, LibraryError> {
        let p = path.to_string_lossy();
        let p = p.trim_end_matches('/');
        Ok(self.conn.execute("DELETE FROM folders WHERE path = ?1", params![p])? > 0)
    }

    /// Vuelve a revisar las carpetas recordadas: añade los fondos nuevos (salvo los quitados a mano)
    /// y marca los que ya no existen. Una carpeta que no existe (disco sin montar) se salta.
    pub fn sync_folders(&mut self) -> Result<SyncReport, LibraryError> {
        self.adopt_old_folders()?;
        let mut report = SyncReport::default();
        for dir in self.folders()? {
            if !dir.is_dir() {
                continue;
            }
            let r = self.import_inner(&dir, true)?;
            report.added += r.added;
            report.recent += r.recent;
            // Borrados de una carpeta que sí está (no un disco sin montar): fuera de la biblioteca.
            let prefix = format!("{}/", dir.to_string_lossy().trim_end_matches('/'));
            let gone: Vec<String> = {
                let mut st = self.conn.prepare("SELECT path FROM wallpapers WHERE substr(path, 1, length(?1)) = ?1")?;
                let it = st.query_map(params![prefix], |r| r.get::<_, String>(0))?;
                it.collect::<Result<Vec<_>, _>>()?.into_iter().filter(|p| !Path::new(p).exists()).collect()
            };
            for p in gone {
                report.removed += self.conn.execute("DELETE FROM wallpapers WHERE path = ?1", params![p])?;
            }
            report.dirs.push(dir.clone());
            collect_dirs(&dir, &mut report.dirs);
        }
        report.missing = self.refresh_missing()?;
        Ok(report)
    }

    /// Bibliotecas creadas antes de recordar carpetas: una carpeta se recuerda si todos los fondos que
    /// contiene (subcarpetas incluidas, porque la revisión es recursiva) están en la biblioteca, o
    /// llegaron DESPUÉS de importarla (los copió el usuario luego). Si hay alguno más antiguo sin
    /// importar, se eligieron archivos sueltos y no se adopta. Los originales sustituidos por su copia
    /// optimizada («nombre (1080p30).mp4») pasan a ignorados. Solo se hace una vez.
    fn adopt_old_folders(&mut self) -> Result<(), LibraryError> {
        use std::os::unix::fs::MetadataExt;
        const KEY: &str = "folders_adopted";
        if self.state(KEY)?.is_some() {
            return Ok(());
        }
        let rows: Vec<(String, i64)> = {
            let mut st = self.conn.prepare("SELECT path, added_at FROM wallpapers")?;
            let it = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            it.collect::<Result<_, _>>()?
        };
        let mut known: std::collections::HashSet<String> = rows.iter().map(|(p, _)| p.clone()).collect();
        {
            let mut st = self.conn.prepare("SELECT path FROM ignored")?;
            for p in st.query_map([], |r| r.get::<_, String>(0))? {
                known.insert(p?);
            }
        }
        // Originales de copias optimizadas que están en la biblioteca → ignorados.
        for (p, _) in &rows {
            let Some(orig_stem) = optimized_source_stem(p) else { continue };
            let Some(dir) = Path::new(p).parent() else { continue };
            let Ok(rd) = fs::read_dir(dir) else { continue };
            for e in rd.flatten() {
                let q = e.path();
                let same = q.file_stem().is_some_and(|s| s.to_string_lossy() == orig_stem);
                if same && WallpaperKind::from_path(&q) == Some(WallpaperKind::Video) {
                    if let Some(qs) = q.to_str().filter(|qs| !known.contains(*qs)) {
                        self.conn.execute("INSERT OR IGNORE INTO ignored (path) VALUES (?1)", params![qs])?;
                        known.insert(qs.to_string());
                    }
                }
            }
        }
        // Carpeta → momento de la última importación de algo de ella.
        let mut parents: std::collections::BTreeMap<PathBuf, i64> = Default::default();
        for (p, t) in &rows {
            if let Some(dir) = Path::new(p).parent() {
                let e = parents.entry(dir.to_path_buf()).or_insert(*t);
                *e = (*e).max(*t);
            }
        }
        for (dir, imported_at) in parents {
            let mut files = Vec::new();
            collect_files(&dir, &mut files, &mut ImportReport::default());
            let ok = files.iter().all(|f| {
                WallpaperKind::from_path(f).is_none()
                    || f.to_str().is_some_and(|s| known.contains(s))
                    || fs::metadata(f).is_ok_and(|m| m.ctime() > imported_at)
            });
            if ok {
                if let Some(p) = dir.to_str() {
                    self.conn.execute("INSERT OR IGNORE INTO folders (path) VALUES (?1)", params![p])?;
                }
            }
        }
        self.set_state(KEY, "1")
    }

    /// Actualiza las rutas de los fondos que están dentro de `old_dir` para que apunten a `new_dir`
    /// (tras mover o renombrar la carpeta). Conserva favoritos y fecha; el id se recalcula a partir
    /// de la ruta nueva. También corrige las rutas guardadas en `state`. Devuelve los pares
    /// (id viejo, id nuevo) para que quien llama pueda renombrar sus cachés (miniaturas).
    pub fn relocate(&mut self, old_dir: &Path, new_dir: &Path) -> Result<Vec<(String, String)>, LibraryError> {
        let old = old_dir.to_string_lossy().trim_end_matches('/').to_string();
        let new = new_dir.to_string_lossy().trim_end_matches('/').to_string();
        let prefix = format!("{old}/");
        let tx = self.conn.transaction()?;
        let rows: Vec<(String, String)> = {
            let mut st = tx.prepare("SELECT id, path FROM wallpapers")?;
            let it = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            it.collect::<Result<_, _>>()?
        };
        let mut moved = Vec::new();
        for (id, path) in rows {
            let Some(rest) = path.strip_prefix(&prefix) else { continue };
            let new_path = format!("{new}/{rest}");
            let new_id = id_for_path(Path::new(&new_path));
            let missing = !Path::new(&new_path).exists();
            tx.execute(
                "UPDATE wallpapers SET id = ?1, path = ?2, missing = ?3 WHERE id = ?4",
                params![new_id, new_path, missing as i64, id],
            )?;
            moved.push((id, new_id));
        }
        for table in ["folders", "ignored"] {
            let paths: Vec<String> = {
                let mut st = tx.prepare(&format!("SELECT path FROM {table}"))?;
                let it = st.query_map([], |r| r.get::<_, String>(0))?;
                it.collect::<Result<_, _>>()?
            };
            for p in paths {
                let moved_to = if p == old { Some(new.clone()) } else { p.strip_prefix(&prefix).map(|r| format!("{new}/{r}")) };
                if let Some(np) = moved_to {
                    tx.execute(&format!("UPDATE OR REPLACE {table} SET path = ?1 WHERE path = ?2"), params![np, p])?;
                }
            }
        }
        for key in ["current_path", "previous_path"] {
            let v: Option<String> = tx
                .query_row("SELECT value FROM state WHERE key = ?1", params![key], |r| r.get(0))
                .optional()?;
            if let Some(rest) = v.as_deref().and_then(|v| v.strip_prefix(&prefix)) {
                tx.execute("UPDATE state SET value = ?1 WHERE key = ?2", params![format!("{new}/{rest}"), key])?;
            }
        }
        tx.commit()?;
        Ok(moved)
    }

    pub fn counts(&self) -> Result<Counts, LibraryError> {
        let (total, favorites, missing): (i64, i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(favorite),0), COALESCE(SUM(missing),0) FROM wallpapers",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(Counts { total: total as usize, favorites: favorites as usize, missing: missing as usize })
    }

    pub fn state(&self, key: &str) -> Result<Option<String>, LibraryError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM state WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<(), LibraryError> {
        self.conn.execute(
            "INSERT INTO state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    let path: String = r.get(1)?;
    let kind: String = r.get(2)?;
    Ok(Entry {
        wallpaper: Wallpaper {
            id: r.get(0)?,
            path: PathBuf::from(path),
            kind: WallpaperKind::parse(&kind).unwrap_or(WallpaperKind::Static),
        },
        favorite: r.get::<_, i64>(3)? != 0,
        missing: r.get::<_, i64>(4)? != 0,
    })
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Recorre carpetas sin seguir enlaces simbólicos (evita ciclos).
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, report: &mut ImportReport) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            report.errors.push((dir.to_path_buf(), e.to_string()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Ocultos fuera: archivos a medio escribir (p. ej. «.nombre.part.mp4» de `optimize`) y carpetas
        // como .thumbnails o .git.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_dir() => collect_files(&path, out, report),
            Ok(t) if t.is_file() => out.push(path),
            Ok(_) => {}
            Err(e) => report.errors.push((path, e.to_string())),
        }
    }
}

/// «/x/Nombre (1080p30).mp4» → «Nombre» (nombre de las copias que crea `optimize`).
fn optimized_source_stem(path: &str) -> Option<String> {
    let name = Path::new(path).file_name()?.to_str()?;
    let base = name.strip_suffix(").mp4")?;
    let open = base.rfind(" (")?;
    let (h, f) = base[open + 2..].split_once('p')?;
    (!h.is_empty() && !f.is_empty() && h.bytes().chain(f.bytes()).all(|b| b.is_ascii_digit()))
        .then(|| base[..open].to_string())
}

/// Subcarpetas (recursivo, sin seguir enlaces simbólicos).
fn collect_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) && !e.file_name().to_string_lossy().starts_with('.') {
            out.push(e.path());
            collect_dirs(&e.path(), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("nekowall-lib-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Crea el archivo con fecha de hace un minuto (la revisión deja para luego lo muy reciente).
    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
        let f = fs::File::options().write(true).open(p).unwrap();
        f.set_modified(SystemTime::now() - std::time::Duration::from_secs(60)).unwrap();
    }

    #[test]
    fn imports_folder_recursively_and_detects_duplicates() {
        let d = tmp();
        touch(&d.join("a.png"));
        touch(&d.join("sub/b.mp4"));
        touch(&d.join("sub/notas.txt"));
        let mut lib = Library::open_in_memory().unwrap();

        let r = lib.import_path(&d).unwrap();
        assert_eq!((r.added, r.duplicates, r.unsupported), (2, 0, 1));
        let r = lib.import_path(&d).unwrap();
        assert_eq!((r.added, r.duplicates), (0, 2));

        assert_eq!(lib.list(&Filter::default()).unwrap().len(), 2);
        let videos = lib.list(&Filter { kind: Some(WallpaperKind::Video), ..Default::default() }).unwrap();
        assert_eq!(videos.len(), 1);
    }

    #[test]
    fn single_unsupported_file_is_an_error() {
        let d = tmp();
        touch(&d.join("x.txt"));
        let mut lib = Library::open_in_memory().unwrap();
        assert!(matches!(lib.import_path(&d.join("x.txt")), Err(LibraryError::Unsupported(_))));
        assert!(matches!(lib.import_path(&d.join("nada.png")), Err(LibraryError::NotFound(_))));
    }

    #[test]
    fn favorites_search_and_wildcard_escaping() {
        let d = tmp();
        touch(&d.join("gato_negro.png"));
        touch(&d.join("gatoXnegro.png"));
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&d).unwrap();

        // "_" debe tratarse literal, no como comodín de LIKE
        let q = |s: &str| lib.list(&Filter { query: Some(s.into()), ..Default::default() }).unwrap();
        assert_eq!(q("gato_negro").len(), 1);
        assert_eq!(q("100%").len(), 0);

        let e = &q("gato_negro")[0];
        assert!(lib.set_favorite(&e.wallpaper.id, true).unwrap());
        assert!(!lib.set_favorite("inexistente", true).unwrap());
        let favs = lib.list(&Filter { favorites_only: true, ..Default::default() }).unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(lib.counts().unwrap(), Counts { total: 2, favorites: 1, missing: 0 });
    }

    #[test]
    fn detects_missing_files_and_remove_keeps_original() {
        let d = tmp();
        touch(&d.join("a.png"));
        touch(&d.join("b.png"));
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&d).unwrap();

        fs::remove_file(d.join("a.png")).unwrap();
        assert_eq!(lib.refresh_missing().unwrap(), 1);
        assert_eq!(lib.counts().unwrap().missing, 1);

        let b = lib.get_by_path(&fs::canonicalize(d.join("b.png")).unwrap()).unwrap().unwrap();
        assert!(lib.remove(&b.wallpaper.id).unwrap());
        assert!(d.join("b.png").exists());
    }

    #[test]
    fn state_and_persistence_across_reopen() {
        let d = tmp();
        let db = d.join("sub/lib.db");
        touch(&d.join("a.png"));
        {
            let mut lib = Library::open(&db).unwrap();
            lib.import_path(&d.join("a.png")).unwrap();
            lib.set_state("k", "v1").unwrap();
            lib.set_state("k", "v2").unwrap();
        }
        let lib = Library::open(&db).unwrap();
        assert_eq!(lib.state("k").unwrap().as_deref(), Some("v2"));
        assert_eq!(lib.state("otro").unwrap(), None);
        assert_eq!(lib.counts().unwrap().total, 1);
    }

    #[test]
    fn relocate_keeps_favorites_and_ignores_similar_prefixes() {
        let base = std::env::temp_dir().join(format!("nw-reloc-{}", std::process::id()));
        let (a, b, other) = (base.join("Aura"), base.join("Videos"), base.join("Aura2"));
        for d in [&a, &b, &other] { fs::create_dir_all(d).unwrap(); }
        fs::write(b.join("x.mp4"), b"v").unwrap();          // ya movido en disco
        fs::write(a.join("x.mp4"), b"v").unwrap();
        fs::write(other.join("y.mp4"), b"v").unwrap();
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&a).unwrap();
        lib.import_path(&other).unwrap();
        let a = fs::canonicalize(&a).unwrap();
        let b = fs::canonicalize(&b).unwrap();
        let old_id = id_for_path(&a.join("x.mp4"));
        lib.set_favorite(&old_id, true).unwrap();
        lib.set_state("current_path", &a.join("x.mp4").to_string_lossy()).unwrap();

        let moved = lib.relocate(&a, &b).unwrap();
        assert_eq!(moved.len(), 1, "Aura2 no debe confundirse con Aura");
        let e = lib.get_by_path(&b.join("x.mp4")).unwrap().expect("ruta nueva");
        assert!(e.favorite && !e.missing);
        assert!(lib.get(&old_id).unwrap().is_none());
        assert_eq!(lib.state("current_path").unwrap().unwrap(), b.join("x.mp4").to_string_lossy());
        assert_eq!(lib.counts().unwrap().total, 2);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_finds_new_files_and_respects_removed_ones() {
        let d = tmp();
        touch(&d.join("a.png"));
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&d).unwrap();
        assert_eq!(lib.folders().unwrap(), vec![fs::canonicalize(&d).unwrap()]);

        // Archivos copiados después a la carpeta (también en subcarpetas).
        touch(&d.join("b.png"));
        touch(&d.join("sub/c.mp4"));
        touch(&d.join(".c.part.mp4")); // conversión a medias: oculto, no cuenta
        let r = lib.sync_folders().unwrap();
        assert_eq!(r.added, 2);
        assert!(r.dirs.contains(&fs::canonicalize(d.join("sub")).unwrap()));

        // Quitar uno: la revisión no lo vuelve a añadir…
        let b = lib.get_by_path(&fs::canonicalize(d.join("b.png")).unwrap()).unwrap().unwrap();
        lib.remove(&b.wallpaper.id).unwrap();
        assert_eq!(lib.sync_folders().unwrap().added, 0);
        // Un archivo que aún se está copiando espera a la próxima revisión.
        fs::write(d.join("copiando.mp4"), b"x").unwrap();
        let r = lib.sync_folders().unwrap();
        assert_eq!((r.added, r.recent), (0, 1));
        // …salvo que se importe a mano («deshacer»).
        assert_eq!(lib.import_path(&d.join("b.png")).unwrap().added, 1);

        // Borrado de la carpeta → desaparece de la biblioteca.
        fs::remove_file(d.join("sub/c.mp4")).unwrap();
        let r = lib.sync_folders().unwrap();
        assert_eq!((r.removed, r.missing), (1, 0));

        assert!(lib.forget_folder(&fs::canonicalize(&d).unwrap()).unwrap());
        touch(&d.join("z.png"));
        assert_eq!(lib.sync_folders().unwrap().added, 0);
    }

    #[test]
    fn old_libraries_adopt_fully_imported_folders() {
        let full = tmp();
        touch(&full.join("a.png"));
        touch(&full.join("b.png"));
        let partial = tmp();
        touch(&partial.join("x.png"));
        touch(&partial.join("y.png"));
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&full.join("a.png")).unwrap();
        lib.import_path(&full.join("b.png")).unwrap();
        lib.import_path(&partial.join("x.png")).unwrap();
        // Simula una importación antigua en «full» (nuevo.png llega después) y una futura en
        // «partial» (y.png ya estaba cuando se eligió solo x.png).
        let (f, pa) = (fs::canonicalize(&full).unwrap(), fs::canonicalize(&partial).unwrap());
        lib.conn.execute("UPDATE wallpapers SET added_at = added_at - 1000 WHERE path LIKE ?1", params![format!("{}/%", f.display())]).unwrap();
        lib.conn.execute("UPDATE wallpapers SET added_at = added_at + 1000 WHERE path LIKE ?1", params![format!("{}/%", pa.display())]).unwrap();

        touch(&full.join("nuevo.png"));
        // La carpeta completa se adopta (y encuentra el nuevo); la parcial no (y.png no entra).
        assert_eq!(lib.sync_folders().unwrap().added, 1);
        assert_eq!(lib.folders().unwrap(), vec![fs::canonicalize(&full).unwrap()]);
    }

    #[test]
    fn adoption_ignores_originals_of_optimized_copies() {
        let d = tmp();
        touch(&d.join("a.png"));
        touch(&d.join("v/Clip 4k.mp4"));
        touch(&d.join("v/Clip 4k (1080p30).mp4"));
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&d.join("a.png")).unwrap();
        lib.import_path(&d.join("v/Clip 4k (1080p30).mp4")).unwrap();
        lib.conn.execute("UPDATE wallpapers SET added_at = added_at + 1000", []).unwrap();

        assert_eq!(lib.sync_folders().unwrap().added, 0);
        let d = fs::canonicalize(&d).unwrap();
        assert_eq!(lib.folders().unwrap(), vec![d.clone(), d.join("v")]);
        assert_eq!(optimized_source_stem("/x/Clip 4k (1080p30).mp4").as_deref(), Some("Clip 4k"));
        assert_eq!(optimized_source_stem("/x/Clip (2019).mp4"), None);
    }
}
