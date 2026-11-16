use nekowall_backend::{WallpaperBackend, WallpaperKind};
use nekowall_core::Service;
use nekowall_library::{Entry, Filter, Library};
use nekowall_mate::MateBackend;
use nekowall_plasma::PlasmaBackend;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

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
  nekowall apply <id|ruta>             Aplica un fondo
  nekowall next | previous             Siguiente / anterior de la biblioteca
  nekowall restore                     Vuelve al fondo anterior
  nekowall favorite <id> [--off]       Marca o desmarca favorito
  nekowall remove <id>                 Quita de la biblioteca (no borra el archivo)
  nekowall status                      Backend, capacidades y fondo actual
  nekowall thumbs [--force]            Genera las miniaturas que falten (ffmpeg)
  nekowall probe <ruta>...             Resolución y fps de los vídeos (JSON)
  nekowall optimize <id|ruta> [--height 1080] [--fps 30] [--replace-in-library]
                                       Crea una copia «<nombre> (1080p30).mp4» junto al original;
                                       con --replace-in-library la biblioteca usa la copia
  nekowall video-settings [--pause-mode 0-3] [--battery on|off] [--battery-level N]
                                       Pausa del vídeo: 0 pantalla completa/maximizada,
                                       1 ventana activa, 2 cualquier ventana, 3 nunca
  nekowall relocate <carpeta-vieja> <carpeta-nueva>
                                       Tras mover/renombrar una carpeta: actualiza las rutas de
                                       la biblioteca (conserva favoritos) y las miniaturas
  nekowall peek                        MATE: muestra u oculta los iconos del escritorio con un vídeo de fondo
  nekowall help

`list`, `status`, `import` y `video-settings` aceptan --json (formato para la interfaz gráfica).
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
fn cmd_optimize(lib: &mut Library, rest: &[String]) -> Result<(), String> {
    let target = rest.iter().find(|a| !a.starts_with("--") && opt_value_owner(rest, a)).ok_or("falta el id o la ruta")?;
    let height: u32 = opt(rest, "--height").unwrap_or("1080").parse().map_err(|_| "--height inválido")?;
    let fps: u32 = opt(rest, "--fps").unwrap_or("30").parse().map_err(|_| "--fps inválido")?;
    let replace = rest.iter().any(|a| a == "--replace-in-library");

    let src = if Path::new(target).exists() {
        std::fs::canonicalize(target).map_err(|e| e.to_string())?
    } else {
        lib.get(target).map_err(|e| e.to_string())?.ok_or(format!("id o ruta desconocida: {target}"))?.wallpaper.path
    };
    let conv = media::convert(&src, height, fps)?;

    let mut new_id = String::new();
    if replace {
        let old = lib.get_by_path(&src).map_err(|e| e.to_string())?;
        lib.import_path(&conv.out).map_err(|e| e.to_string())?;
        let new = lib.get_by_path(&std::fs::canonicalize(&conv.out).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .ok_or("la copia no quedó en la biblioteca")?;
        if let Some(old) = old {
            if old.favorite {
                lib.set_favorite(&new.wallpaper.id, true).map_err(|e| e.to_string())?;
            }
            lib.remove(&old.wallpaper.id).map_err(|e| e.to_string())?;
        }
        new_id = new.wallpaper.id;
    }
    println!("{}", serde_json::json!({
        "ok": true, "source": src.to_string_lossy(), "out": conv.out.to_string_lossy(),
        "id": new_id, "encoder": conv.encoder, "reused": conv.reused,
    }));
    Ok(())
}

/// `true` si `a` no es el valor de una opción (p. ej. el «1080» de «--height 1080»).
fn opt_value_owner(rest: &[String], a: &String) -> bool {
    let i = rest.iter().position(|x| std::ptr::eq(x, a)).unwrap_or(0);
    !(i > 0 && matches!(rest[i - 1].as_str(), "--height" | "--fps"))
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
        let path = args.get(1).ok_or("falta la ruta del vídeo")?;
        return nekowall_mate::video::run_daemon(Path::new(path));
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
            let t = rest.first().ok_or("falta el id o la ruta")?;
            let w = svc.apply_target(t).map_err(|e| e.to_string())?;
            println!("Aplicado: {}", w.path.display());
        }
        "next" => println!("Aplicado: {}", svc.next().map_err(|e| e.to_string())?.path.display()),
        "previous" => println!("Aplicado: {}", svc.previous().map_err(|e| e.to_string())?.path.display()),
        "restore" => println!("Restaurado: {}", svc.restore_previous().map_err(|e| e.to_string())?.path.display()),
        "favorite" => {
            let id = rest.first().ok_or("falta el id")?;
            let on = !rest.iter().any(|a| a == "--off");
            if !svc.library.set_favorite(id, on).map_err(|e| e.to_string())? {
                return Err(format!("id desconocido: {id}"));
            }
            println!("{} favorito", if on { "Marcado como" } else { "Quitado de" });
        }
        "remove" => {
            let id = rest.first().ok_or("falta el id")?;
            if !svc.library.remove(id).map_err(|e| e.to_string())? {
                return Err(format!("id desconocido: {id}"));
            }
            println!("Quitado de la biblioteca (el archivo original no se toca)");
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
        "optimize" => cmd_optimize(&mut svc.library, rest)?,
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
