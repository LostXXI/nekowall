// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Backend MATE (X11).
//!
//! Imágenes: clave `org.mate.background picture-filename` de gsettings; Caja la lee y redibuja el
//! escritorio al momento. Comprobado en neko-void MATE (2026-11): la sesión es X11, Caja dibuja el
//! escritorio y Marco es el gestor de ventanas.
//! Vídeo: `xwinwrap` + `mpv` detrás de las ventanas y un proceso propio que pausa (ver `video.rs`).
//!
//! Varias pantallas: cada una tiene su fondo (`mon.<pantalla>=<ruta>` en ~/.config/nekowall/mate.conf).
//! Caja solo admite UNA imagen, así que con varias pantallas se compone una imagen
//! del tamaño de todo el escritorio (con ffmpeg), cada fondo recortado a su pantalla, y se muestra con
//! `picture-options = spanned`. Las pantallas con vídeo llevan debajo un fotograma del vídeo.

use nekowall_backend::{
    BackendCapabilities, Monitor, Wallpaper, WallpaperBackend, WallpaperError, WallpaperKind,
};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
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
    /// Carpeta de mate.conf (~/.config/nekowall; otra en los tests).
    dir: PathBuf,
}

fn in_path(prog: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
        .unwrap_or(false)
}

impl MateBackend<SystemRunner> {
    pub fn new() -> Self {
        Self { runner: SystemRunner, video: in_path("mpv") && in_path("xwinwrap"), dir: video::config_dir() }
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
    /// Para los tests: estado en una carpeta temporal propia.
    pub fn with_runner(runner: R) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("nw-mate-state-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        Self { runner, video: false, dir }
    }

    pub fn video_available(&self) -> bool {
        self.video
    }

    /// Pantallas (xrandr). Sin xrandr: una sola llamada «default».
    pub fn monitors(&self) -> Vec<Monitor> {
        let v = self
            .runner
            .run("xrandr", &["--listmonitors"])
            .map(|o| video::parse_listmonitors(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default();
        if v.is_empty() {
            video::monitors()
        } else {
            v
        }
    }

    /// Fondo de cada pantalla. Las que aún no tienen (primera vez, versión anterior, pantalla nueva)
    /// toman el fondo que había, o el de la pantalla principal.
    fn assignments(&self, mons: &[Monitor]) -> BTreeMap<String, PathBuf> {
        let mut a = video::assignments_in(&self.dir);
        if mons.iter().all(|m| a.contains_key(&m.name)) {
            return a;
        }
        let fallback = self
            .legacy_current()
            .or_else(|| mons.iter().find(|m| m.primary).and_then(|m| a.get(&m.name).cloned()))
            .or_else(|| a.values().next().cloned());
        if let Some(f) = fallback {
            for m in mons {
                a.entry(m.name.clone()).or_insert_with(|| f.clone());
            }
        }
        a
    }

    /// Fondo único de las versiones anteriores (antes de recordar uno por pantalla).
    fn legacy_current(&self) -> Option<PathBuf> {
        if self.video && video::running() {
            if let Some(v) = video::legacy_video_in(&self.dir) {
                return Some(v);
            }
        }
        let raw = self.gsettings(&["get", SCHEMA, "picture-filename"]).ok()?;
        let p = PathBuf::from(parse_gvariant_string(&raw)?);
        // Las imágenes que genera NekoWall (fotogramas, composiciones) no son «el fondo».
        let ours = video::cache_dir().is_some_and(|c| p.starts_with(c));
        (!ours && p.as_os_str().len() > 0).then_some(p)
    }

    /// Lo que debe mostrar Caja: el fondo común, o una imagen compuesta si las pantallas difieren.
    fn render(&self, a: &BTreeMap<String, PathBuf>, mons: &[Monitor]) -> Result<(), WallpaperError> {
        let mut stills: Vec<(&Monitor, PathBuf)> = Vec::new();
        for m in mons {
            let Some(p) = a.get(&m.name) else { continue };
            let still = if WallpaperKind::from_path(p) == Some(WallpaperKind::Video) {
                match video::still_frame(p) {
                    Some(f) => f,
                    None => continue,
                }
            } else {
                p.clone()
            };
            stills.push((m, still));
        }
        let Some(first) = stills.first().map(|(_, p)| p.clone()) else { return Ok(()) };
        // Con una pantalla, la imagen tal cual. Con varias SIEMPRE se compone (aunque sea la misma en
        // todas): así cada pantalla la recibe recortada a su tamaño, y no estirada sobre el escritorio
        // entero como hace Caja en algunas configuraciones.
        let same = mons.len() == 1 && stills.len() == 1;
        let _ = &first;

        let options = self.gsettings(&["get", SCHEMA, "picture-options"]).unwrap_or_default();
        let options = options.trim_matches('\'');
        let file = if same {
            // Un solo fondo: el modo de siempre (y se deshace el «spanned» que puso NekoWall).
            if options == "spanned" || options == "none" {
                let before = video::read_state_in(&self.dir).into_iter().find(|(k, _)| k == "options_before").map(|(_, v)| v);
                let back = before.filter(|b| b != "spanned" && b != "none").unwrap_or_else(|| "zoom".into());
                self.gsettings(&["set", SCHEMA, "picture-options", &back])?;
            }
            first
        } else {
            let out = compose(&stills, mons).ok_or_else(|| WallpaperError::CommandFailed {
                command: "ffmpeg (componer el fondo de varias pantallas)".into(),
                detail: "no se pudo crear la imagen; ¿está instalado ffmpeg?".into(),
            })?;
            if options != "spanned" {
                let mut st: Vec<_> = video::read_state_in(&self.dir).into_iter().filter(|(k, _)| k != "options_before").collect();
                st.push(("options_before".into(), options.to_string()));
                video::write_state_in(&self.dir, &st)?;
                self.gsettings(&["set", SCHEMA, "picture-options", "spanned"])?;
            }
            out
        };
        self.gsettings(&["set", SCHEMA, "draw-background", "true"])?;
        self.gsettings(&["set", SCHEMA, "picture-filename", &gvariant_string(&file.to_string_lossy())])?;
        Ok(())
    }

    /// Deja de reproducir vídeos (si los hay) y devuelve el escritorio de Caja a como estaba.
    fn stop_video(&self) -> Result<(), WallpaperError> {
        if !self.video {
            return Ok(());
        }
        video::stop();
        video::set_autostart(None)?;
        if let Some(prev) = video::take_saved_icons() {
            self.gsettings(&["set", SCHEMA, "show-desktop-icons", &prev])?;
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

/// Imagen del tamaño de todo el escritorio con el fondo de cada pantalla recortado a su sitio
/// (~/.cache/nekowall/span-<hash>.jpg). Se reutiliza si ya existe la misma combinación.
fn compose(stills: &[(&Monitor, PathBuf)], mons: &[Monitor]) -> Option<PathBuf> {
    let w = mons.iter().map(|m| m.x + m.width as i32).max()?.max(1);
    let h = mons.iter().map(|m| m.y + m.height as i32).max()?.max(1);
    let mut key = format!("{w}x{h}");
    for (m, p) in stills {
        let t = std::fs::metadata(p).and_then(|md| md.modified()).ok().map(|t| format!("{t:?}")).unwrap_or_default();
        key.push_str(&format!("|{}:{}x{}+{}+{}={}@{t}", m.name, m.width, m.height, m.x, m.y, p.display()));
    }
    let dir = video::cache_dir()?;
    let out = dir.join(format!("span-{}.jpg", nekowall_backend::id_for_path(Path::new(&key))));
    if !out.exists() {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-y", "-nostdin", "-f", "lavfi", "-i", &format!("color=c=black:s={w}x{h}")]);
        let mut graph = String::new();
        let mut last = "0:v".to_string();
        for (i, (m, p)) in stills.iter().enumerate() {
            cmd.arg("-i").arg(p);
            let (mw, mh) = (m.width, m.height);
            graph.push_str(&format!(
                "[{n}:v]scale={mw}:{mh}:force_original_aspect_ratio=increase,crop={mw}:{mh},setsar=1[s{i}];\
                 [{last}][s{i}]overlay={x}:{y}[c{i}];",
                n = i + 1, x = m.x.max(0), y = m.y.max(0)
            ));
            last = format!("c{i}");
        }
        graph.pop();
        let tmp = dir.join(".span-tmp.jpg");
        let ok = cmd
            .args(["-filter_complex", &graph, "-map", &format!("[{last}]"), "-frames:v", "1", "-q:v", "2"])
            .arg(&tmp)
            .status()
            .is_ok_and(|s| s.success());
        if !ok || std::fs::rename(&tmp, &out).is_err() {
            return None;
        }
    }
    // Composiciones viejas fuera (Caja ya no las usa: el nombre cambia con cada combinación).
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("span-") && e.path() != out {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Some(out)
}

/// La disposición de pantallas cambió (el vigilante lo detecta): rehacer la imagen de Caja.
pub(crate) fn rerender_background() {
    let b = MateBackend::new();
    let mons = b.monitors();
    let a = b.assignments(&mons);
    let _ = b.render(&a, &mons);
}

impl<R: CommandRunner> WallpaperBackend for MateBackend<R> {
    fn name(&self) -> &'static str {
        "mate"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            static_wallpapers: true,
            video_wallpapers: self.video,
            per_monitor: true,
            pause_video: self.video,
        }
    }

    fn monitors(&self) -> Vec<Monitor> {
        MateBackend::monitors(self)
    }

    fn apply(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), WallpaperError> {
        if wallpaper.kind == WallpaperKind::Video && !self.video {
            return Err(WallpaperError::Unsupported(
                "para fondos de vídeo en MATE instala `mpv` y `xwinwrap` (xbps-install mpv xwinwrap)".into(),
            ));
        }
        if !wallpaper.path.is_file() {
            return Err(WallpaperError::NotFound(wallpaper.path.clone()));
        }
        let mons = self.monitors();
        let targets: Vec<String> = match monitor {
            None => mons.iter().map(|m| m.name.clone()).collect(),
            Some(n) if mons.iter().any(|m| m.name == n) => vec![n.to_string()],
            Some(n) => {
                let names: Vec<&str> = mons.iter().map(|m| m.name.as_str()).collect();
                return Err(WallpaperError::Unsupported(format!(
                    "no hay ninguna pantalla «{n}» (hay: {})",
                    names.join(", ")
                )));
            }
        };
        let mut a = self.assignments(&mons);
        for t in targets {
            a.insert(t, wallpaper.path.clone());
        }
        // 1) Primero lo que muestra Caja (con un fotograma donde vaya vídeo): si en algún instante se
        //    ve Caja, ya es la escena nueva. 2) Luego el estado, que hace que el vigilante cree, cambie
        //    o cierre los reproductores.
        self.render(&a, &mons)?;
        video::save_assignments_in(&self.dir, &a)?;
        let has_video = mons.iter().any(|m| {
            a.get(&m.name).is_some_and(|p| WallpaperKind::from_path(p) == Some(WallpaperKind::Video))
        });
        if !self.video {
            return Ok(());
        }
        if has_video {
            let exe = std::env::current_exe()?;
            if !video::running() {
                video::stop(); // restos de un vigilante anterior
                video::spawn(&exe)?;
            }
            video::set_autostart(Some(&exe))?;
            // Versiones anteriores apagaban el escritorio de Caja: si quedó apagado, se restaura.
            if let Some(prev) = video::take_saved_icons() {
                self.gsettings(&["set", SCHEMA, "show-desktop-icons", &prev])?;
            }
            Ok(())
        } else {
            self.stop_video()
        }
    }

    fn current(&self, monitor: Option<&str>) -> Result<Option<Wallpaper>, WallpaperError> {
        let mons = self.monitors();
        let a = self.assignments(&mons);
        let name = match monitor {
            Some(n) => n.to_string(),
            None => mons.iter().find(|m| m.primary).or(mons.first()).map(|m| m.name.clone()).unwrap_or_default(),
        };
        if let Some(p) = a.get(&name) {
            return Ok(Wallpaper::from_path(p));
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
            if program == "xrandr" {
                let out = "Monitors: 1\n 0: +*eDP-1 1920/309x1080/174+0+0  eDP-1\n";
                return Ok(Output { status: ExitStatus::from_raw(0), stdout: out.into(), stderr: vec![] });
            }
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
        drop(calls);
        // Por pantalla: la que existe funciona; una inexistente da error.
        let b = MateBackend::with_runner(&f);
        assert!(b.apply(&w, Some("eDP-1")).is_ok());
        assert_eq!(b.current(Some("eDP-1")).unwrap().unwrap().path, img);
        let calls = f.calls.lock().unwrap();
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
        assert!(b.apply(&img, Some("HDMI-1")).is_err(), "no hay ninguna pantalla HDMI-1");
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
