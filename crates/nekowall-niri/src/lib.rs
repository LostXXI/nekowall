//! Backend para **niri** (compositor Wayland en mosaico).
//!
//! - Pantallas: `niri msg -j outputs`.
//! - Imágenes: si **Noctalia** está corriendo (neko-void niri la trae), `noctalia msg wallpaper-set`:
//!   Noctalia hace sus transiciones y genera los colores del sistema a partir del fondo. Si no, **awww**
//!   (`awww img`, con su `awww-daemon`), con transición suave.
//! - Vídeos: un **mpvpaper** por pantalla en la capa `bottom` (encima del fondo de Noctalia/awww y debajo
//!   de las ventanas), sin audio y en bucle. Debajo se pone un fotograma del vídeo como imagen, para que
//!   los colores del sistema sigan al vídeo y no haya parpadeo.
//! - Pausa: el vigilante `nekowall _niri-video` mira cada segundo las ventanas del escritorio activo de
//!   cada pantalla (`niri msg -j workspaces/windows`) y pausa ese vídeo por IPC según los Ajustes (ventana
//!   maximizada, activa, cualquiera o nunca; y batería baja). niri no avisa a mpvpaper de que está tapado.
//! - Estado: `~/.config/nekowall/niri.conf` (`mon.<pantalla>=ruta`, `pid.<pantalla>=pid de mpvpaper`).
//! - Al iniciar sesión: `~/.config/niri/nekowall.kdl` (incluido desde config.kdl) lanza
//!   `nekowall _niri-restore`, que vuelve a poner los vídeos; y guarda el atajo Meta+Alt+W.

use nekowall_backend::{
    BackendCapabilities, Monitor, Wallpaper, WallpaperBackend, WallpaperError, WallpaperKind,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct NiriBackend;

impl Default for NiriBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn state_path() -> PathBuf {
    config_dir().join("nekowall/niri.conf")
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nekowall")
}

// ── Estado (clave=valor) ────────────────────────────────────────────────────

pub fn read_state() -> BTreeMap<String, String> {
    std::fs::read_to_string(state_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

fn write_state(st: &BTreeMap<String, String>) -> Result<(), WallpaperError> {
    let p = state_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let text: String = st.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let tmp = p.with_extension("conf.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

// ── Utilidades ──────────────────────────────────────────────────────────────

fn run(cmd: &str, args: &[&str]) -> Result<String, WallpaperError> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| WallpaperError::CommandFailed { command: cmd.into(), detail: e.to_string() })?;
    if !out.status.success() {
        return Err(WallpaperError::CommandFailed {
            command: format!("{cmd} {}", args.join(" ")),
            detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn has(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

fn process_running(name: &str) -> bool {
    Command::new("pgrep").args(["-x", name]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// ¿El proceso `pid` es un mpvpaper (y sigue vivo)?
fn is_mpvpaper(pid: u32) -> bool {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|c| c.split(|b| *b == 0).next().is_some_and(|a| a.ends_with(b"mpvpaper")))
        .unwrap_or(false)
}

/// Parsea `niri msg -j outputs` (objeto nombre → datos). Solo las pantallas encendidas (con `logical`).
/// Orden: de izquierda a derecha y de arriba abajo; la primera es la «principal».
pub fn parse_outputs(json: &str) -> Vec<Monitor> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut mons: Vec<Monitor> = map
        .values()
        .filter_map(|o| {
            let l = o.get("logical")?.as_object()?;
            // Tamaño real en píxeles: el modo actual (girado si la pantalla está rotada).
            let mode = o["current_mode"].as_u64().and_then(|i| o["modes"].get(i as usize));
            let (mut w, mut h) = match mode {
                Some(m) => (m["width"].as_u64()? as u32, m["height"].as_u64()? as u32),
                None => (l["width"].as_u64()? as u32, l["height"].as_u64()? as u32),
            };
            let t = l.get("transform").and_then(|t| t.as_str()).unwrap_or("Normal");
            if t.contains("90") || t.contains("270") {
                std::mem::swap(&mut w, &mut h);
            }
            Some(Monitor {
                name: o["name"].as_str()?.to_string(),
                x: l["x"].as_i64()? as i32,
                y: l["y"].as_i64()? as i32,
                width: w,
                height: h,
                primary: false,
            })
        })
        .collect();
    mons.sort_by_key(|m| (m.x, m.y));
    if let Some(m) = mons.first_mut() {
        m.primary = true;
    }
    mons
}

/// Fotograma de un vídeo (para poner debajo del vídeo y para los colores del sistema).
fn still_frame(video: &Path) -> Option<PathBuf> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join(format!("frame-{}.jpg", nekowall_backend::id_for_path(video)));
    if out.is_file() {
        return Some(out);
    }
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-ss", "1", "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-q:v", "3"])
        .arg(&out)
        .status()
        .is_ok_and(|s| s.success());
    (ok && out.is_file()).then_some(out)
}

fn mpv_socket(monitor: &str) -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("nekowall").join(format!("mpvpaper-{monitor}.sock"))
}

impl NiriBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn is_current_desktop() -> bool {
        std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_ascii_lowercase().split(':').any(|p| p == "niri"))
            || std::env::var_os("NIRI_SOCKET").is_some()
    }

    fn noctalia() -> bool {
        has("noctalia") && process_running("noctalia")
    }

    /// Pone una imagen en esas pantallas (`None` = todas).
    fn set_image(&self, path: &Path, targets: Option<&[String]>) -> Result<(), WallpaperError> {
        let p = path.to_string_lossy();
        if Self::noctalia() {
            match targets {
                None => { run("noctalia", &["msg", "wallpaper-set", &p])?; }
                Some(ts) => {
                    for t in ts {
                        run("noctalia", &["msg", "wallpaper-set", t, &p])?;
                    }
                }
            }
            return Ok(());
        }
        if !has("awww") {
            return Err(WallpaperError::BackendUnavailable(
                "falta awww para poner imágenes en niri (sudo xbps-install -S awww)".into(),
            ));
        }
        // awww necesita su proceso en segundo plano.
        if run("awww", &["query"]).is_err() {
            let _ = Command::new("awww-daemon")
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                .process_group(0)
                .spawn();
            for _ in 0..30 {
                if run("awww", &["query"]).is_ok() { break; }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        let mut args = vec!["img", "--transition-type", "fade", "--transition-duration", "0.8"];
        let joined;
        if let Some(ts) = targets {
            joined = ts.join(",");
            args.extend(["-o", joined.as_str()]);
        }
        args.push(&p);
        run("awww", &args)?;
        Ok(())
    }

    fn stop_video(&self, monitor: &str, st: &mut BTreeMap<String, String>) {
        if let Some(pid) = st.remove(&format!("pid.{monitor}")).and_then(|p| p.parse::<u32>().ok()) {
            if is_mpvpaper(pid) {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
        let _ = std::fs::remove_file(mpv_socket(monitor));
    }

    fn start_video(&self, monitor: &str, video: &Path, st: &mut BTreeMap<String, String>) -> Result<(), WallpaperError> {
        if !has("mpvpaper") {
            return Err(WallpaperError::BackendUnavailable(
                "falta mpvpaper para los fondos de vídeo en niri (sudo xbps-install -S mpvpaper)".into(),
            ));
        }
        self.stop_video(monitor, st);
        let sock = mpv_socket(monitor);
        if let Some(d) = sock.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let opts = format!(
            "no-audio loop-file=inf hwdec=auto panscan=1.0 input-ipc-server={}",
            sock.display()
        );
        let child = Command::new("mpvpaper")
            .args(["-l", "bottom", "-o", &opts, monitor])
            .arg(video)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .process_group(0)   // sigue vivo aunque se cierre NekoWall
            .spawn()
            .map_err(|e| WallpaperError::CommandFailed { command: "mpvpaper".into(), detail: e.to_string() })?;
        st.insert(format!("pid.{monitor}"), child.id().to_string());
        Ok(())
    }

    /// Vuelve a poner los vídeos guardados (al iniciar sesión). Las imágenes las recuerdan
    /// Noctalia/awww por su cuenta.
    pub fn restore(&self) -> Result<usize, WallpaperError> {
        let mut st = read_state();
        let names: Vec<String> = self.monitors().into_iter().map(|m| m.name).collect();
        let mut n = 0;
        let videos: Vec<(String, PathBuf)> = st
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix("mon.")?.to_string(), PathBuf::from(v))))
            .filter(|(m, p)| names.contains(m) && p.is_file()
                && WallpaperKind::from_path(p) == Some(WallpaperKind::Video))
            .collect();
        for (m, p) in videos {
            if self.start_video(&m, &p, &mut st).is_ok() {
                n += 1;
            }
        }
        write_state(&st)?;
        if n > 0 {
            spawn_watcher();
        }
        Ok(n)
    }

    fn validate(&self, monitor: Option<&str>, mons: &[Monitor]) -> Result<Vec<String>, WallpaperError> {
        match monitor {
            Some(m) if mons.iter().any(|x| x.name == m) => Ok(vec![m.to_string()]),
            Some(m) => Err(WallpaperError::Unsupported(format!(
                "no hay ninguna pantalla «{m}» (hay: {})",
                mons.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")
            ))),
            None => Ok(mons.iter().map(|x| x.name.clone()).collect()),
        }
    }
}

// ── Configuración de niri: arranque y atajo ─────────────────────────────────

/// Escribe `~/.config/niri/nekowall.kdl` (arranque + atajos) y lo incluye desde config.kdl.
/// `cli`: ruta de `nekowall`; `gui`: ruta de `nekowall-gui` (si se conoce, añade Meta+Alt+W).
/// niri recarga su configuración sola al cambiar el archivo.
pub fn write_niri_config(cli: &Path, gui: Option<&Path>) -> Result<PathBuf, WallpaperError> {
    let dir = config_dir().join("niri");
    let main = dir.join("config.kdl");
    if !main.is_file() {
        return Err(WallpaperError::BackendUnavailable(format!("no encuentro {}", main.display())));
    }
    let mut kdl = String::from(
        "// NekoWall: generado automáticamente. Para quitarlo, borra este archivo y la línea\n\
         // `include \"nekowall.kdl\"` de config.kdl.\n\n",
    );
    kdl += &format!("spawn-at-startup \"{}\" \"_niri-restore\"\n", cli.display());
    if let Some(g) = gui {
        kdl += &format!(
            "\nbinds {{\n    Mod+Alt+W hotkey-overlay-title=\"NekoWall: selector rápido\" {{ spawn \"{}\" \"--hud\"; }}\n}}\n",
            g.display()
        );
    }
    let file = dir.join("nekowall.kdl");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(kdl.as_str()) {
        std::fs::write(&file, kdl)?;
    }
    let text = std::fs::read_to_string(&main)?;
    if !text.lines().any(|l| l.trim() == "include \"nekowall.kdl\"") {
        let sep = if text.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&main, format!("{text}{sep}\n// NekoWall (fondos y atajo Meta+Alt+W)\ninclude \"nekowall.kdl\"\n"))?;
    }
    Ok(file)
}

impl WallpaperBackend for NiriBackend {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            static_wallpapers: true,
            video_wallpapers: has("mpvpaper"),
            per_monitor: true,
            pause_video: true,
        }
    }

    fn monitors(&self) -> Vec<Monitor> {
        run("niri", &["msg", "-j", "outputs"]).map(|j| parse_outputs(&j)).unwrap_or_default()
    }

    fn apply(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), WallpaperError> {
        if !wallpaper.path.is_file() {
            return Err(WallpaperError::NotFound(wallpaper.path.clone()));
        }
        let mons = self.monitors();
        if mons.is_empty() {
            return Err(WallpaperError::BackendUnavailable("niri no responde (`niri msg outputs`)".into()));
        }
        let targets = self.validate(monitor, &mons)?;
        let only = monitor.map(|_| targets.as_slice());
        let mut st = read_state();
        match wallpaper.kind {
            WallpaperKind::Static => {
                for t in &targets {
                    self.stop_video(t, &mut st);
                }
                self.set_image(&wallpaper.path, only)?;
            }
            WallpaperKind::Video => {
                if let Some(frame) = still_frame(&wallpaper.path) {
                    let _ = self.set_image(&frame, only);
                }
                for t in &targets {
                    self.start_video(t, &wallpaper.path, &mut st)?;
                }
                write_state(&st)?;
                spawn_watcher();
            }
        }
        for t in &targets {
            st.insert(format!("mon.{t}"), wallpaper.path.to_string_lossy().into_owned());
        }
        write_state(&st)?;
        // Que el fondo vuelva al iniciar sesión.
        if let Ok(exe) = std::env::current_exe() {
            let gui = read_state().get("gui").map(PathBuf::from);
            let _ = write_niri_config(&exe, gui.as_deref());
        }
        Ok(())
    }

    fn current(&self, monitor: Option<&str>) -> Result<Option<Wallpaper>, WallpaperError> {
        let st = read_state();
        let name = match monitor {
            Some(m) => m.to_string(),
            None => match self.monitors().into_iter().find(|m| m.primary) {
                Some(m) => m.name,
                None => return Ok(None),
            },
        };
        if let Some(p) = st.get(&format!("mon.{name}")) {
            return Ok(Wallpaper::from_path(Path::new(p)));
        }
        // Sin estado propio: lo que tenga Noctalia.
        if Self::noctalia() {
            if let Ok(p) = run("noctalia", &["msg", "wallpaper-get", &name]) {
                return Ok(Wallpaper::from_path(Path::new(p.trim())));
            }
        }
        Ok(None)
    }
}

// ── Pausa del vídeo ─────────────────────────────────────────────────────────

/// Ajustes de pausa (mismas opciones que en KDE y MATE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseSettings {
    /// 0 = ventana maximizada o a pantalla completa · 1 = ventana activa · 2 = cualquier ventana · 3 = nunca
    pub pause_mode: u8,
    pub battery_pauses: bool,
    pub battery_level: u8,
}

impl Default for PauseSettings {
    fn default() -> Self {
        Self { pause_mode: 0, battery_pauses: true, battery_level: 10 }
    }
}

pub fn load_settings() -> PauseSettings {
    let st = read_state();
    let d = PauseSettings::default();
    PauseSettings {
        pause_mode: st.get("pause_mode").and_then(|v| v.parse().ok()).filter(|m| *m <= 3).unwrap_or(d.pause_mode),
        battery_pauses: st.get("battery_pauses").map(|v| v == "true").unwrap_or(d.battery_pauses),
        battery_level: st.get("battery_level").and_then(|v| v.parse().ok()).filter(|l| *l <= 100).unwrap_or(d.battery_level),
    }
}

pub fn save_settings(s: &PauseSettings) -> Result<(), WallpaperError> {
    let mut st = read_state();
    st.insert("pause_mode".into(), s.pause_mode.to_string());
    st.insert("battery_pauses".into(), s.battery_pauses.to_string());
    st.insert("battery_level".into(), s.battery_level.to_string());
    write_state(&st)
}

/// ¿Hay mpvpaper para los vídeos?
pub fn video_available() -> bool {
    has("mpvpaper")
}

fn battery_low(level: u8) -> bool {
    let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else { return false };
    for e in rd.flatten() {
        let p = e.path();
        let read = |f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default().trim().to_string();
        if read("type") == "Battery" && read("status") == "Discharging" {
            if let Ok(c) = read("capacity").parse::<u8>() {
                return c < level;
            }
        }
    }
    false
}

/// Una ventana del escritorio activo de una pantalla: (ancho, alto, ¿es la enfocada?).
pub type WinInfo = (f64, f64, bool);

/// ¿Pausar el vídeo de una pantalla de `ow`×`oh` (lógicos) con estas ventanas en su escritorio activo?
pub fn should_pause(s: &PauseSettings, wins: &[WinInfo], ow: f64, oh: f64, low_battery: bool) -> bool {
    if s.battery_pauses && low_battery {
        return true;
    }
    match s.pause_mode {
        3 => false,
        1 => wins.iter().any(|w| w.2),
        2 => !wins.is_empty(),
        // Maximizada o a pantalla completa: ocupa casi toda la pantalla (niri deja márgenes y la barra).
        _ => wins.iter().any(|w| w.0 >= ow * 0.85 && w.1 >= oh * 0.75),
    }
}

fn mpv_pause(monitor: &str, pause: bool) -> bool {
    use std::io::Write;
    match std::os::unix::net::UnixStream::connect(mpv_socket(monitor)) {
        Ok(mut s) => writeln!(s, r#"{{"command":["set_property","pause",{pause}]}}"#).is_ok(),
        Err(_) => false,
    }
}

fn watcher_running() -> bool {
    let me = std::process::id();
    std::fs::read_dir("/proc").ok().is_some_and(|rd| rd.flatten().any(|e| {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { return false };
        if pid == me { return false; }
        std::fs::read(e.path().join("cmdline")).is_ok_and(|c| c.split(|b| *b == 0).nth(1) == Some(b"_niri-video"))
    }))
}

/// Arranca el vigilante de los vídeos si no está ya en marcha.
pub fn spawn_watcher() {
    if watcher_running() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe)
            .arg("_niri-video")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .process_group(0)
            .spawn();
    }
}

/// `nekowall _niri-video`: pausa y reanuda cada vídeo según lo que lo tapa. Termina cuando ya no
/// queda ningún mpvpaper de NekoWall.
pub fn run_watcher() -> Result<(), String> {
    let mut paused: BTreeMap<String, bool> = BTreeMap::new();
    loop {
        let st = read_state();
        let videos: Vec<String> = st
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix("pid.")?, v.parse::<u32>().ok()?)))
            .filter(|(_, pid)| is_mpvpaper(*pid))
            .map(|(m, _)| m.to_string())
            .collect();
        if videos.is_empty() {
            return Ok(());
        }
        let settings = load_settings();
        let low = settings.battery_pauses && battery_low(settings.battery_level);
        let outputs: Value = run("niri", &["msg", "-j", "outputs"]).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or(Value::Null);
        let workspaces: Value = run("niri", &["msg", "-j", "workspaces"]).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or(Value::Null);
        let windows: Value = run("niri", &["msg", "-j", "windows"]).ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or(Value::Null);
        for m in &videos {
            let l = &outputs[m.as_str()]["logical"];
            let (ow, oh) = (l["width"].as_f64().unwrap_or(1.0), l["height"].as_f64().unwrap_or(1.0));
            let ws_id = workspaces.as_array().and_then(|a| a.iter().find(|w| w["output"] == m.as_str() && w["is_active"] == true))
                .and_then(|w| w["id"].as_u64());
            let wins: Vec<WinInfo> = windows.as_array().map(|a| a.iter()
                .filter(|w| ws_id.is_some() && w["workspace_id"].as_u64() == ws_id)
                .map(|w| (w["layout"]["tile_size"][0].as_f64().unwrap_or(0.0),
                          w["layout"]["tile_size"][1].as_f64().unwrap_or(0.0),
                          w["is_focused"] == true))
                .collect()).unwrap_or_default();
            let want = should_pause(&settings, &wins, ow, oh, low);
            if paused.get(m) != Some(&want) && mpv_pause(m, want) {
                paused.insert(m.clone(), want);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
}

/// Guarda la ruta de nekowall-gui (para el atajo) y regenera nekowall.kdl.
pub fn register_shortcut(cli: &Path, gui: &Path) -> Result<PathBuf, WallpaperError> {
    let mut st = read_state();
    st.insert("gui".into(), gui.to_string_lossy().into_owned());
    write_state(&st)?;
    write_niri_config(cli, Some(gui))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_niri_outputs() {
        let j = r#"{
          "HDMI-A-1": {"name":"HDMI-A-1","modes":[{"width":2560,"height":1440}],"current_mode":0,
                       "logical":{"x":1920,"y":0,"width":2560,"height":1440,"scale":1.0,"transform":"Normal"}},
          "eDP-1": {"name":"eDP-1","modes":[{"width":1280,"height":800},{"width":1920,"height":1080}],"current_mode":1,
                    "logical":{"x":0,"y":0,"width":1536,"height":864,"scale":1.25,"transform":"Normal"}},
          "DP-2": {"name":"DP-2","modes":[{"width":1920,"height":1080}],"current_mode":null,"logical":null},
          "DP-3": {"name":"DP-3","modes":[{"width":1920,"height":1080}],"current_mode":0,
                   "logical":{"x":4480,"y":0,"width":1080,"height":1920,"scale":1.0,"transform":"90"}}
        }"#;
        let m = parse_outputs(j);
        assert_eq!(m.iter().map(|m| (m.name.as_str(), m.width, m.height, m.primary)).collect::<Vec<_>>(),
                   vec![("eDP-1", 1920, 1080, true), ("HDMI-A-1", 2560, 1440, false), ("DP-3", 1080, 1920, false)]);
        assert!(parse_outputs("{}").is_empty());
        assert!(parse_outputs("basura").is_empty());
    }

    #[test]
    fn pauses_like_kde_and_mate() {
        let s = PauseSettings::default();
        let max = (1408.0, 821.0, false);      // maximizada en 1440×900 (márgenes + barra)
        let small = (700.0, 821.0, true);      // media pantalla, enfocada
        assert!(should_pause(&s, &[max], 1440.0, 900.0, false));
        assert!(!should_pause(&s, &[small], 1440.0, 900.0, false));
        assert!(!should_pause(&s, &[], 1440.0, 900.0, false));
        assert!(should_pause(&PauseSettings { pause_mode: 1, ..s }, &[small], 1440.0, 900.0, false));
        assert!(should_pause(&PauseSettings { pause_mode: 2, ..s }, &[(300.0, 300.0, false)], 1440.0, 900.0, false));
        assert!(!should_pause(&PauseSettings { pause_mode: 3, ..s }, &[max], 1440.0, 900.0, false));
        assert!(should_pause(&PauseSettings { pause_mode: 3, ..s }, &[], 1440.0, 900.0, true));   // batería
    }

    #[test]
    fn writes_niri_config_once() {
        let d = std::env::temp_dir().join(format!("nekowall-niri-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("niri")).unwrap();
        std::fs::write(d.join("niri/config.kdl"), "input {}\n").unwrap();
        // write_niri_config usa XDG_CONFIG_HOME
        std::env::set_var("XDG_CONFIG_HOME", &d);
        let f = write_niri_config(Path::new("/usr/bin/nekowall"), Some(Path::new("/usr/bin/nekowall-gui"))).unwrap();
        write_niri_config(Path::new("/usr/bin/nekowall"), Some(Path::new("/usr/bin/nekowall-gui"))).unwrap();
        let main = std::fs::read_to_string(d.join("niri/config.kdl")).unwrap();
        assert_eq!(main.matches("include \"nekowall.kdl\"").count(), 1);
        let kdl = std::fs::read_to_string(f).unwrap();
        assert!(kdl.contains("spawn-at-startup \"/usr/bin/nekowall\" \"_niri-restore\""));
        assert!(kdl.contains("Mod+Alt+W"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
