//! Fondos de vídeo en MATE (X11).
//!
//! Caja dibuja el escritorio en una ventana propia que taparía cualquier vídeo, así que mientras hay
//! un vídeo de fondo se desactiva el escritorio de Caja (`show-desktop-icons false`, lo que oculta los
//! iconos) y se restaura al volver a una imagen. El vídeo lo reproduce `mpv` dentro de una ventana
//! de tipo escritorio creada por `xwinwrap`, debajo de todas las demás.
//!
//! Un proceso propio (`nekowall _mate-video <ruta>`) vigila esa reproducción: pausa el vídeo según las
//! ventanas (mismos modos que el plugin de Plasma) y la batería, hablando con mpv por su socket IPC.
//! Probado en neko-void MATE dentro de una VM (2026-11-15).

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Ajustes de vídeo propios de MATE (en Plasma los guarda el plugin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MateVideoSettings {
    /// 0 = ventana maximizada o a pantalla completa · 1 = ventana activa · 2 = cualquier ventana visible · 3 = nunca
    pub pause_mode: u8,
    pub battery_pauses: bool,
    pub battery_level: u8,
}

impl Default for MateVideoSettings {
    fn default() -> Self {
        Self { pause_mode: 0, battery_pauses: true, battery_level: 10 }
    }
}

// ── Rutas y estado ──────────────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nekowall")
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir())
        .join("nekowall")
}

fn state_file() -> PathBuf {
    config_dir().join("mate.conf")
}

pub fn mpv_socket() -> PathBuf {
    runtime_dir().join("mpv.sock")
}

fn pid_file() -> PathBuf {
    runtime_dir().join("mate-video.pid")
}

fn autostart_file() -> PathBuf {
    config_dir().parent().unwrap_or(Path::new(".")).join("autostart/nekowall-video.desktop")
}

/// Estado persistente sencillo «clave=valor» (vídeo actual, ajustes, iconos de Caja guardados).
fn read_state() -> Vec<(String, String)> {
    std::fs::read_to_string(state_file())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.to_string())))
        .collect()
}

fn get_state(key: &str) -> Option<String> {
    read_state().into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn set_state(key: &str, value: Option<&str>) -> std::io::Result<()> {
    let mut st: Vec<_> = read_state().into_iter().filter(|(k, _)| k != key).collect();
    if let Some(v) = value {
        st.push((key.to_string(), v.to_string()));
    }
    std::fs::create_dir_all(config_dir())?;
    let text: String = st.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(state_file(), text)
}

pub fn load_settings() -> MateVideoSettings {
    let d = MateVideoSettings::default();
    MateVideoSettings {
        pause_mode: get_state("pause_mode").and_then(|v| v.parse().ok()).filter(|m| *m <= 3).unwrap_or(d.pause_mode),
        battery_pauses: get_state("battery_pauses").map(|v| v == "true").unwrap_or(d.battery_pauses),
        battery_level: get_state("battery_level").and_then(|v| v.parse().ok()).filter(|l| *l <= 100).unwrap_or(d.battery_level),
    }
}

pub fn save_settings(s: &MateVideoSettings) -> std::io::Result<()> {
    set_state("pause_mode", Some(&s.pause_mode.to_string()))?;
    set_state("battery_pauses", Some(if s.battery_pauses { "true" } else { "false" }))?;
    set_state("battery_level", Some(&s.battery_level.to_string()))
}

/// Vídeo aplicado actualmente, si su reproducción sigue viva.
pub fn current_video() -> Option<PathBuf> {
    let p = get_state("video")?;
    if running_pid().is_some() { Some(PathBuf::from(p)) } else { None }
}

pub fn remember_video(path: Option<&Path>) -> std::io::Result<()> {
    set_state("video", path.map(|p| p.to_string_lossy()).as_deref())
}

/// Guarda el valor de `show-desktop-icons` la primera vez (para restaurarlo después).
pub fn save_icons_once(current: &str) -> std::io::Result<()> {
    if get_state("caja_icons").is_none() {
        set_state("caja_icons", Some(current))?;
    }
    Ok(())
}

/// Valor guardado de `show-desktop-icons` (y lo olvida).
pub fn take_saved_icons() -> Option<String> {
    let v = get_state("caja_icons");
    let _ = set_state("caja_icons", None);
    v
}

// ── Proceso de reproducción ─────────────────────────────────────────────────

fn running_pid() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(pid_file()).ok()?.trim().parse().ok()?;
    // ¿sigue vivo y es nuestro? (/proc/<pid>/cmdline contiene «_mate-video»)
    let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    String::from_utf8_lossy(&cmd).contains("_mate-video").then_some(pid)
}

/// Detiene la reproducción (el proceso vigilante y, con él, xwinwrap y mpv).
pub fn stop() {
    if let Some(pid) = running_pid() {
        // SIGTERM al grupo de procesos: el vigilante es líder de su grupo (setsid).
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
        for _ in 0..20 {
            if running_pid().is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let _ = std::fs::remove_file(pid_file());
    let _ = std::fs::remove_file(mpv_socket());
}

/// Lanza `<exe> _mate-video <ruta>` en segundo plano, en su propia sesión.
pub fn spawn(exe: &Path, video: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    Command::new("setsid")
        .arg("-f")
        .arg(exe)
        .arg("_mate-video")
        .arg(video)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    for _ in 0..40 {
        if running_pid().is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(std::io::Error::other("el reproductor de vídeo no arrancó (¿están instalados mpv y xwinwrap?)"))
}

/// Arranque del vídeo al iniciar sesión (~/.config/autostart).
pub fn set_autostart(exe: Option<&Path>, video: Option<&Path>) -> std::io::Result<()> {
    let f = autostart_file();
    match (exe, video) {
        (Some(exe), Some(video)) => {
            std::fs::create_dir_all(f.parent().unwrap())?;
            let q = |p: &Path| format!("\"{}\"", p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""));
            std::fs::write(
                f,
                format!(
                    "[Desktop Entry]\nType=Application\nName=NekoWall (fondo de vídeo)\n\
                     Exec={} _mate-video {}\nOnlyShowIn=MATE;\nNoDisplay=true\nX-MATE-Autostart-Delay=3\n",
                    q(exe),
                    q(video)
                ),
            )
        }
        _ => match std::fs::remove_file(f) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

fn xprop(args: &[&str]) -> String {
    Command::new("xprop")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// «… = 1280, 800» → (1280, 800)
fn screen_size() -> (u32, u32) {
    let out = xprop(&["-root", "_NET_DESKTOP_GEOMETRY"]);
    let nums: Vec<u32> = out
        .split('=')
        .nth(1)
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    match nums[..] {
        [w, h, ..] if w > 0 && h > 0 => (w, h),
        _ => (1920, 1080),
    }
}

/// Ids de ventana en una propiedad de xprop («… # 0x1, 0x2»).
fn window_ids(out: &str) -> Vec<String> {
    out.split('#')
        .nth(1)
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("0x") && s != "0x0")
        .collect()
}

/// Estado y tipo de una ventana (una llamada a xprop).
struct Win {
    normal: bool,
    hidden: bool,
    big: bool,
}

fn window(id: &str) -> Win {
    let out = xprop(&["-id", id, "_NET_WM_STATE", "_NET_WM_WINDOW_TYPE"]);
    let ty = out.lines().find(|l| l.starts_with("_NET_WM_WINDOW_TYPE")).unwrap_or("");
    let st = out.lines().find(|l| l.starts_with("_NET_WM_STATE")).unwrap_or("");
    Win {
        // Sin tipo declarado = ventana normal (convención EWMH); desktop/dock/notificaciones no cuentan.
        normal: !ty.contains("=") || ty.contains("_NET_WM_WINDOW_TYPE_NORMAL") || ty.contains("_NET_WM_WINDOW_TYPE_DIALOG"),
        hidden: st.contains("_NET_WM_STATE_HIDDEN"),
        big: st.contains("_NET_WM_STATE_FULLSCREEN")
            || (st.contains("_NET_WM_STATE_MAXIMIZED_VERT") && st.contains("_NET_WM_STATE_MAXIMIZED_HORZ")),
    }
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

/// ¿Debe pausarse el vídeo ahora? (`own` = ventana de xwinwrap, que no cuenta)
fn should_pause(s: &MateVideoSettings, own: Option<&str>) -> bool {
    if s.battery_pauses && battery_low(s.battery_level) {
        return true;
    }
    match s.pause_mode {
        3 => false,
        1 => {
            let active = window_ids(&xprop(&["-root", "_NET_ACTIVE_WINDOW"]));
            active.first().filter(|id| Some(id.as_str()) != own).map(|id| {
                let w = window(id);
                w.normal && !w.hidden
            }) == Some(true)
        }
        m => window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"]))
            .iter()
            .filter(|id| Some(id.as_str()) != own)
            .map(|id| window(id))
            .any(|w| w.normal && !w.hidden && (m == 2 || w.big)),
    }
}

fn mpv_pause(pause: bool) {
    if let Ok(mut s) = UnixStream::connect(mpv_socket()) {
        let _ = writeln!(s, r#"{{"command":["set_property","pause",{pause}]}}"#);
    }
}

/// Bucle del proceso `_mate-video`: lanza xwinwrap+mpv y pausa/reanuda hasta que mpv termine o llegue SIGTERM.
pub fn run_daemon(video: &Path) -> Result<(), String> {
    std::fs::create_dir_all(runtime_dir()).map_err(|e| e.to_string())?;
    if let Some(old) = running_pid() {
        if old != std::process::id() as i32 {
            return Err("ya hay un fondo de vídeo en marcha".into());
        }
    }
    std::fs::write(pid_file(), std::process::id().to_string()).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(mpv_socket());
    let (w, h) = screen_size();
    let mut child: Child = Command::new("xwinwrap")
        .args(["-g", &format!("{w}x{h}+0+0"), "-ni", "-fdt", "-b", "-un", "-s", "-st", "-sp", "-nf", "--"])
        .args(["mpv", "-wid", "WID", "--loop", "--no-audio", "--no-osc", "--no-input-default-bindings",
               "--hwdec=auto-safe", "--panscan=1.0", "--really-quiet", "--no-terminal"])
        .arg(format!("--input-ipc-server={}", mpv_socket().display()))
        .arg(video)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("no se pudo lanzar xwinwrap/mpv: {e}"))?;

    // Ventana propia (la de xwinwrap) para no contarla al decidir la pausa.
    std::thread::sleep(Duration::from_millis(800));
    let own = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"]))
        .into_iter()
        .find(|id| xprop(&["-id", id, "_NET_WM_WINDOW_TYPE"]).contains("_NET_WM_WINDOW_TYPE_DESKTOP"));

    let mut paused = false;
    let mut tick = 0u32;
    let mut settings = load_settings();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            break; // mpv/xwinwrap terminó (ventana cerrada, archivo inválido…)
        }
        if tick % 10 == 0 {
            settings = load_settings(); // recoge cambios hechos desde la interfaz
        }
        let want = should_pause(&settings, own.as_deref());
        if want != paused {
            mpv_pause(want);
            paused = want;
        }
        tick = tick.wrapping_add(1);
        std::thread::sleep(Duration::from_millis(1000));
    }
    let _ = std::fs::remove_file(pid_file());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_window_lists() {
        assert_eq!(
            window_ids("_NET_CLIENT_LIST(WINDOW): window id # 0x2000003, 0x1600001"),
            vec!["0x2000003", "0x1600001"]
        );
        assert_eq!(window_ids("_NET_ACTIVE_WINDOW(WINDOW): window id # 0x0"), Vec::<String>::new());
        assert!(window_ids("_NET_CLIENT_LIST:  not found.").is_empty());
    }
}
