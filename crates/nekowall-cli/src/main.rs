use nekowall_backend::{WallpaperBackend, WallpaperKind};
use nekowall_core::Service;
use nekowall_library::{Entry, Filter, Library};
use nekowall_mate::MateBackend;
use nekowall_plasma::PlasmaBackend;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod link;
mod media;

/// Valor de una opción «--nombre valor».
fn opt<'a>(rest: &'a [String], name: &str) -> Option<&'a str> {
    rest.iter().position(|a| a == name).and_then(|i| rest.get(i + 1)).map(String::as_str)
}

const HELP: &str = "\
nekowall — gestor de fondos de pantalla (prototipo)

USO:
  nekowall import <ruta>...            Indexa archivos o carpetas (sin copiar)
  nekowall list [--video|--static] [--favorites] [--search TEXTO]
  nekowall apply <id|ruta> [--monitor PANTALLA]
                                       Aplica un fondo (en todas las pantallas o en una)
  nekowall next | previous [--monitor PANTALLA]
                                       Siguiente / anterior de la biblioteca
  nekowall restore [--monitor PANTALLA]
                                       Vuelve al fondo anterior
  nekowall monitors                    Pantallas conectadas y el fondo de cada una
  nekowall favorite <id> [--off]       Marca o desmarca favorito
  nekowall remove <id>... [--forget]   Quita de la biblioteca (no borra el archivo); --forget:
                                       el archivo ya se borró (no se marca como ignorado)
  nekowall sync                        Busca fondos nuevos en las carpetas añadidas (y crea
                                       Imágenes/nekowallpapers, la carpeta propia de NekoWall)
  nekowall folders [--forget <ruta>]   Carpetas que se revisan (o deja de revisar una)
  nekowall status                      Backend, capacidades y fondo actual
  nekowall thumbs [--force]            Genera las miniaturas que falten (ffmpeg)
  nekowall probe <ruta>...             Resolución y fps de los vídeos (JSON)
  nekowall optimize <id|ruta> [--width 1920] [--height 1080] [--fps 30] [--replace-in-library]
                                       Vídeo: copia «<nombre> (1080p30).mp4» junto al original.
                                       Imagen: copia recortada a la pantalla «<nombre> (1920x1080).jpg».
                                       Imagen con encuadre: --scale S (o --scale-x SX --scale-y SY para
                                       estirarla) --x X --y Y [--fill blur|black]
                                       [--apply [--monitor PANTALLA]]
                                       (S = píxeles de pantalla por píxel de imagen; X,Y = esquina de la
                                       imagen en la pantalla). Siempre parte del original.
                                       Con --replace-in-library la biblioteca usa la copia (y si era
                                       el fondo puesto, se cambia por la copia)
  nekowall link-info <enlace>          Título, vista previa y formatos de un enlace (JSON; usa yt-dlp)
  nekowall link-preview <enlace>       Vista previa corta en H.264 (para vídeos AV1/H.265)
  nekowall link-download <enlace> --format ID [--convert ALTURA [--reencode]] [--title T] [--out CARPETA]
                                       Descarga ese formato (sin audio) a nekowallpapers
  nekowall fit-source <ruta>           Original de una copia ajustada y su tamaño (JSON)
  nekowall video-settings [--pause-mode 0-3] [--battery on|off] [--battery-level N]
                                       Pausa del vídeo: 0 pantalla completa/maximizada,
                                       1 ventana activa, 2 cualquier ventana, 3 nunca
  nekowall relocate <carpeta-vieja> <carpeta-nueva>
                                       Tras mover/renombrar una carpeta: actualiza las rutas de
                                       la biblioteca (conserva favoritos) y las miniaturas
  nekowall peek                        MATE: muestra u oculta los iconos del escritorio con un vídeo de fondo
  nekowall help

`list`, `status`, `import`, `sync`, `folders`, `monitors` y `video-settings` aceptan --json (formato para la interfaz gráfica).
La interfaz gráfica es el programa aparte `nekowall-gui` (carpeta app/).
Variables: NEKOWALL_DB para elegir la ruta de la base de datos.
";

/// Carpeta de miniaturas: $XDG_CACHE_HOME/nekowall/thumbs (o ~/.cache/...).
fn thumbs_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("nekowall/thumbs")
}

fn thumb_path(id: &str) -> PathBuf {
    thumbs_dir().join(format!("{id}.jpg"))
}

/// La miniatura existe y no es más antigua que el archivo original.
fn thumb_fresh(src: &Path, thumb: &Path) -> bool {
    let (Ok(t), Ok(s)) = (std::fs::metadata(thumb), std::fs::metadata(src)) else {
        return false;
    };
    let newer = match (t.modified(), s.modified()) {
        (Ok(tm), Ok(sm)) => tm >= sm,
        _ => true,
    };
    newer && t.len() > 0
}

fn entry_json(e: &Entry) -> serde_json::Value {
    let w = &e.wallpaper;
    let name = w.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let size = std::fs::metadata(&w.path).map(|m| m.len()).unwrap_or(0);
    let meta = std::fs::metadata(&w.path).ok();
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let thumb = thumb_path(&w.id);
    serde_json::json!({
        "id": w.id,
        "path": w.path.to_string_lossy(),
        "name": name,
        "kind": w.kind.as_str(),
        "favorite": e.favorite,
        "missing": e.missing,
        "size": size,
        "mtime": mtime,
        "thumb": if thumb.exists() { thumb.to_string_lossy().into_owned() } else { String::new() },
    })
}

/// Una miniatura con ffmpeg: 640 px de ancho máximo, JPEG.
/// En vídeos toma el fotograma del segundo 1 y, si el vídeo es más corto, el primero.
fn make_thumb(e: &Entry, out: &Path) -> Result<(), String> {
    let tmp = out.with_extension("tmp.jpg");
    let run = |seek: bool| -> Result<bool, String> {
        let mut c = Command::new("ffmpeg");
        c.args(["-v", "error", "-y", "-nostdin"]);
        if seek {
            c.args(["-ss", "1"]);
        }
        c.arg("-i").arg(&e.wallpaper.path);
        c.args(["-frames:v", "1", "-vf", "scale='min(640,iw)':-2", "-q:v", "4"]);
        c.arg(&tmp);
        let o = c.output().map_err(|err| format!("no se pudo ejecutar ffmpeg: {err}"))?;
        let ok = o.status.success() && std::fs::metadata(&tmp).map(|m| m.len() > 0).unwrap_or(false);
        if !ok && !seek {
            return Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("falló ffmpeg").to_string());
        }
        Ok(ok)
    };
    let is_video = e.wallpaper.kind == WallpaperKind::Video;
    if !(is_video && run(true)?) {
        run(false)?;
    }
    std::fs::rename(&tmp, out).map_err(|err| err.to_string())
}

/// Convierte un vídeo y, si se pide, hace que la biblioteca use la copia en lugar del original.
/// Última línea de stdout: JSON con el resultado (las anteriores son «encoder …» / «progress N»).
/// Original de una copia ajustada (se recuerda al crearla), o el propio archivo.
fn fit_source(lib: &Library, path: &Path) -> PathBuf {
    lib.state(&format!("fit_source@{}", path.display()))
        .ok()
        .flatten()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .unwrap_or_else(|| path.to_path_buf())
}

fn cmd_optimize(svc: &mut Service, rest: &[String]) -> Result<(), String> {
    let target = rest.iter().find(|a| !a.starts_with("--") && opt_value_owner(rest, a)).ok_or("falta el id o la ruta")?;
    let height: u32 = opt(rest, "--height").unwrap_or("1080").parse().map_err(|_| "--height inválido")?;
    let width: u32 = opt(rest, "--width").unwrap_or("1920").parse().map_err(|_| "--width inválido")?;
    let fps: u32 = opt(rest, "--fps").unwrap_or("30").parse().map_err(|_| "--fps inválido")?;
    let replace = rest.iter().any(|a| a == "--replace-in-library");
    let num = |name: &str| -> Result<Option<f64>, String> {
        opt(rest, name).map(|v| v.parse::<f64>().map_err(|_| format!("{name} inválido"))).transpose()
    };
    let (scale, x, y) = (num("--scale")?, num("--x")?, num("--y")?);
    // Ancho y alto por separado (imagen estirada); --scale vale para los dos.
    let (scale_x, scale_y) = (num("--scale-x")?.or(scale), num("--scale-y")?.or(scale));
    let fill = match opt(rest, "--fill").unwrap_or("blur") {
        "black" | "negro" => media::Fill::Black,
        _ => media::Fill::Blur,
    };

    // `target`: el archivo que el usuario tiene en la biblioteca (puede ser una copia ya ajustada).
    let target = if Path::new(target).exists() {
        std::fs::canonicalize(target).map_err(|e| e.to_string())?
    } else {
        svc.library.get(target).map_err(|e| e.to_string())?.ok_or(format!("id o ruta desconocida: {target}"))?.wallpaper.path
    };
    let is_image = WallpaperKind::from_path(&target) == Some(WallpaperKind::Static);
    // Las imágenes se ajustan siempre desde el original: recortar una copia perdería calidad.
    let source = if is_image { fit_source(&svc.library, &target) } else { target.clone() };

    let conv = if is_image {
        let frame = match (scale_x, scale_y) {
            (Some(sx), Some(sy)) => Some(media::Frame {
                sw: width, sh: height, sx, sy, x: x.unwrap_or(0.0), y: y.unwrap_or(0.0), fill,
            }),
            (None, None) => None,
            _ => return Err("faltan --scale-x o --scale-y (o usa --scale para los dos)".into()),
        };
        let own = default_folder(&mut svc.library);
        let c = media::fit_image(&source, width, height, own.as_deref(), frame)?;
        let out = std::fs::canonicalize(&c.out).unwrap_or_else(|_| c.out.clone());
        svc.library.set_state(&format!("fit_source@{}", out.display()), &source.to_string_lossy())
            .map_err(|e| e.to_string())?;
        c
    } else {
        media::convert(&source, height, fps)?
    };

    // Pantallas que muestran ese fondo ahora mismo: al terminar se les pone la copia.
    let showing: Vec<Option<String>> = match svc.status() {
        Ok(s) if !s.monitors.is_empty() => s
            .monitors
            .iter()
            .filter(|(_, w)| w.as_ref().is_some_and(|w| w.path == target))
            .map(|(m, _)| Some(m.name.clone()))
            .collect(),
        Ok(s) if s.current.as_ref().is_some_and(|w| w.path == target) => vec![None],
        _ => Vec::new(),
    };

    let mut new_id = String::new();
    let mut reapplied = 0;
    if replace {
        let old = svc.library.get_by_path(&target).map_err(|e| e.to_string())?;
        svc.library.import_path(&conv.out).map_err(|e| e.to_string())?;
        let new = svc.library.get_by_path(&std::fs::canonicalize(&conv.out).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .ok_or("la copia no quedó en la biblioteca")?;
        if let Some(old) = old {
            if old.favorite {
                svc.library.set_favorite(&new.wallpaper.id, true).map_err(|e| e.to_string())?;
            }
            svc.library.remove(&old.wallpaper.id).map_err(|e| e.to_string())?;
        }
        new_id = new.wallpaper.id.clone();
        for m in &showing {
            if svc.apply_target_on(&new_id, m.as_deref()).is_ok() {
                reapplied += 1;
            }
        }
        // «Guardar y aplicar»: ponerla de fondo (en --monitor o en todas) si no lo era ya.
        if rest.iter().any(|a| a == "--apply") && showing.is_empty() {
            svc.apply_target_on(&new_id, opt(rest, "--monitor")).map_err(|e| e.to_string())?;
            reapplied += 1;
        }
        // La copia anterior (hecha por NekoWall, nunca el original) ya no hace falta.
        if is_image && target != source {
            let _ = std::fs::remove_file(&target);
            let _ = std::fs::remove_file(thumb_path(&nekowall_backend::id_for_path(&target)));
        }
    }
    println!("{}", serde_json::json!({
        "ok": true, "source": source.to_string_lossy(), "out": conv.out.to_string_lossy(),
        "id": new_id, "encoder": conv.encoder, "reused": conv.reused, "reapplied": reapplied,
    }));
    Ok(())
}

/// `true` si `a` no es el valor de una opción de link-download.
fn link_value_owner(rest: &[String], a: &String) -> bool {
    let i = rest.iter().position(|x| std::ptr::eq(x, a)).unwrap_or(0);
    !(i > 0 && matches!(rest[i - 1].as_str(), "--format" | "--convert" | "--title" | "--out"))
}

/// `true` si `a` no es el valor de una opción (p. ej. el «1080» de «--height 1080»).
fn opt_value_owner(rest: &[String], a: &String) -> bool {
    let i = rest.iter().position(|x| std::ptr::eq(x, a)).unwrap_or(0);
    !(i > 0 && matches!(rest[i - 1].as_str(), "--height" | "--width" | "--fps" | "--scale" | "--scale-x" | "--scale-y" | "--x" | "--y" | "--fill" | "--monitor"))
}

fn cmd_thumbs(lib: &Library, force: bool) -> Result<(), String> {
    let dir = thumbs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let todo: Vec<Entry> = lib
        .list(&Filter::default())
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|e| !e.missing && (force || !thumb_fresh(&e.wallpaper.path, &thumb_path(&e.wallpaper.id))))
        .collect();
    let total = lib.counts().map_err(|e| e.to_string())?.total;
    let workers = std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let errors = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(e) = todo.get(i) else { break };
                if let Err(err) = make_thumb(e, &thumb_path(&e.wallpaper.id)) {
                    errors.lock().unwrap().push(format!("{}: {err}", e.wallpaper.path.display()));
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    println!(
        "Miniaturas: {} nuevas · {} ya estaban · {} errores",
        todo.len() - errors.len(),
        total - todo.len(),
        errors.len()
    );
    for e in &errors {
        eprintln!("  {e}");
    }
    Ok(())
}

/// Backend según el escritorio (`XDG_CURRENT_DESKTOP`); `NEKOWALL_BACKEND=mate|plasma` lo fuerza.
fn detect_backend() -> Box<dyn WallpaperBackend> {
    match std::env::var("NEKOWALL_BACKEND").ok().as_deref() {
        Some("mate") => return Box::new(MateBackend::new()),
        Some("plasma") => return Box::new(PlasmaBackend::new()),
        _ => {}
    }
    if MateBackend::is_current_desktop() {
        Box::new(MateBackend::new())
    } else {
        Box::new(PlasmaBackend::new())
    }
}

fn db_path() -> PathBuf {
    if let Some(p) = std::env::var_os("NEKOWALL_DB") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("nekowall/library.db")
}

fn on_monitor(m: Option<&str>) -> String {
    m.map(|m| format!(" (pantalla {m})")).unwrap_or_default()
}

fn monitor_json(m: &nekowall_backend::Monitor, w: Option<&nekowall_backend::Wallpaper>) -> serde_json::Value {
    serde_json::json!({
        "name": m.name, "x": m.x, "y": m.y, "width": m.width, "height": m.height, "primary": m.primary,
        "current": w.map(|w| serde_json::json!({ "id": w.id, "path": w.path.to_string_lossy(), "kind": w.kind.as_str() })),
    })
}

/// Carpeta de imágenes del usuario (XDG_PICTURES_DIR de ~/.config/user-dirs.dirs; si no, ~/Pictures).
fn pictures_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    if let Ok(text) = std::fs::read_to_string(cfg.join("user-dirs.dirs")) {
        for line in text.lines() {
            let Some(v) = line.trim().strip_prefix("XDG_PICTURES_DIR=") else { continue };
            let v = v.trim().trim_matches('"');
            let p = match v.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(v),
            };
            if p.is_absolute() && p != home {
                return p;
            }
        }
    }
    home.join("Pictures")
}

/// Carpeta propia de NekoWall: «nekowallpapers» dentro de Imágenes. Se crea y se vigila sola; lo que
/// se copia ahí aparece en la biblioteca. Si el usuario deja de vigilarla, no se vuelve a crear.
fn default_folder(lib: &mut Library) -> Option<PathBuf> {
    const KEY: &str = "default_folder";
    let dir = match lib.state(KEY).ok().flatten() {
        Some(p) => {
            let p = PathBuf::from(p);
            if !p.is_dir() && lib.folders().ok()?.contains(&p) {
                std::fs::create_dir_all(&p).ok()?;
            }
            p
        }
        None => {
            let p = pictures_dir().join("nekowallpapers");
            std::fs::create_dir_all(&p).ok()?;
            let p = std::fs::canonicalize(&p).ok()?;
            lib.import_path(&p).ok()?;   // la registra como carpeta vigilada
            lib.set_state(KEY, &p.to_string_lossy()).ok()?;
            p
        }
    };
    Some(dir)
}

/// Fondos oficiales de neko-void (paquete «neko-backgrounds» en /usr/share/backgrounds). Se vinculan
/// como carpeta vigilada una sola vez y solo en neko-void (se reconoce por el archivo de MATE que trae
/// el paquete): en otro sistema esa carpeta puede tener cualquier cosa. Si el usuario la quita, no
/// vuelve. Los archivos son del sistema: NekoWall solo los lee.
fn official_folder(lib: &mut Library) {
    link_official(
        lib,
        Path::new("/usr/share/mate-background-properties/neko.xml"),
        Path::new("/usr/share/backgrounds"),
    );
}

fn link_official(lib: &mut Library, marker: &Path, dir: &Path) {
    const KEY: &str = "official_folder";
    if lib.state(KEY).ok().flatten().is_some() || !marker.is_file() || !dir.is_dir() {
        return;
    }
    let Ok(dir) = std::fs::canonicalize(dir) else { return };
    if lib.import_path(&dir).is_ok() {
        let _ = lib.set_state(KEY, &dir.to_string_lossy());
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    if matches!(cmd, "help" | "-h" | "--help") {
        print!("{HELP}");
        return Ok(());
    }
    if matches!(cmd, "gui" | "hud") {
        return Err(format!("usa el programa `nekowall-gui{}` (carpeta app/)", if cmd == "hud" { " --hud" } else { "" }));
    }

    // Proceso interno que reproduce y pausa el vídeo de fondo en MATE (lo lanza `apply`).
    if cmd == "_mate-video" {
        // Sin argumento: lee los vídeos de cada pantalla. Con ruta: autostart de versiones anteriores.
        return nekowall_mate::video::run_daemon(args.get(1).map(Path::new));
    }

    // MATE: alterna «mostrar escritorio» (oculta y pausa el vídeo de fondo para ver los iconos de Caja).
    if cmd == "peek" {
        let shown = nekowall_mate::video::toggle_peek()?;
        println!("{}", if shown { "Escritorio visible (vídeo en pausa)" } else { "Vídeo de fondo reanudado" });
        return Ok(());
    }

    let library = Library::open(&db_path()).map_err(|e| e.to_string())?;
    let mut svc = Service::new(library, detect_backend());
    let rest = &args[1..];
    let json = rest.iter().any(|a| a == "--json");
    let rest: Vec<String> = rest.iter().filter(|a| *a != "--json").cloned().collect();
    let rest = &rest[..];

    match cmd {
        "import" => {
            if rest.is_empty() {
                return Err("falta la ruta".into());
            }
            let (mut added, mut dups, mut unsup) = (0, 0, 0);
            let mut errors: Vec<(String, String)> = Vec::new();
            for path in rest {
                match svc.library.import_path(path.as_ref()) {
                    Ok(r) => {
                        added += r.added;
                        dups += r.duplicates;
                        unsup += r.unsupported;
                        errors.extend(r.errors.into_iter().map(|(p, e)| (p.display().to_string(), e)));
                    }
                    Err(e) => errors.push((path.clone(), e.to_string())),
                }
            }
            if json {
                let errs: Vec<_> = errors.iter().map(|(p, e)| serde_json::json!({ "path": p, "error": e })).collect();
                println!("{}", serde_json::json!({
                    "added": added, "duplicates": dups, "unsupported": unsup, "errors": errs,
                }));
                return Ok(());
            }
            println!(
                "Añadidos: {added} · duplicados: {dups} · no soportados: {unsup} · errores: {}",
                errors.len()
            );
            for (p, e) in &errors {
                eprintln!("  {p}: {e}");
            }
        }
        "list" => {
            let mut f = Filter::default();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--video" => f.kind = Some(WallpaperKind::Video),
                    "--static" => f.kind = Some(WallpaperKind::Static),
                    "--favorites" => f.favorites_only = true,
                    "--search" => f.query = Some(it.next().ok_or("falta el texto de --search")?.clone()),
                    other => return Err(format!("opción desconocida: {other}")),
                }
            }
            let entries = svc.library.list(&f).map_err(|e| e.to_string())?;
            if json {
                let arr: Vec<_> = entries.iter().map(entry_json).collect();
                println!("{}", serde_json::Value::Array(arr));
                return Ok(());
            }
            for e in entries {
                println!(
                    "{} {} {:<6} {}{}",
                    e.wallpaper.id,
                    if e.favorite { "★" } else { " " },
                    e.wallpaper.kind.as_str(),
                    e.wallpaper.path.display(),
                    if e.missing { "  [falta el archivo]" } else { "" }
                );
            }
        }
        "apply" => {
            let t = rest.iter().find(|a| !a.starts_with("--") && opt_value_owner(rest, a)).ok_or("falta el id o la ruta")?;
            let w = svc.apply_target_on(t, opt(rest, "--monitor")).map_err(|e| e.to_string())?;
            println!("Aplicado: {}{}", w.path.display(), on_monitor(opt(rest, "--monitor")));
        }
        "next" | "previous" => {
            let m = opt(rest, "--monitor");
            let w = svc.step(if cmd == "next" { 1 } else { -1 }, m).map_err(|e| e.to_string())?;
            println!("Aplicado: {}{}", w.path.display(), on_monitor(m));
        }
        "restore" => {
            let m = opt(rest, "--monitor");
            let w = svc.restore_previous_on(m).map_err(|e| e.to_string())?;
            println!("Restaurado: {}{}", w.path.display(), on_monitor(m));
        }
        "monitors" => {
            let s = svc.status().map_err(|e| e.to_string())?;
            if json {
                let arr: Vec<_> = s.monitors.iter().map(|(m, w)| monitor_json(m, w.as_ref())).collect();
                println!("{}", serde_json::Value::Array(arr));
                return Ok(());
            }
            if s.monitors.is_empty() {
                println!("Este escritorio no informa de sus pantallas.");
            }
            for (m, w) in &s.monitors {
                println!(
                    "{}{}  {}×{} en ({}, {})  {}",
                    m.name,
                    if m.primary { " (principal)" } else { "" },
                    m.width, m.height, m.x, m.y,
                    w.as_ref().map(|w| w.path.display().to_string()).unwrap_or_else(|| "—".into())
                );
            }
        }
        "favorite" => {
            let id = rest.first().ok_or("falta el id")?;
            let on = !rest.iter().any(|a| a == "--off");
            if !svc.library.set_favorite(id, on).map_err(|e| e.to_string())? {
                return Err(format!("id desconocido: {id}"));
            }
            println!("{} favorito", if on { "Marcado como" } else { "Quitado de" });
        }
        "remove" => {
            // Varios ids a la vez. --forget: el archivo ya se borró (p. ej. a la papelera): no se marca
            // como ignorado, así si el usuario lo recupera vuelve a aparecer.
            let forget = rest.iter().any(|a| a == "--forget");
            let ids: Vec<&String> = rest.iter().filter(|a| !a.starts_with("--")).collect();
            if ids.is_empty() {
                return Err("falta el id".into());
            }
            let mut unknown = Vec::new();
            // Con --forget (archivo ya en la papelera): si era una copia ajustada por NekoWall, su
            // original oculto también sobra; se devuelve para que la interfaz lo mande a la papelera.
            let mut originals: Vec<String> = Vec::new();
            for id in &ids {
                if forget {
                    if let Ok(Some(e)) = svc.library.get(id) {
                        let src = fit_source(&svc.library, &e.wallpaper.path);
                        if src != e.wallpaper.path && svc.library.is_ignored(&src).unwrap_or(false) {
                            originals.push(src.to_string_lossy().into_owned());
                        }
                    }
                }
                let ok = if forget { svc.library.forget(id) } else { svc.library.remove(id) };
                if !ok.map_err(|e| e.to_string())? {
                    unknown.push(id.as_str());
                }
            }
            if !unknown.is_empty() {
                return Err(format!("id desconocido: {}", unknown.join(", ")));
            }
            if json {
                println!("{}", serde_json::json!({ "removed": ids.len(), "originals": originals }));
            } else if forget {
                println!("Quitados de la biblioteca: {}", ids.len());
            } else {
                println!("Quitado de la biblioteca (el archivo original no se toca): {}", ids.len());
            }
        }
        "sync" => {
            let own = default_folder(&mut svc.library);
            official_folder(&mut svc.library);
            let r = svc.library.sync_folders().map_err(|e| e.to_string())?;
            if json {
                let dirs: Vec<_> = r.dirs.iter().map(|d| d.to_string_lossy()).collect();
                let own = own.map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                println!("{}", serde_json::json!({ "added": r.added, "recent": r.recent, "removed": r.removed, "missing": r.missing, "dirs": dirs, "default_dir": own }));
            } else {
                println!("Fondos nuevos: {} · quitados (borrados de la carpeta): {} · archivos que faltan: {}", r.added, r.removed, r.missing);
            }
        }
        "folders" => {
            if let Some(dir) = opt(rest, "--forget") {
                let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| PathBuf::from(dir));
                if !svc.library.forget_folder(&dir).map_err(|e| e.to_string())? {
                    return Err(format!("esa carpeta no se estaba revisando: {}", dir.display()));
                }
                println!("Ya no se revisa {} (sus fondos siguen en la biblioteca)", dir.display());
                return Ok(());
            }
            let folders = svc.library.folders().map_err(|e| e.to_string())?;
            if json {
                let arr: Vec<_> = folders.iter().map(|d| d.to_string_lossy()).collect();
                println!("{}", serde_json::json!(arr));
            } else if folders.is_empty() {
                println!("No hay carpetas: añade una con «nekowall import <carpeta>».");
            } else {
                for d in folders {
                    println!("{}", d.display());
                }
            }
        }
        "status" => {
            let missing = svc.library.refresh_missing().map_err(|e| e.to_string())?;
            let s = svc.status().map_err(|e| e.to_string())?;
            let c = s.capabilities;
            if json {
                let cur = s.current.as_ref().map(|w| {
                    serde_json::json!({ "id": w.id, "path": w.path.to_string_lossy(), "kind": w.kind.as_str() })
                });
                let v = serde_json::json!({
                    "backend": s.backend,
                    "kde": PlasmaBackend::is_current_desktop(),
                    "db": db_path().to_string_lossy(),
                    "thumbs_dir": thumbs_dir().to_string_lossy(),
                    "capabilities": {
                        "static": c.static_wallpapers, "video": c.video_wallpapers,
                        "per_monitor": c.per_monitor, "pause": c.pause_video,
                    },
                    "current": cur,
                    "monitors": s.monitors.iter().map(|(m, w)| monitor_json(m, w.as_ref())).collect::<Vec<_>>(),
                    "counts": { "total": s.counts.total, "favorites": s.counts.favorites, "missing": missing },
                });
                println!("{v}");
                return Ok(());
            }
            println!("Backend: {} (en KDE: {})", s.backend, PlasmaBackend::is_current_desktop());
            println!(
                "Capacidades: estáticos={} vídeo={} por-monitor={} pausa={}",
                c.static_wallpapers, c.video_wallpapers, c.per_monitor, c.pause_video
            );
            match s.current {
                Some(w) => println!("Fondo actual: {}", w.path.display()),
                None => println!("Fondo actual: (desconocido)"),
            }
            println!(
                "Biblioteca: {} fondos · {} favoritos · {} sin archivo",
                s.counts.total, s.counts.favorites, missing
            );
        }
        "thumbs" => cmd_thumbs(&svc.library, rest.iter().any(|a| a == "--force"))?,
        "probe" => {
            let paths: Vec<String> = rest.iter().filter(|a| !a.starts_with("--")).cloned().collect();
            let arr: Vec<serde_json::Value> = media::collect_videos(&paths)
                .iter()
                .map(|p| match media::probe(p) {
                    Ok(i) => i.to_json(p),
                    Err(e) => serde_json::json!({ "path": p.to_string_lossy(), "error": e }),
                })
                .collect();
            println!("{}", serde_json::Value::Array(arr));
        }
        "optimize" => cmd_optimize(&mut svc, rest)?,
        "link-info" => {
            let url = rest.iter().find(|a| !a.starts_with("--")).ok_or("falta el enlace")?;
            println!("{}", link::info(url)?);
        }
        "link-preview" => {
            let url = rest.iter().find(|a| !a.starts_with("--")).ok_or("falta el enlace")?;
            let out = thumbs_dir().parent().map(|p| p.join("vista-previa.mp4")).unwrap_or_else(|| std::env::temp_dir().join("nekowall-vista-previa.mp4"));
            let path = link::make_preview(url, &out)?;
            println!("{}", serde_json::json!({ "path": path.to_string_lossy() }));
        }
        "link-download" => {
            let url = rest.iter().find(|a| !a.starts_with("--") && link_value_owner(rest, a)).ok_or("falta el enlace")?;
            let format = opt(rest, "--format").unwrap_or("directo");
            let convert: u32 = opt(rest, "--convert").unwrap_or("0").parse().map_err(|_| "--convert inválido")?;
            let title = opt(rest, "--title").unwrap_or("fondo");
            let out = match opt(rest, "--out") {
                Some(d) => PathBuf::from(d),
                None => default_folder(&mut svc.library).ok_or("no encuentro la carpeta nekowallpapers")?,
            };
            let tmp = thumbs_dir().parent().map(|p| p.join("descargas")).unwrap_or_else(|| std::env::temp_dir().join("nekowall-descargas"));
            let reencode = rest.iter().any(|a| a == "--reencode");
            let path = link::download(url, format, convert, reencode, title, &out, &tmp)?;
            println!("{}", serde_json::json!({ "path": path.to_string_lossy() }));
        }
        "fit-source" => {
            let p = rest.first().ok_or("falta la ruta")?;
            let p = std::fs::canonicalize(p).map_err(|e| format!("{p}: {e}"))?;
            let src = fit_source(&svc.library, &p);
            let info = media::probe(&src)?;
            println!("{}", serde_json::json!({
                "source": src.to_string_lossy(), "width": info.width, "height": info.height,
            }));
        }
        "relocate" => {
            let (Some(old), Some(new)) = (rest.first(), rest.get(1)) else {
                return Err("uso: nekowall relocate <carpeta-vieja> <carpeta-nueva>".into());
            };
            // La carpeta vieja ya no existe (se movió): solo se normaliza la nueva.
            let new = std::fs::canonicalize(new).map_err(|e| format!("{new}: {e}"))?;
            let old = PathBuf::from(old.trim_end_matches('/'));
            let moved = svc.library.relocate(&old, &new).map_err(|e| e.to_string())?;
            let mut thumbs = 0;
            for (o, n) in &moved {
                if std::fs::rename(thumb_path(o), thumb_path(n)).is_ok() {
                    thumbs += 1;
                }
            }
            println!("Rutas actualizadas: {} · miniaturas conservadas: {thumbs}", moved.len());
        }
        "video-settings" => {
            let mate = svc.backend_name() == "mate";
            if !mate && svc.backend_name() != "plasma" {
                return Err(format!(
                    "las opciones de pausa del vídeo no existen en este escritorio ({})",
                    svc.backend_name()
                ));
            }
            let plasma = PlasmaBackend::new();
            let m = nekowall_mate::video::load_settings();
            let mut s = if mate {
                nekowall_plasma::VideoPauseSettings {
                    pause_mode: m.pause_mode, battery_pauses: m.battery_pauses, battery_level: m.battery_level,
                }
            } else {
                plasma.video_pause_settings()
            };
            let mut changed = false;
            if let Some(v) = opt(rest, "--pause-mode") {
                s.pause_mode = v.parse().map_err(|_| format!("--pause-mode inválido: {v}"))?;
                changed = true;
            }
            if let Some(v) = opt(rest, "--battery") {
                s.battery_pauses = match v {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    _ => return Err(format!("--battery espera on u off, no «{v}»")),
                };
                changed = true;
            }
            if let Some(v) = opt(rest, "--battery-level") {
                s.battery_level = v.parse().map_err(|_| format!("--battery-level inválido: {v}"))?;
                changed = true;
            }
            if changed {
                if mate {
                    if s.pause_mode > 3 || s.battery_level > 100 {
                        return Err(format!("valores fuera de rango: modo {} (0-3), batería {} % (0-100)", s.pause_mode, s.battery_level));
                    }
                    nekowall_mate::video::save_settings(&nekowall_mate::video::MateVideoSettings {
                        pause_mode: s.pause_mode, battery_pauses: s.battery_pauses, battery_level: s.battery_level,
                    }).map_err(|e| e.to_string())?;
                } else {
                    plasma.set_video_pause_settings(&s).map_err(|e| e.to_string())?;
                }
            }
            if json {
                println!("{}", serde_json::json!({
                    "pause_mode": s.pause_mode, "battery_pauses": s.battery_pauses,
                    "battery_level": s.battery_level,
                    "plugin": if mate { MateBackend::new().video_available() } else { plasma.video_plugin_installed() },
                }));
            } else {
                const MODES: [&str; 4] = ["ventana maximizada o a pantalla completa", "ventana activa",
                                          "cualquier ventana visible", "nunca"];
                println!("Pausar el vídeo con: {}", MODES.get(s.pause_mode as usize).unwrap_or(&"?"));
                println!("Pausar con batería baja: {} (por debajo del {} %)",
                         if s.battery_pauses { "sí" } else { "no" }, s.battery_level);
            }
        }
        other => return Err(format!("comando desconocido: {other} (prueba `nekowall help`)")),
    }
    Ok(())
}

// Evita warning si el trait no se usa directamente.
#[allow(dead_code)]
fn _assert_backend<T: WallpaperBackend>() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_folder_only_on_neko_void_and_only_once() {
        let base = std::env::temp_dir().join(format!("nekowall-cli-oficial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("backgrounds");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("neko.png"), b"x").unwrap();
        let marker = base.join("neko.xml");
        let mut lib = Library::open_in_memory().unwrap();

        // Sin la marca de neko-void no se vincula nada.
        link_official(&mut lib, &marker, &dir);
        assert!(lib.folders().unwrap().is_empty());

        // En neko-void sí, con sus fondos.
        std::fs::write(&marker, b"<wallpapers/>").unwrap();
        link_official(&mut lib, &marker, &dir);
        let canon = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(lib.folders().unwrap(), vec![canon.clone()]);
        assert_eq!(lib.list(&Filter::default()).unwrap().len(), 1);

        // Si el usuario la quita, no vuelve.
        lib.forget_folder(&canon).unwrap();
        link_official(&mut lib, &marker, &dir);
        assert!(lib.folders().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
