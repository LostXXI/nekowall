// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Backend KDE Plasma (Wayland).
//!
//! Estáticos: `plasma-apply-wallpaperimage` (plasma-workspace). Validado en Plasma 6.7.5.
//! Vídeo: plugin «Smart Video Wallpaper Reborn», configurado enviando un script
//! a `org.kde.plasmashell /PlasmaShell evaluateScript` (vía `gdbus`).
//!   - El cambio de vídeo por este método fue VALIDADO a mano (el vídeo cambió sin
//!     abrir la interfaz y el shell siguió respondiendo).
//!   - El formato de `VideoUrls` se copió de lo que Plasma escribió realmente.
//!   - Pendiente de validar: pasar de vídeo a imagen con `plasma-apply-wallpaperimage`.
//! Varias pantallas: Plasma tiene un escritorio (contenedor) por pantalla. El script elige el de la
//! pantalla pedida comparando `screenGeometry(d.screen)` con la posición que da `kscreen-doctor -j`
//! (comprobado en Plasma 6.7.5 con dos pantallas reales, 2026-11-16).

use nekowall_backend::{
    BackendCapabilities, Monitor, Wallpaper, WallpaperBackend, WallpaperError, WallpaperKind,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const VIDEO_PLUGIN_ID: &str = "luisbocanegra.smart.video.wallpaper.reborn";
const IMAGE_PLUGIN_ID: &str = "org.kde.image";
const APPLY_TOOL: &str = "plasma-apply-wallpaperimage";
const DBUS_TIMEOUT_SECS: &str = "5";

/// Abstracción de ejecución de comandos para poder probar sin Plasma.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output>;
}

pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        Command::new(program).args(args).output()
    }
}

pub struct PlasmaBackend<R: CommandRunner = SystemRunner> {
    runner: R,
    /// Archivo donde Plasma guarda la configuración de applets/fondos.
    config_path: PathBuf,
    /// Carpetas donde buscar plugins de fondo instalados.
    plugin_dirs: Vec<PathBuf>,
}

impl PlasmaBackend<SystemRunner> {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut dirs = vec![PathBuf::from("/usr/share/plasma/wallpapers")];
        if let Some(h) = &home {
            dirs.push(h.join(".local/share/plasma/wallpapers"));
        }
        Self::with_runner(SystemRunner, default_config_path(), dirs)
    }

    /// Detección simple por `XDG_CURRENT_DESKTOP`; las capacidades reales se
    /// comprueban al aplicar.
    pub fn is_current_desktop() -> bool {
        std::env::var("XDG_CURRENT_DESKTOP")
            .map(|v| v.split(':').any(|p| p.eq_ignore_ascii_case("KDE")))
            .unwrap_or(false)
    }
}

impl Default for PlasmaBackend<SystemRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: CommandRunner> PlasmaBackend<R> {
    pub fn with_runner(runner: R, config_path: PathBuf, plugin_dirs: Vec<PathBuf>) -> Self {
        Self { runner, config_path, plugin_dirs }
    }

    pub fn video_plugin_installed(&self) -> bool {
        self.plugin_dirs.iter().any(|d| d.join(VIDEO_PLUGIN_ID).is_dir())
    }

    /// Ejecuta un programa y traduce los fallos a errores comprensibles.
    fn run_checked(&self, program: &str, args: &[&str], label: &str) -> Result<(), WallpaperError> {
        let out = self.runner.run(program, args).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                WallpaperError::BackendUnavailable(format!(
                    "no se encontró `{program}` (¿instalado y en PATH?)"
                ))
            } else {
                WallpaperError::Io(e)
            }
        })?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let detail = [stderr, stdout]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");

        // Un plasmashell colgado o ausente no expone /PlasmaShell (observado en el equipo real).
        const DEAD: [&str; 4] = ["UnknownObject", "ServiceUnknown", "NameHasNoOwner", "Timeout"];
        if DEAD.iter().any(|m| detail.contains(m)) {
            return Err(WallpaperError::BackendUnavailable(
                "plasmashell no responde por D-Bus (¿colgado o no iniciado?)".into(),
            ));
        }
        Err(WallpaperError::CommandFailed {
            command: label.to_string(),
            detail: if detail.is_empty() {
                "sin salida; comprueba que plasmashell responde".into()
            } else {
                detail
            },
        })
    }

    /// Pantallas encendidas según `kscreen-doctor -j` (tamaño lógico: modo ÷ escala, girado si toca).
    pub fn monitors(&self) -> Vec<Monitor> {
        let Ok(out) = self.runner.run("kscreen-doctor", &["-j"]) else { return Vec::new() };
        parse_kscreen(&String::from_utf8_lossy(&out.stdout))
    }

    /// Ejecuta el script y devuelve lo que imprimió (`print`).
    fn evaluate_script_output(&self, script: &str) -> Result<String, WallpaperError> {
        let out = self
            .runner
            .run(
                "gdbus",
                &[
                    "call", "--session", "--timeout", DBUS_TIMEOUT_SECS,
                    "--dest", "org.kde.plasmashell",
                    "--object-path", "/PlasmaShell",
                    "--method", "org.kde.PlasmaShell.evaluateScript",
                    script,
                ],
            )
            .map_err(WallpaperError::Io)?;
        if !out.status.success() {
            let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
            const DEAD: [&str; 4] = ["UnknownObject", "ServiceUnknown", "NameHasNoOwner", "Timeout"];
            if DEAD.iter().any(|m| detail.contains(m)) {
                return Err(WallpaperError::BackendUnavailable(
                    "plasmashell no responde por D-Bus (¿colgado o no iniciado?)".into(),
                ));
            }
            return Err(WallpaperError::CommandFailed { command: "gdbus … evaluateScript".into(), detail });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Aplica en UNA pantalla: el script cambia solo el escritorio cuya geometría empieza en la
    /// posición de esa pantalla y avisa si no encontró ninguno.
    fn apply_on_monitor(&self, wallpaper: &Wallpaper, name: &str) -> Result<(), WallpaperError> {
        let mons = self.monitors();
        let Some(m) = mons.iter().find(|m| m.name == name) else {
            let names: Vec<&str> = mons.iter().map(|m| m.name.as_str()).collect();
            return Err(WallpaperError::Unsupported(format!(
                "no hay ninguna pantalla «{name}» (hay: {})",
                if names.is_empty() { "ninguna detectada; ¿está kscreen-doctor?".to_string() } else { names.join(", ") }
            )));
        };
        let body = match wallpaper.kind {
            WallpaperKind::Video => video_body(&wallpaper.path),
            WallpaperKind::Static => image_body(&wallpaper.path),
        };
        let script = format!(
            "var n = 0; var ds = desktops();
             for (var i = 0; i < ds.length; i++) {{
               var d = ds[i];
               if (d.screen < 0) continue;
               var g = screenGeometry(d.screen);
               if (g.x != {x} || g.y != {y}) continue;
               {body}
               n++;
             }}
             print('aplicados=' + n);",
            x = m.x,
            y = m.y
        );
        let out = self.evaluate_script_output(&script)?;
        if out.contains("aplicados=0") {
            return Err(WallpaperError::CommandFailed {
                command: "evaluateScript".into(),
                detail: format!("Plasma no tiene ningún escritorio en la pantalla {name}"),
            });
        }
        Ok(())
    }

    fn evaluate_script(&self, script: &str) -> Result<(), WallpaperError> {
        self.run_checked(
            "gdbus",
            &[
                "call", "--session", "--timeout", DBUS_TIMEOUT_SECS,
                "--dest", "org.kde.plasmashell",
                "--object-path", "/PlasmaShell",
                "--method", "org.kde.PlasmaShell.evaluateScript",
                script,
            ],
            "gdbus … evaluateScript",
        )
    }
}

/// Opciones de pausa del plugin de vídeo (claves de su `main.xml`).
/// `pause_mode`: 0 = ventana maximizada o a pantalla completa, 1 = ventana activa,
/// 2 = cualquier ventana visible, 3 = nunca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoPauseSettings {
    pub pause_mode: u8,
    pub battery_pauses: bool,
    pub battery_level: u8,
}

impl Default for VideoPauseSettings {
    /// Valores por defecto del plugin (main.xml de Smart Video Wallpaper Reborn).
    fn default() -> Self {
        Self { pause_mode: 0, battery_pauses: true, battery_level: 10 }
    }
}

impl<R: CommandRunner> PlasmaBackend<R> {
    /// Lee las opciones de pausa del primer escritorio; lo que falte toma el valor por defecto.
    pub fn video_pause_settings(&self) -> VideoPauseSettings {
        let text = std::fs::read_to_string(&self.config_path).unwrap_or_default();
        parse_pause_settings(&text)
    }

    /// Escribe las opciones de pausa en todos los escritorios. Plasma las aplica al momento.
    pub fn set_video_pause_settings(&self, s: &VideoPauseSettings) -> Result<(), WallpaperError> {
        if s.pause_mode > 3 || s.battery_level > 100 {
            return Err(WallpaperError::Unsupported(format!(
                "valores fuera de rango: modo {} (0-3), batería {} % (0-100)",
                s.pause_mode, s.battery_level
            )));
        }
        self.evaluate_script(&pause_script(s))
    }
}

fn pause_script(s: &VideoPauseSettings) -> String {
    format!(
        "var ds = desktops();\n\
         for (var i = 0; i < ds.length; i++) {{\n\
           var d = ds[i];\n\
           d.currentConfigGroup = [\"Wallpaper\", \"{VIDEO_PLUGIN_ID}\", \"General\"];\n\
           d.writeConfig(\"PauseMode\", {});\n\
           d.writeConfig(\"BatteryPausesVideo\", {});\n\
           d.writeConfig(\"PauseBatteryLevel\", {});\n\
         }}\n",
        s.pause_mode, s.battery_pauses, s.battery_level
    )
}

/// Busca la primera sección `[Containments][N][Wallpaper][<plugin de vídeo>][General]`.
fn parse_pause_settings(text: &str) -> VideoPauseSettings {
    let mut s = VideoPauseSettings::default();
    let mut inside = false;
    let mut seen = false;
    for line in text.lines().map(str::trim) {
        if let Some(h) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if seen && inside {
                break; // solo el primer escritorio
            }
            inside = matches!(classify(h), Some((_, Section::Video)));
            seen |= inside;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "PauseMode" => s.pause_mode = v.parse().unwrap_or(s.pause_mode),
                "BatteryPausesVideo" => s.battery_pauses = v.eq_ignore_ascii_case("true"),
                "PauseBatteryLevel" => s.battery_level = v.parse().unwrap_or(s.battery_level),
                _ => {}
            }
        }
    }
    s
}

fn default_config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("plasma-org.kde.plasma.desktop-appletsrc")
}

/// Cuerpo de script (para un escritorio `d`) que activa el plugin de vídeo con un único vídeo.
/// Los campos de cada entrada se copiaron de lo que Plasma escribió realmente.
/// serde_json se encarga de escapar la ruta.
fn video_body(path: &Path) -> String {
    let videos = json!([{
        "filename": format!("file://{}", path.display()),
        "enabled": true,
        "duration": 0,
        "customDuration": 0,
        "playbackRate": 0,
        "alternativePlaybackRate": 0,
        "loop": false,
        "dayNightPhase": 4
    }]);
    format!(
        "d.wallpaperPlugin = \"{VIDEO_PLUGIN_ID}\";
           d.currentConfigGroup = [\"Wallpaper\", \"{VIDEO_PLUGIN_ID}\", \"General\"];
           d.writeConfig(\"VideoUrls\", JSON.stringify({videos}));"
    )
}

/// Cuerpo de script que pone una imagen (lo mismo que hace `plasma-apply-wallpaperimage`).
fn image_body(path: &Path) -> String {
    let url = serde_json::to_string(&format!("file://{}", path.display())).unwrap_or_default();
    format!(
        "d.wallpaperPlugin = \"{IMAGE_PLUGIN_ID}\";
           d.currentConfigGroup = [\"Wallpaper\", \"{IMAGE_PLUGIN_ID}\", \"General\"];
           d.writeConfig(\"Image\", {url});"
    )
}

/// Script de Plasma que pone el vídeo en todos los escritorios.
fn video_script(path: &Path) -> String {
    format!(
        "var ds = desktops();\n\
         for (var i = 0; i < ds.length; i++) {{\n\
           var d = ds[i];\n\
           {}\n\
         }}\n",
        video_body(path)
    )
}

/// `kscreen-doctor -j` → pantallas encendidas. Plasma numera sus pantallas por prioridad (1 = principal).
fn parse_kscreen(text: &str) -> Vec<Monitor> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut out: Vec<(i64, Monitor)> = Vec::new();
    for o in v.get("outputs").and_then(Value::as_array).into_iter().flatten() {
        if o.get("enabled").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let num = |v: Option<&Value>| v.and_then(Value::as_f64).unwrap_or(0.0);
        let scale = num(o.get("scale")).max(0.1);
        let (mut w, mut h) = (num(o.pointer("/size/width")) / scale, num(o.pointer("/size/height")) / scale);
        // rotation: 1 normal, 2 izquierda, 4 invertida, 8 derecha
        if matches!(o.get("rotation").and_then(Value::as_i64), Some(2 | 8)) {
            std::mem::swap(&mut w, &mut h);
        }
        let prio = o.get("priority").and_then(Value::as_i64).unwrap_or(99);
        out.push((prio, Monitor {
            name: o.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            x: num(o.pointer("/pos/x")) as i32,
            y: num(o.pointer("/pos/y")) as i32,
            width: w.round() as u32,
            height: h.round() as u32,
            primary: prio == 1,
        }));
    }
    out.sort_by_key(|(p, _)| *p);
    out.into_iter().map(|(_, m)| m).collect()
}

impl<R: CommandRunner> WallpaperBackend for PlasmaBackend<R> {
    fn name(&self) -> &'static str {
        "plasma"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            static_wallpapers: true,
            video_wallpapers: self.video_plugin_installed(),
            per_monitor: true,
            pause_video: false,
        }
    }

    fn monitors(&self) -> Vec<Monitor> {
        PlasmaBackend::monitors(self)
    }

    fn apply(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), WallpaperError> {
        if !wallpaper.path.is_file() {
            return Err(WallpaperError::NotFound(wallpaper.path.clone()));
        }
        if wallpaper.kind == WallpaperKind::Video && !self.video_plugin_installed() {
            return Err(WallpaperError::Unsupported(format!(
                "falta el plugin de vídeo `{VIDEO_PLUGIN_ID}` (Smart Video Wallpaper Reborn)"
            )));
        }
        if let Some(name) = monitor {
            return self.apply_on_monitor(wallpaper, name);
        }
        match wallpaper.kind {
            WallpaperKind::Video => self.evaluate_script(&video_script(&wallpaper.path)),
            WallpaperKind::Static => {
                let path = wallpaper
                    .path
                    .to_str()
                    .ok_or_else(|| WallpaperError::Unsupported("ruta no UTF-8".into()))?;
                self.run_checked(APPLY_TOOL, &[path], &format!("{APPLY_TOOL} {path}"))
            }
        }
    }

    fn current(&self, monitor: Option<&str>) -> Result<Option<Wallpaper>, WallpaperError> {
        // Número de pantalla de Plasma = prioridad − 1 (la lista ya viene ordenada por prioridad).
        let screen = monitor.and_then(|n| self.monitors().iter().position(|m| m.name == n)).map(|i| i as i64);
        match fs::read_to_string(&self.config_path) {
            Ok(text) => Ok(parse_current(&text, screen)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Default)]
struct Containment {
    plugin: Option<String>,
    screen: Option<i64>,
    image: Option<String>,
    videos: Option<String>,
}

enum Section {
    Root,
    Image,
    Video,
}

fn classify(header: &str) -> Option<(String, Section)> {
    let parts: Vec<&str> = header.split("][").collect();
    if parts.len() < 2 || parts[0] != "Containments" {
        return None;
    }
    let id = parts[1].to_string();
    match parts.len() {
        2 => Some((id, Section::Root)),
        5 if parts[2] == "Wallpaper" && parts[4] == "General" => match parts[3] {
            IMAGE_PLUGIN_ID => Some((id, Section::Image)),
            VIDEO_PLUGIN_ID => Some((id, Section::Video)),
            _ => None,
        },
        _ => None,
    }
}

/// Determina el fondo activo (de la pantalla `screen`, o el primero). Plasma conserva la configuración
/// de plugins que ya no están activos (p. ej. `Image=` con el vídeo activo), así que manda `wallpaperplugin=`.
fn parse_current(text: &str, screen: Option<i64>) -> Option<Wallpaper> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Containment> = HashMap::new();
    let mut cur: Option<(String, Section)> = None;

    for line in text.lines().map(str::trim) {
        if let Some(h) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            cur = classify(h);
            if let Some((id, _)) = &cur {
                if !map.contains_key(id) {
                    order.push(id.clone());
                    map.insert(id.clone(), Containment::default());
                }
            }
        } else if let Some((id, section)) = &cur {
            let c = map.get_mut(id)?;
            match section {
                Section::Root => {
                    if let Some(v) = line.strip_prefix("wallpaperplugin=") {
                        c.plugin = Some(v.to_string());
                    } else if let Some(v) = line.strip_prefix("lastScreen=") {
                        c.screen = v.parse().ok();
                    }
                }
                Section::Image => {
                    if let Some(v) = line.strip_prefix("Image=") {
                        c.image = Some(v.to_string());
                    }
                }
                Section::Video => {
                    if let Some(v) = line.strip_prefix("VideoUrls=") {
                        c.videos = Some(v.to_string());
                    }
                }
            }
        }
    }

    // Primer contenedor de escritorio (los paneles no tienen wallpaperplugin) de esa pantalla.
    let chosen = order
        .iter()
        .filter_map(|id| map.get(id))
        .find(|c| c.plugin.is_some() && screen.is_none_or(|s| c.screen == Some(s)))
        .or_else(|| order.iter().filter_map(|id| map.get(id)).find(|c| c.image.is_some()))?;

    match chosen.plugin.as_deref() {
        Some(VIDEO_PLUGIN_ID) => first_enabled_video(chosen.videos.as_deref()?),
        _ => {
            let raw = chosen.image.as_deref()?;
            let raw = raw.strip_prefix("file://").unwrap_or(raw);
            Wallpaper::from_path(Path::new(&percent_decode(raw)))
        }
    }
}

fn first_enabled_video(json_text: &str) -> Option<Wallpaper> {
    let list: Vec<Value> = serde_json::from_str(json_text).ok()?;
    let entry = list
        .iter()
        .find(|v| v.get("enabled").and_then(Value::as_bool).unwrap_or(true))?;
    let filename = entry.get("filename")?.as_str()?;
    // Plasma guarda la ruta sin codificar (espacios literales): no se decodifica.
    let path = filename.strip_prefix("file://").unwrap_or(filename);
    Wallpaper::from_path(Path::new(path))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::sync::Mutex;

    struct Fake {
        result: Box<dyn Fn() -> io::Result<Output> + Send + Sync>,
        /// Respuesta de `kscreen-doctor -j` (si no, la misma que el resto).
        kscreen: Option<String>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }
    impl Fake {
        fn ok() -> Self {
            Self::new(|| Ok(out(0, "", "")))
        }
        fn new(f: impl Fn() -> io::Result<Output> + Send + Sync + 'static) -> Self {
            Self { result: Box::new(f), kscreen: None, calls: Mutex::new(vec![]) }
        }
    }
    impl CommandRunner for &Fake {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
            self.calls
                .lock()
                .unwrap()
                .push((program.into(), args.iter().map(|s| s.to_string()).collect()));
            if let (Some(k), "kscreen-doctor") = (&self.kscreen, program) {
                return Ok(out(0, k, ""));
            }
            (self.result)()
        }
    }
    fn out(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nw-plasma-{}-{tag}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn file(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, b"x").unwrap();
        p
    }
    /// Backend con (o sin) plugin de vídeo «instalado».
    fn backend<'a>(f: &'a Fake, with_plugin: bool, tag: &str) -> PlasmaBackend<&'a Fake> {
        let plugins = tmpdir(&format!("plugins-{tag}"));
        if with_plugin {
            fs::create_dir_all(plugins.join(VIDEO_PLUGIN_ID)).unwrap();
        }
        PlasmaBackend::with_runner(f, PathBuf::from("/no/existe"), vec![plugins])
    }

    // Fragmento REAL escrito por Plasma 6.7.5 con el plugin de vídeo activo.
    const REAL: &str = "[Containments][1]\n\
        activityId=\nformfactor=0\nlastScreen=0\nlocation=0\nplugin=org.kde.plasma.folder\n\
        wallpaperplugin=luisbocanegra.smart.video.wallpaper.reborn\n\n\
        [Containments][1][ConfigDialog]\nDialogHeight=1006\n\n\
        [Containments][1][Wallpaper][luisbocanegra.smart.video.wallpaper.reborn][General]\n\
        VideoUrls=[{\"filename\":\"file:///home/usuario/Pictures/Wallpapers/Videos/Video de prueba - 1080p.mp4\",\"enabled\":true,\"duration\":0,\"customDuration\":0,\"playbackRate\":0,\"alternativePlaybackRate\":0,\"loop\":false,\"dayNightPhase\":4}]\n\n\
        [Containments][1][Wallpaper][org.kde.image][General]\n\
        Image=file:///home/usuario/Pictures/Wallpapers/imagen-de-prueba.png\nSlidePaths=/usr/share/wallpapers/\n\n\
        [Containments][2]\nformfactor=2\nlocation=4\nplugin=org.kde.panel\n";

    #[test]
    fn applies_static_with_expected_command() {
        let f = Fake::ok();
        let p = file(&tmpdir("s1"), "a.png");
        backend(&f, false, "s1").apply(&Wallpaper::from_path(&p).unwrap(), None).unwrap();
        let calls = f.calls.lock().unwrap();
        assert_eq!(calls[0].0, "plasma-apply-wallpaperimage");
        assert_eq!(calls[0].1, vec![p.to_str().unwrap().to_string()]);
    }

    #[test]
    fn applies_video_via_evaluate_script_with_escaped_path() {
        let f = Fake::ok();
        let p = file(&tmpdir("v1"), "mi \"video\" raro.mp4");
        backend(&f, true, "v1").apply(&Wallpaper::from_path(&p).unwrap(), None).unwrap();

        let calls = f.calls.lock().unwrap();
        let (prog, args) = &calls[0];
        assert_eq!(prog, "gdbus");
        assert!(args.contains(&"org.kde.PlasmaShell.evaluateScript".to_string()));
        assert!(args.contains(&"/PlasmaShell".to_string()));
        assert!(args.contains(&"--timeout".to_string()));

        let script = args.last().unwrap();
        assert!(script.contains(&format!("d.wallpaperPlugin = \"{VIDEO_PLUGIN_ID}\"")));
        assert!(script.contains("writeConfig(\"VideoUrls\""));
        // Las comillas de la ruta deben ir escapadas (JSON válido dentro del script).
        assert!(script.contains("mi \\\"video\\\" raro.mp4"));
        assert!(script.contains("\"dayNightPhase\":4"));
    }

    #[test]
    fn video_requires_plugin_and_capability_follows_it() {
        let f = Fake::ok();
        let p = file(&tmpdir("v2"), "a.mp4");
        let w = Wallpaper::from_path(&p).unwrap();

        let without = backend(&f, false, "v2a");
        assert!(!without.capabilities().video_wallpapers);
        assert!(matches!(without.apply(&w, None), Err(WallpaperError::Unsupported(_))));

        let with = backend(&f, true, "v2b");
        assert!(with.capabilities().video_wallpapers);
        assert!(f.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn rejects_monitor_and_missing_file_without_running_commands() {
        let f = Fake::ok();
        let b = backend(&f, true, "r1");
        let p = file(&tmpdir("r1"), "a.png");
        // kscreen-doctor no devuelve pantallas → «no hay ninguna pantalla DP-1».
        assert!(matches!(
            b.apply(&Wallpaper::from_path(&p).unwrap(), Some("DP-1")),
            Err(WallpaperError::Unsupported(_))
        ));
        let gone = Wallpaper { id: "g".into(), path: "/no/existe.png".into(), kind: WallpaperKind::Static };
        assert!(matches!(b.apply(&gone, None), Err(WallpaperError::NotFound(_))));
        assert!(f.calls.lock().unwrap().iter().all(|(p, _)| p == "kscreen-doctor"), "nada aplicado");
    }

    // Recorte de `kscreen-doctor -j` real (Plasma 6.7.5) + una segunda pantalla a la derecha con escala 1,5.
    const KSCREEN: &str = r#"{"outputs":[
        {"id":2,"name":"HDMI-A-1","enabled":true,"priority":2,"pos":{"x":1920,"y":0},"size":{"width":3840,"height":2160},"scale":1.5,"rotation":1},
        {"id":1,"name":"eDP-1","enabled":true,"priority":1,"pos":{"x":0,"y":0},"size":{"width":1920,"height":1080},"scale":1,"rotation":1},
        {"id":3,"name":"DP-2","enabled":false,"priority":0,"pos":{"x":0,"y":0},"size":{"width":800,"height":600},"scale":1,"rotation":1}]}"#;

    #[test]
    fn parses_kscreen_outputs_in_plasma_order() {
        let m = parse_kscreen(KSCREEN);
        assert_eq!(m.len(), 2, "la apagada no cuenta");
        assert_eq!((m[0].name.as_str(), m[0].primary), ("eDP-1", true));
        assert_eq!((m[1].name.as_str(), m[1].x, m[1].width, m[1].height), ("HDMI-A-1", 1920, 2560, 1440));
    }

    #[test]
    fn applies_on_one_monitor_by_its_position() {
        let mut f = Fake::new(|| Ok(out(0, "('aplicados=1',)", "")));
        f.kscreen = Some(KSCREEN.into());
        let p = file(&tmpdir("m1"), "b.png");
        let w = Wallpaper::from_path(&p).unwrap();
        backend(&f, true, "m1").apply(&w, Some("HDMI-A-1")).unwrap();
        let calls = f.calls.lock().unwrap();
        let (prog, args) = calls.last().unwrap();
        assert_eq!(prog, "gdbus");
        let script = args.last().unwrap();
        assert!(script.contains("g.x != 1920 || g.y != 0"));
        assert!(script.contains(&format!("\"file://{}\"", p.display())));
        assert!(!script.contains("\\n"), "gdbus estropea los «\\n» del script");
        drop(calls);

        // Si Plasma no tiene escritorio en esa pantalla, es un error (no un éxito silencioso).
        let mut g = Fake::new(|| Ok(out(0, "('aplicados=0',)", "")));
        g.kscreen = Some(KSCREEN.into());
        assert!(backend(&g, true, "m2").apply(&w, Some("eDP-1")).is_err());
    }

    #[test]
    fn current_per_screen_uses_last_screen() {
        let two = format!(
            "{REAL}\n[Containments][7]\nlastScreen=1\nplugin=org.kde.plasma.folder\nwallpaperplugin=org.kde.image\n\n\
             [Containments][7][Wallpaper][org.kde.image][General]\nImage=file:///x/Segunda.png\n"
        );
        assert_eq!(parse_current(&two, Some(1)).unwrap().path, PathBuf::from("/x/Segunda.png"));
        assert_eq!(parse_current(&two, Some(0)).unwrap().kind, WallpaperKind::Video);
    }

    #[test]
    fn error_mapping() {
        let p = file(&tmpdir("e1"), "a.png");
        let w = Wallpaper::from_path(&p).unwrap();

        // Herramienta ausente
        let f = Fake::new(|| Err(io::Error::from(io::ErrorKind::NotFound)));
        assert!(matches!(backend(&f, false, "e1a").apply(&w, None), Err(WallpaperError::BackendUnavailable(_))));

        // plasmashell colgado: /PlasmaShell no existe (caso real observado)
        let f = Fake::new(|| Ok(out(1, "", "Error: GDBus.Error:org.freedesktop.DBus.Error.UnknownObject: No such object path '/PlasmaShell'")));
        assert!(matches!(backend(&f, false, "e1b").apply(&w, None), Err(WallpaperError::BackendUnavailable(_))));

        // Salida solo por stdout: ya no se pierde el mensaje
        let f = Fake::new(|| Ok(out(1, "motivo en stdout", "")));
        match backend(&f, false, "e1c").apply(&w, None) {
            Err(WallpaperError::CommandFailed { detail, .. }) => assert_eq!(detail, "motivo en stdout"),
            other => panic!("inesperado: {other:?}"),
        }

        // Sin ninguna salida (caso real del `apply` con el shell colgado)
        let f = Fake::new(|| Ok(out(1, "", "")));
        match backend(&f, false, "e1d").apply(&w, None) {
            Err(WallpaperError::CommandFailed { detail, .. }) => assert!(detail.contains("plasmashell")),
            other => panic!("inesperado: {other:?}"),
        }
    }

    #[test]
    fn current_prefers_active_plugin() {
        // Con el plugin de vídeo activo, manda el vídeo aunque quede `Image=` guardado.
        let w = parse_current(REAL, None).unwrap();
        assert_eq!(w.kind, WallpaperKind::Video);
        assert_eq!(
            w.path,
            PathBuf::from("/home/usuario/Pictures/Wallpapers/Videos/Video de prueba - 1080p.mp4")
        );

        // Con el plugin de imagen activo, manda la imagen.
        let img = REAL.replace(
            "wallpaperplugin=luisbocanegra.smart.video.wallpaper.reborn",
            "wallpaperplugin=org.kde.image",
        );
        let w = parse_current(&img, None).unwrap();
        assert_eq!(w.kind, WallpaperKind::Static);
        assert_eq!(w.path, PathBuf::from("/home/usuario/Pictures/Wallpapers/imagen-de-prueba.png"));
    }

    #[test]
    fn current_edge_cases() {
        // Vídeo activo pero lista vacía (tras la recuperación) → sin fondo conocido
        let empty = REAL.replace(
            REAL.lines().find(|l| l.starts_with("VideoUrls=")).unwrap(),
            "VideoUrls=",
        );
        assert_eq!(parse_current(&empty, None), None);
        // Sin `wallpaperplugin`, se usa Image= (configuración antigua)
        let old = "[Containments][1][Wallpaper][org.kde.image][General]\nImage=file:///x/Gato%20Negro.png\n";
        assert_eq!(parse_current(old, None).unwrap().path, PathBuf::from("/x/Gato Negro.png"));
        // Entradas desactivadas se ignoran
        let off = REAL.replace("\"enabled\":true", "\"enabled\":false");
        assert_eq!(parse_current(&off, None), None);
        assert_eq!(parse_current("", None), None);
    }

    #[test]
    fn percent_decode_handles_edges() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn pause_settings_parse_and_defaults() {
        assert_eq!(parse_pause_settings(""), VideoPauseSettings::default());
        let cfg = "[Containments][1][Wallpaper][luisbocanegra.smart.video.wallpaper.reborn][General]\n\
                   PauseMode=2\nBatteryPausesVideo=false\nPauseBatteryLevel=25\n\
                   [Containments][7][Wallpaper][luisbocanegra.smart.video.wallpaper.reborn][General]\n\
                   PauseMode=3\n";
        let s = parse_pause_settings(cfg);
        assert_eq!(s, VideoPauseSettings { pause_mode: 2, battery_pauses: false, battery_level: 25 });
        // Solo con la sección del plugin de imagen: valores por defecto.
        let img = "[Containments][1][Wallpaper][org.kde.image][General]\nPauseMode=3\n";
        assert_eq!(parse_pause_settings(img), VideoPauseSettings::default());
    }

    #[test]
    fn pause_settings_script_and_validation() {
        let f = Fake::ok();
        let b = backend(&f, true, "pause");
        let s = VideoPauseSettings { pause_mode: 1, battery_pauses: false, battery_level: 30 };
        b.set_video_pause_settings(&s).unwrap();
        let script = f.calls.lock().unwrap().last().unwrap().1.last().unwrap().clone();
        assert!(script.contains("writeConfig(\"PauseMode\", 1)"));
        assert!(script.contains("writeConfig(\"BatteryPausesVideo\", false)"));
        assert!(script.contains("writeConfig(\"PauseBatteryLevel\", 30)"));
        let bad = VideoPauseSettings { pause_mode: 9, ..s };
        assert!(b.set_video_pause_settings(&bad).is_err());
    }
}
