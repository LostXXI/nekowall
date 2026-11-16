//! Backend MATE (X11).
//!
//! Imágenes: clave `org.mate.background picture-filename` de gsettings; Caja la lee y redibuja el
//! escritorio al momento. Comprobado en neko-void MATE (2026-11): la sesión es X11, Caja dibuja el
//! escritorio y Marco es el gestor de ventanas.
//! Vídeo: `xwinwrap` + `mpv` detrás de las ventanas y un proceso propio que pausa (ver `video.rs`).

use nekowall_backend::{
    BackendCapabilities, Wallpaper, WallpaperBackend, WallpaperError, WallpaperKind,
};
use std::io;
use std::path::Path;
use std::process::{Command, Output};

pub mod video;

const SCHEMA: &str = "org.mate.background";

/// Abstracción de ejecución de comandos para poder probar sin MATE.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output>;
}

pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        Command::new(program).args(args).output()
    }
}

pub struct MateBackend<R: CommandRunner = SystemRunner> {
    runner: R,
    /// Hay `mpv` y `xwinwrap`: se pueden poner vídeos (desactivado en los tests).
    video: bool,
}

fn in_path(prog: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
        .unwrap_or(false)
}

impl MateBackend<SystemRunner> {
    pub fn new() -> Self {
        Self { runner: SystemRunner, video: in_path("mpv") && in_path("xwinwrap") }
    }

    /// `XDG_CURRENT_DESKTOP` contiene «MATE».
    pub fn is_current_desktop() -> bool {
        std::env::var("XDG_CURRENT_DESKTOP")
            .map(|v| v.split(':').any(|p| p.eq_ignore_ascii_case("MATE")))
            .unwrap_or(false)
    }
}

impl Default for MateBackend<SystemRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: CommandRunner> MateBackend<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner, video: false }
    }

    pub fn video_available(&self) -> bool {
        self.video
    }

    /// Deja de reproducir el vídeo (si lo hay) y devuelve el escritorio de Caja a como estaba.
    fn stop_video(&self) -> Result<(), WallpaperError> {
        if !self.video {
            return Ok(());
        }
        video::stop();
        video::remember_video(None)?;
        video::set_autostart(None, None)?;
        if let Some(prev) = video::take_saved_icons() {
            self.gsettings(&["set", SCHEMA, "show-desktop-icons", &prev])?;
        }
        Ok(())
    }

    fn apply_video(&self, w: &Wallpaper) -> Result<(), WallpaperError> {
        let exe = std::env::current_exe()?;
        video::stop();
        video::spawn(&exe, &w.path)?;
        // Versiones anteriores apagaban el escritorio de Caja: si quedó apagado por NekoWall, se restaura
        // (ahora Caja sigue activo debajo del vídeo y recibe los clics).
        if let Some(prev) = video::take_saved_icons() {
            self.gsettings(&["set", SCHEMA, "show-desktop-icons", &prev])?;
        }
        video::remember_video(Some(&w.path))?;
        video::set_autostart(Some(&exe), Some(&w.path))?;
        // Debajo del vídeo, Caja muestra un fotograma del mismo vídeo: al «mostrar escritorio» (Meta+Alt+D)
        // el cambio es suave. Si ffmpeg falla, se queda la imagen que hubiera.
        if let Some(frame) = video::still_frame(&w.path) {
            let _ = self.gsettings(&["set", SCHEMA, "picture-filename", &gvariant_string(&frame.to_string_lossy())]);
        }
        Ok(())
    }

    fn gsettings(&self, args: &[&str]) -> Result<String, WallpaperError> {
        let out = self.runner.run("gsettings", args).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                WallpaperError::BackendUnavailable("no se encontró `gsettings` (paquete glib)".into())
            } else {
                WallpaperError::Io(e)
            }
        })?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(WallpaperError::CommandFailed {
                command: format!("gsettings {}", args.join(" ")),
                detail: if err.is_empty() { "sin salida (¿hay sesión de MATE?)".into() } else { err },
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

/// Texto → cadena GVariant entre comillas simples (escapando `\` y `'`), para que gsettings
/// nunca interprete la ruta como otro tipo de valor.
fn gvariant_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// Inversa de `gvariant_string` para lo que imprime `gsettings get` (`'…'` o `"…"`).
fn parse_gvariant_string(s: &str) -> Option<String> {
    let s = s.trim();
    let q = s.chars().next()?;
    if (q != '\'' && q != '"') || s.len() < 2 || !s.ends_with(q) {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

impl<R: CommandRunner> WallpaperBackend for MateBackend<R> {
    fn name(&self) -> &'static str {
        "mate"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            static_wallpapers: true,
            video_wallpapers: self.video,
            pause_video: self.video,
            ..Default::default()
        }
    }

    fn apply(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), WallpaperError> {
        if monitor.is_some() {
            return Err(WallpaperError::Unsupported("MATE usa el mismo fondo en todos los monitores".into()));
        }
        if wallpaper.kind == WallpaperKind::Video && !self.video {
            return Err(WallpaperError::Unsupported(
                "para fondos de vídeo en MATE instala `mpv` y `xwinwrap` (xbps-install mpv xwinwrap)".into(),
            ));
        }
        if !wallpaper.path.is_file() {
            return Err(WallpaperError::NotFound(wallpaper.path.clone()));
        }
        if wallpaper.kind == WallpaperKind::Video {
            return self.apply_video(wallpaper);
        }
        self.stop_video()?;
        let path = wallpaper.path.to_string_lossy();
        // Si el usuario tenía «sin imagen», Caja no mostraría nada: forzar un modo con imagen.
        let options = self.gsettings(&["get", SCHEMA, "picture-options"]).unwrap_or_default();
        if options.trim_matches('\'') == "none" {
            self.gsettings(&["set", SCHEMA, "picture-options", "zoom"])?;
        }
        self.gsettings(&["set", SCHEMA, "draw-background", "true"])?;
        self.gsettings(&["set", SCHEMA, "picture-filename", &gvariant_string(&path)])?;
        Ok(())
    }

    fn current(&self, _monitor: Option<&str>) -> Result<Option<Wallpaper>, WallpaperError> {
        if self.video {
            if let Some(v) = video::current_video() {
                return Ok(Wallpaper::from_path(&v));
            }
        }
        let raw = self.gsettings(&["get", SCHEMA, "picture-filename"])?;
        Ok(parse_gvariant_string(&raw)
            .filter(|p| !p.is_empty())
            .and_then(|p| Wallpaper::from_path(Path::new(&p))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::path::PathBuf;
    use std::process::ExitStatus;
    use std::sync::Mutex;

    /// Simula gsettings: guarda las llamadas y responde a `get` con un valor fijo.
    struct Fake {
        get_reply: String,
        calls: Mutex<Vec<Vec<String>>>,
    }
    impl Fake {
        fn new(get_reply: &str) -> Self {
            Self { get_reply: get_reply.into(), calls: Mutex::new(vec![]) }
        }
    }
    impl CommandRunner for &Fake {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
            assert_eq!(program, "gsettings");
            self.calls.lock().unwrap().push(args.iter().map(|s| s.to_string()).collect());
            let stdout = if args.first() == Some(&"get") { self.get_reply.clone() } else { String::new() };
            Ok(Output { status: ExitStatus::from_raw(0), stdout: stdout.into_bytes(), stderr: vec![] })
        }
    }

    fn tmpfile(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nw-mate-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn gvariant_round_trip_with_quotes_and_spaces() {
        for s in ["/a/b.png", "/home/u/Mis fondos/l'été.jpg", r"/raro\nombre.png"] {
            assert_eq!(parse_gvariant_string(&gvariant_string(s)).as_deref(), Some(s));
        }
        assert_eq!(gvariant_string("/x/it's.png"), r"'/x/it\'s.png'");
        assert_eq!(parse_gvariant_string("\"/a/b.png\"").as_deref(), Some("/a/b.png"));
        assert_eq!(parse_gvariant_string("@ms nothing"), None);
    }

    #[test]
    fn applies_image_via_gsettings() {
        let f = Fake::new("'zoom'");
        let img = tmpfile("fondo de prueba.png");
        let w = Wallpaper::from_path(&img).unwrap();
        MateBackend::with_runner(&f).apply(&w, None).unwrap();
        let calls = f.calls.lock().unwrap();
        let last = calls.last().unwrap();
        assert_eq!(last[..3], ["set", SCHEMA, "picture-filename"]);
        assert_eq!(parse_gvariant_string(&last[3]).unwrap(), img.to_string_lossy());
        // Con 'zoom' no se toca picture-options.
        assert!(!calls.iter().any(|c| c[0] == "set" && c[2] == "picture-options"));
    }

    #[test]
    fn forces_zoom_when_picture_options_is_none() {
        let f = Fake::new("'none'");
        let w = Wallpaper::from_path(&tmpfile("a.png")).unwrap();
        MateBackend::with_runner(&f).apply(&w, None).unwrap();
        assert!(f.calls.lock().unwrap().iter().any(|c| c[..] == ["set", SCHEMA, "picture-options", "zoom"]));
    }

    #[test]
    fn rejects_video_missing_file_and_monitor() {
        let f = Fake::new("'zoom'");
        let b = MateBackend::with_runner(&f);
        let vid = Wallpaper::from_path(&tmpfile("v.mp4")).unwrap();
        assert!(matches!(b.apply(&vid, None), Err(WallpaperError::Unsupported(_))));
        let gone = Wallpaper::from_path(Path::new("/no/existe.png")).unwrap();
        assert!(matches!(b.apply(&gone, None), Err(WallpaperError::NotFound(_))));
        let img = Wallpaper::from_path(&tmpfile("b.png")).unwrap();
        assert!(b.apply(&img, Some("HDMI-1")).is_err());
        assert!(f.calls.lock().unwrap().iter().all(|c| c[0] == "get"), "no debe escribir nada");
    }

    #[test]
    fn current_reads_picture_filename() {
        let f = Fake::new("'/usr/share/backgrounds/moony-art3.png'");
        let w = MateBackend::with_runner(&f).current(None).unwrap().unwrap();
        assert_eq!(w.path, PathBuf::from("/usr/share/backgrounds/moony-art3.png"));
        assert_eq!(w.kind, WallpaperKind::Static);
        assert!(MateBackend::with_runner(&Fake::new("''")).current(None).unwrap().is_none());
    }
}
