//! Fondos de vídeo en MATE (X11).
//!
//! El vídeo lo reproduce `mpv` dentro de una ventana de tipo escritorio creada por `xwinwrap`, debajo
//! de las ventanas normales pero ENCIMA del escritorio de Caja, y sin recibir clics (`-ni`): los clics
//! llegan a Caja, así que el clic derecho del escritorio y los iconos siguen funcionando aunque no se
//! vean. Caja no puede ser transparente (copia el fondo estático bajo los iconos), por eso los iconos
//! quedan tapados; `nekowall peek` (atajo Meta+Alt+D) oculta y pausa el vídeo para verlos.
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

fn peek_file() -> PathBuf {
    runtime_dir().join("peek")
}

/// Alterna «mostrar escritorio»: devuelve `true` si ahora se ven los iconos (vídeo oculto y en pausa).
pub fn toggle_peek() -> Result<bool, String> {
    if running_pid().is_none() {
        return Err("no hay ningún fondo de vídeo en marcha".into());
    }
    let f = peek_file();
    if f.exists() {
        std::fs::remove_file(&f).map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        std::fs::write(&f, b"1").map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// Fotograma del vídeo (segundo 1, a tamaño completo) en ~/.cache/nekowall/mate-frame.jpg.
pub fn still_frame(video: &Path) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("nekowall");
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join("mate-frame.jpg");
    for seek in ["1", "0"] {
        let ok = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-nostdin", "-ss", seek, "-i"])
            .arg(video)
            .args(["-frames:v", "1", "-q:v", "3"])
            .arg(&out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && std::fs::metadata(&out).map(|m| m.len() > 0).unwrap_or(false) {
            return Some(out);
        }
    }
    None
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
/// (pid, ppid, uid) de todos los procesos, leídos de /proc.
fn processes() -> Vec<(i32, i32, u32, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return out };
    for e in rd.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() else { continue };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        // «pid (comm) estado ppid …»: el nombre puede llevar espacios, se corta tras el último ')'.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else { continue };
        let ppid = rest.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        let uid = std::fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|st| st.lines().find(|l| l.starts_with("Uid:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
            .unwrap_or(u32::MAX);
        let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).map(|c| String::from_utf8_lossy(&c).replace('\0', " ")).unwrap_or_default();
        out.push((pid, ppid, uid, cmd));
    }
    out
}

fn my_uid() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|st| st.lines().find(|l| l.starts_with("Uid:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(u32::MAX)
}

/// Detiene la reproducción: el vigilante y todos sus descendientes (xwinwrap, mpv, xprop), aunque no
/// sea líder de su grupo (p. ej. si lo lanzó el inicio de sesión), y cualquier vigilante perdido.
pub fn stop() {
    let procs = processes();
    let me = std::process::id() as i32;
    let uid = my_uid();
    let mut targets: Vec<i32> = procs
        .iter()
        .filter(|(pid, _, u, cmd)| *u == uid && *pid != me && cmd.contains(" _mate-video "))
        .map(|(pid, ..)| *pid)
        .collect();
    // Añadir descendientes (hasta que no aparezcan más).
    let mut i = 0;
    while i < targets.len() {
        let parent = targets[i];
        for (pid, ppid, ..) in &procs {
            if *ppid == parent && !targets.contains(pid) {
                targets.push(*pid);
            }
        }
        i += 1;
    }
    if !targets.is_empty() {
        let args: Vec<String> = std::iter::once("-TERM".to_string()).chain(targets.iter().map(|p| p.to_string())).collect();
        let _ = Command::new("kill").args(&args).stderr(Stdio::null()).status();
        for _ in 0..20 {
            if targets.iter().all(|p| !Path::new(&format!("/proc/{p}")).exists()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let _ = std::fs::remove_file(pid_file());
    let _ = std::fs::remove_file(mpv_socket());
    let _ = std::fs::remove_file(peek_file());
}

/// Cambia el vídeo del reproductor que ya está en marcha (sin cerrarlo: sin parpadeo).
/// Devuelve `false` si no hay reproductor al que pedírselo.
pub fn switch_to(video: &Path) -> bool {
    if running_pid().is_none() {
        return false;
    }
    let Ok(mut s) = UnixStream::connect(mpv_socket()) else { return false };
    let cmd = format!(
        "{{\"command\":[\"loadfile\",{}]}}\n",
        json_string(&video.to_string_lossy())
    );
    s.write_all(cmd.as_bytes()).is_ok()
}

/// Texto → cadena JSON (para el IPC de mpv).
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
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

fn gsettings(args: &[&str]) -> String {
    Command::new("gsettings")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Si la ventana de escritorio de Caja quedó ENCIMA del vídeo (Marco la sube al cerrarse cualquier
/// ventana), se sube el vídeo con `windowraise`: no toca Caja, así que los iconos no parpadean.
/// Solo si eso no funciona se recrea el escritorio de Caja (Marco pone la nueva debajo).
fn caja_above(own: &str) -> Option<bool> {
    let stack = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST_STACKING"])); // de abajo a arriba
    let video_at = stack.iter().position(|w| w == own)?;
    Some(stack[video_at + 1..].iter().any(|w| xprop(&["-id", w, "WM_CLASS"]).contains("\"desktop_window\"")))
}

fn ensure_video_above_caja(own: &str) {
    if caja_above(own) != Some(true) {
        return;
    }
    xdotool(&["windowraise", own]);
    std::thread::sleep(Duration::from_millis(60));
    if caja_above(own) == Some(true) && gsettings(&["get", "org.mate.background", "show-desktop-icons"]) == "true" {
        gsettings(&["set", "org.mate.background", "show-desktop-icons", "false"]);
        std::thread::sleep(Duration::from_millis(400));
        gsettings(&["set", "org.mate.background", "show-desktop-icons", "true"]);
    }
}

/// Avisa (por el canal) cada vez que cambia el orden de las ventanas: `xprop -spy` imprime una línea
/// por cambio, así se corrige al instante en lugar de esperar al siguiente sondeo.
fn spy_stacking(tx: std::sync::mpsc::Sender<()>) -> Option<Child> {
    let mut child = Command::new("xprop")
        .args(["-root", "-spy", "_NET_CLIENT_LIST_STACKING"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out = child.stdout.take()?;
    std::thread::spawn(move || {
        use std::io::BufRead;
        for _ in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(()).is_err() {
                break;
            }
        }
    });
    Some(child)
}

fn xdotool(args: &[&str]) {
    let _ = Command::new("xdotool").args(args).stdout(Stdio::null()).stderr(Stdio::null()).status();
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
    // Ventanas que ya existían: la del vídeo será la nueva (Caja también es «de escritorio»).
    let before = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"]));
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

    // Ventana propia (la de xwinwrap): nueva, de tipo escritorio y sin WM_CLASS. Se espera hasta 5 s.
    let mut own = None;
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(200));
        own = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"])).into_iter().find(|id| {
            !before.contains(id) && {
                let p = xprop(&["-id", id, "_NET_WM_WINDOW_TYPE", "WM_CLASS"]);
                p.contains("_NET_WM_WINDOW_TYPE_DESKTOP") && !p.contains("desktop_window")
            }
        });
        if own.is_some() {
            break;
        }
    }

    let _ = std::fs::remove_file(peek_file());
    if let Some(id) = own.as_deref() {
        std::thread::sleep(Duration::from_millis(300));
        ensure_video_above_caja(id);
    }

    // Cambios de orden de ventanas → corregir al momento (y además un sondeo de respaldo cada 5 s).
    let (tx, rx) = std::sync::mpsc::channel();
    let mut spy = spy_stacking(tx);

    // Bucle: «mostrar escritorio» y el orden se atienden al momento; la pausa se decide cada segundo.
    let mut paused = false;
    let mut peeking = false;
    let mut tick = 0u32;
    let mut settings = load_settings();
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            break; // mpv/xwinwrap terminó (ventana cerrada, archivo inválido…)
        }
        let peek = peek_file().exists();
        if peek != peeking {
            peeking = peek;
            if let Some(id) = own.as_deref() {
                if peek {
                    mpv_pause(true);
                    xdotool(&["windowunmap", id]);
                } else {
                    xdotool(&["windowmap", id]);
                    std::thread::sleep(Duration::from_millis(300));
                    ensure_video_above_caja(id);
                    paused = !paused; // fuerza a reaplicar el estado de pausa abajo
                }
            }
        }
        if !peeking && tick % 5 == 0 {
            if tick % 50 == 0 {
                settings = load_settings(); // recoge cambios hechos desde la interfaz
            }
            let want = should_pause(&settings, own.as_deref());
            if want != paused {
                mpv_pause(want);
                paused = want;
            }
        }
        if !peeking && tick % 25 == 0 && tick > 0 {
            if let Some(id) = own.as_deref() {
                ensure_video_above_caja(id);
            }
        }
        tick = tick.wrapping_add(1);
        // Espera hasta 200 ms, o menos si cambió el orden de las ventanas.
        if rx.recv_timeout(Duration::from_millis(200)).is_ok() {
            while rx.try_recv().is_ok() {}
            if !peeking {
                if let Some(id) = own.as_deref() {
                    // Subir el vídeo YA (una sola llamada, sin comprobar antes): si ya estaba arriba no
                    // cambia nada. Luego se verifica, por si hiciera falta el último recurso.
                    xdotool(&["windowraise", id]);
                    ensure_video_above_caja(id);
                }
            }
        }
    }
    if let Some(mut c) = spy.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = std::fs::remove_file(peek_file());
    let _ = std::fs::remove_file(pid_file());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_for_mpv() {
        assert_eq!(json_string("/a/b c.mp4"), r#""/a/b c.mp4""#);
        assert_eq!(json_string(r#"/x/"comillas"\barra.mp4"#), r#""/x/\"comillas\"\\barra.mp4""#);
    }

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
