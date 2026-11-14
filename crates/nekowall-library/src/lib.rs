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
    pub errors: Vec<(PathBuf, String)>,
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

    /// Indexa un archivo o una carpeta (recursivo) sin copiar nada.
    pub fn import_path(&mut self, path: &Path) -> Result<ImportReport, LibraryError> {
        let path = fs::canonicalize(path).map_err(|_| LibraryError::NotFound(path.to_path_buf()))?;
        let mut report = ImportReport::default();
        let mut files = Vec::new();

        if path.is_dir() {
            collect_files(&path, &mut files, &mut report);
            files.sort();
        } else {
            if WallpaperKind::from_path(&path).is_none() {
                return Err(LibraryError::Unsupported(path));
            }
            files.push(path);
        }

        let tx = self.conn.transaction()?;
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

    /// Quita de la biblioteca (nunca borra el archivo original).
    pub fn remove(&self, id: &str) -> Result<bool, LibraryError> {
        Ok(self.conn.execute("DELETE FROM wallpapers WHERE id = ?1", params![id])? > 0)
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
        match entry.file_type() {
            Ok(t) if t.is_dir() => collect_files(&path, out, report),
            Ok(t) if t.is_file() => out.push(path),
            Ok(_) => {}
            Err(e) => report.errors.push((path, e.to_string())),
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

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
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
}
