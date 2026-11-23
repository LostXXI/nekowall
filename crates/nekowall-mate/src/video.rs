// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Fondos de vídeo en MATE (X11).
//!
//! El vídeo lo reproduce `mpv` dentro de una ventana de tipo escritorio creada por `xwinwrap`, debajo
//! de las ventanas normales pero ENCIMA del escritorio de Caja, y sin recibir clics (`-ni`): los clics
//! llegan a Caja, así que el clic derecho del escritorio y los iconos siguen funcionando aunque no se
//! vean. Caja no puede ser transparente (copia el fondo estático bajo los iconos), por eso los iconos
//! quedan tapados; `nekowall peek` (atajo Meta+Alt+D) oculta y pausa el vídeo para verlos.
//!
//! Un proceso propio (`nekowall _mate-video`) lleva un reproductor (xwinwrap+mpv) por cada pantalla que
//! tiene un vídeo asignado (`mon.<pantalla>=<ruta>` en mate.conf): los crea, cambia o cierra cuando
//! cambia ese archivo, y pausa cada vídeo según las ventanas de SU pantalla y la batería, hablando con
//! cada mpv por su socket IPC. Probado en neko-void MATE dentro de una VM (2026-11-15; varias
//! pantallas 2026-11-16).

use nekowall_backend::{Monitor, WallpaperKind};
use std::collections::BTreeMap;
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

pub(crate) fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nekowall")
}

pub(crate) fn cache_dir() -> Option<PathBuf> {
    let d = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("nekowall");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
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

/// Socket IPC del mpv de una pantalla.
fn mpv_socket(monitor: &str) -> PathBuf {
    let safe: String = monitor.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    runtime_dir().join(format!("mpv-{safe}.sock"))
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

/// Fotograma del vídeo (segundo 1, a tamaño completo) en ~/.cache/nekowall/frame-<id>.jpg; se reutiliza
/// mientras sea más reciente que el vídeo. Es lo que Caja muestra debajo (y al «mostrar escritorio»).
pub fn still_frame(video: &Path) -> Option<PathBuf> {
    let out = cache_dir()?.join(format!("frame-{}.jpg", nekowall_backend::id_for_path(video)));
    let fresh = |o: &Path| {
        let (Ok(a), Ok(b)) = (std::fs::metadata(o), std::fs::metadata(video)) else { return false };
        a.len() > 0 && matches!((a.modified(), b.modified()), (Ok(ta), Ok(tb)) if ta >= tb)
    };
    if fresh(&out) {
        return Some(out);
    }
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

/// Estado persistente sencillo «clave=valor» (fondo de cada pantalla, ajustes, iconos de Caja guardados).
/// `dir` permite a los tests usar una carpeta propia.
pub(crate) fn read_state_in(dir: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(dir.join("mate.conf"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.to_string())))
        .collect()
}

pub(crate) fn write_state_in(dir: &Path, st: &[(String, String)]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text: String = st.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    // Escritura atómica: el vigilante lee este archivo mientras tanto.
    let tmp = dir.join(".mate.conf.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, dir.join("mate.conf"))
}

fn read_state() -> Vec<(String, String)> {
    read_state_in(&config_dir())
}

fn get_state(key: &str) -> Option<String> {
    read_state().into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn set_state(key: &str, value: Option<&str>) -> std::io::Result<()> {
    let mut st: Vec<_> = read_state().into_iter().filter(|(k, _)| k != key).collect();
    if let Some(v) = value {
        st.push((key.to_string(), v.to_string()));
    }
    write_state_in(&config_dir(), &st)
}

/// Fondo asignado a cada pantalla (`mon.<pantalla>=<ruta>`).
pub(crate) fn assignments_in(dir: &Path) -> BTreeMap<String, PathBuf> {
    read_state_in(dir)
        .into_iter()
        .filter_map(|(k, v)| k.strip_prefix("mon.").map(|m| (m.to_string(), PathBuf::from(v))))
        .collect()
}

pub(crate) fn save_assignments_in(dir: &Path, a: &BTreeMap<String, PathBuf>) -> std::io::Result<()> {
    let mut st: Vec<_> = read_state_in(dir).into_iter().filter(|(k, _)| !k.starts_with("mon.") && k != "video").collect();
    st.extend(a.iter().map(|(m, p)| (format!("mon.{m}"), p.to_string_lossy().into_owned())));
    write_state_in(dir, &st)
}

/// Pantallas con un vídeo asignado.
fn video_assignments() -> BTreeMap<String, PathBuf> {
    assignments_in(&config_dir())
        .into_iter()
        .filter(|(_, p)| WallpaperKind::from_path(p) == Some(WallpaperKind::Video))
        .collect()
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

/// ¿Está en marcha el vigilante de los vídeos?
pub fn running() -> bool {
    running_pid().is_some()
}

/// Vídeo de las versiones de una sola pantalla (`video=`), para migrar.
pub(crate) fn legacy_video_in(dir: &Path) -> Option<PathBuf> {
    read_state_in(dir).into_iter().find(|(k, _)| k == "video").map(|(_, v)| PathBuf::from(v))
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
    (String::from_utf8_lossy(&cmd).split('\0').nth(1) == Some("_mate-video")).then_some(pid)
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
        // Argumentos separados por '\0' (tal cual en /proc), para comparar argumentos exactos.
        let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).map(|c| String::from_utf8_lossy(&c).into_owned()).unwrap_or_default();
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
        // Solo el vigilante de verdad: su PRIMER argumento es «_mate-video» (no cualquier proceso que
        // mencione ese texto, como un `pgrep -f _mate-video` en una terminal).
        .filter(|(pid, _, u, cmd)| *u == uid && *pid != me && cmd.split('\0').nth(1) == Some("_mate-video"))
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
    remove_sockets();
    let _ = std::fs::remove_file(peek_file());
}

fn remove_sockets() {
    if let Ok(rd) = std::fs::read_dir(runtime_dir()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("mpv") && n.ends_with(".sock") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Cambia el vídeo de un reproductor en marcha (sin cerrarlo: sin parpadeo).
fn mpv_loadfile(monitor: &str, video: &Path) -> bool {
    let Ok(mut s) = UnixStream::connect(mpv_socket(monitor)) else { return false };
    let cmd = format!("{{\"command\":[\"loadfile\",{}]}}\n", json_string(&video.to_string_lossy()));
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

/// Lanza `<exe> _mate-video` en segundo plano, en su propia sesión.
pub fn spawn(exe: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    Command::new("setsid")
        .arg("-f")
        .arg(exe)
        .arg("_mate-video")
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

/// Arranque de los vídeos al iniciar sesión (~/.config/autostart). Los vídeos se leen de mate.conf.
pub fn set_autostart(exe: Option<&Path>) -> std::io::Result<()> {
    let f = autostart_file();
    match exe {
        Some(exe) => {
            std::fs::create_dir_all(f.parent().unwrap())?;
            let q = format!("\"{}\"", exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""));
            std::fs::write(
                f,
                format!(
                    "[Desktop Entry]\nType=Application\nName=NekoWall (fondo de vídeo)\n\
                     Exec={q} _mate-video\nOnlyShowIn=MATE;\nNoDisplay=true\nX-MATE-Autostart-Delay=3\n"
                ),
            )
        }
        None => match std::fs::remove_file(f) {
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

/// Salida de `xrandr --listmonitors`:
/// « 0: +*eDP-1 1920/309x1080/174+0+0  eDP-1» → pantalla eDP-1 de 1920×1080 en (0,0), principal.
pub(crate) fn parse_listmonitors(out: &str) -> Vec<Monitor> {
    let mut v = Vec::new();
    for line in out.lines().skip_while(|l| !l.starts_with("Monitors:")).skip(1) {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 3 {
            continue;
        }
        let primary = t[1].contains('*');
        let name = t[1].trim_start_matches(['+', '*']).to_string();
        // «1920/309x1080/174+0+0»
        let g = t[2];
        let Some((wpart, rest)) = g.split_once('x') else { continue };
        let w: u32 = wpart.split('/').next().unwrap_or("").parse().unwrap_or(0);
        let mut it = rest.splitn(2, ['+', '-']);
        let h: u32 = it.next().unwrap_or("").split('/').next().unwrap_or("").parse().unwrap_or(0);
        // Posición: «+X+Y» (también admite negativos «-X+Y»).
        let pos = &rest[rest.find(['+', '-']).unwrap_or(rest.len())..];
        let mut nums = Vec::new();
        let mut cur = String::new();
        for c in pos.chars() {
            if (c == '+' || c == '-') && !cur.is_empty() {
                nums.push(cur.clone());
                cur.clear();
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            nums.push(cur);
        }
        let x = nums.first().and_then(|n| n.parse().ok()).unwrap_or(0);
        let y = nums.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
        if w > 0 && h > 0 && !name.is_empty() {
            v.push(Monitor { name, x, y, width: w, height: h, primary });
        }
    }
    if !v.iter().any(|m| m.primary) {
        if let Some(m) = v.first_mut() {
            m.primary = true;
        }
    }
    v
}

/// Pantallas actuales (xrandr); sin xrandr, una sola del tamaño del escritorio.
pub fn monitors() -> Vec<Monitor> {
    let out = Command::new("xrandr").arg("--listmonitors").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    let v = out.map(|o| parse_listmonitors(&o)).unwrap_or_default();
    if !v.is_empty() {
        return v;
    }
    let (w, h) = screen_size();
    vec![Monitor { name: "default".into(), x: 0, y: 0, width: w, height: h, primary: true }]
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
    id: String,
    normal: bool,
    hidden: bool,
    big: bool,
    /// Centro de la ventana (solo se calcula con varias pantallas).
    center: Option<(i32, i32)>,
}

fn window(id: &str, with_geometry: bool) -> Win {
    let out = xprop(&["-id", id, "_NET_WM_STATE", "_NET_WM_WINDOW_TYPE"]);
    let ty = out.lines().find(|l| l.starts_with("_NET_WM_WINDOW_TYPE")).unwrap_or("");
    let st = out.lines().find(|l| l.starts_with("_NET_WM_STATE")).unwrap_or("");
    let center = with_geometry.then(|| window_center(id)).flatten();
    Win {
        id: id.to_string(),
        // Sin tipo declarado = ventana normal (convención EWMH); desktop/dock/notificaciones no cuentan.
        normal: !ty.contains("=") || ty.contains("_NET_WM_WINDOW_TYPE_NORMAL") || ty.contains("_NET_WM_WINDOW_TYPE_DIALOG"),
        hidden: st.contains("_NET_WM_STATE_HIDDEN"),
        big: st.contains("_NET_WM_STATE_FULLSCREEN")
            || (st.contains("_NET_WM_STATE_MAXIMIZED_VERT") && st.contains("_NET_WM_STATE_MAXIMIZED_HORZ")),
        center,
    }
}

/// Centro de una ventana en coordenadas de la pantalla completa (`xdotool getwindowgeometry --shell`).
fn window_center(id: &str) -> Option<(i32, i32)> {
    let out = Command::new("xdotool").args(["getwindowgeometry", "--shell", id]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)).and_then(|v| v.trim().parse::<i32>().ok());
    Some((get("X=")? + get("WIDTH=")? / 2, get("Y=")? + get("HEIGHT=")? / 2))
}

fn on_monitor(c: (i32, i32), m: &Monitor) -> bool {
    c.0 >= m.x && c.0 < m.x + m.width as i32 && c.1 >= m.y && c.1 < m.y + m.height as i32
}

fn gsettings(args: &[&str]) -> String {
    Command::new("gsettings")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Si la ventana de escritorio de Caja quedó ENCIMA de algún vídeo (Marco la sube al cerrarse cualquier
/// ventana), se suben los vídeos con `windowraise`: no toca Caja, así que los iconos no parpadean.
/// Solo si eso no funciona se recrea el escritorio de Caja (Marco pone la nueva debajo).
fn caja_above(own: &str) -> Option<bool> {
    let stack = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST_STACKING"])); // de abajo a arriba
    let video_at = stack.iter().position(|w| w == own)?;
    Some(stack[video_at + 1..].iter().any(|w| xprop(&["-id", w, "WM_CLASS"]).contains("\"desktop_window\"")))
}

fn ensure_videos_above_caja(own: &[String]) {
    if !own.iter().any(|id| caja_above(id) == Some(true)) {
        return;
    }
    for id in own {
        xdotool(&["windowraise", id]);
    }
    std::thread::sleep(Duration::from_millis(60));
    if own.iter().any(|id| caja_above(id) == Some(true))
        && gsettings(&["get", "org.mate.background", "show-desktop-icons"]) == "true"
    {
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

/// Vuelve a mostrar la ventana del vídeo (tras «mostrar escritorio»). Al ocultarla y mostrarla, Marco
/// olvida su estado: sin esto aparecía en la barra de tareas y fuera de la capa «debajo» (y al subirla
/// podía quedar encima de las ventanas).
fn show_video_window(id: &str) {
    xdotool(&["windowmap", id]);
    for st in ["add,below,sticky", "add,skip_taskbar,skip_pager"] {
        let _ = Command::new("wmctrl").args(["-i", "-r", id, "-b", st])
            .stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
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

/// ¿Debe pausarse el vídeo de la pantalla `m`? Solo cuentan las ventanas de esa pantalla (con una
/// sola pantalla, todas). `own` = ventanas de los vídeos, que no cuentan.
fn should_pause(s: &MateVideoSettings, wins: &[Win], active: Option<&str>, m: &Monitor, multi: bool) -> bool {
    let here = |w: &&Win| !multi || w.center.is_some_and(|c| on_monitor(c, m));
    match s.pause_mode {
        3 => false,
        1 => active.is_some_and(|a| wins.iter().filter(here).any(|w| w.id == a && w.normal && !w.hidden)),
        mode => wins.iter().filter(here).any(|w| w.normal && !w.hidden && (mode == 2 || w.big)),
    }
}

fn mpv_pause(monitor: &str, pause: bool) {
    if let Ok(mut s) = UnixStream::connect(mpv_socket(monitor)) {
        let _ = writeln!(s, r#"{{"command":["set_property","pause",{pause}]}}"#);
    }
}

/// Un reproductor (xwinwrap + mpv) en una pantalla.
struct Player {
    child: Child,
    video: PathBuf,
    geom: Monitor,
    /// Ventana de xwinwrap (para subirla sobre Caja, ocultarla al «mostrar escritorio»…).
    own: Option<String>,
    paused: bool,
}

impl Player {
    fn start(m: &Monitor, video: &Path) -> Result<Self, String> {
        let sock = mpv_socket(&m.name);
        let _ = std::fs::remove_file(&sock);
        // Ventanas que ya existían: la del vídeo será la nueva (Caja también es «de escritorio»).
        let before = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"]));
        let child = Command::new("xwinwrap")
            .args(["-g", &format!("{}x{}+{}+{}", m.width, m.height, m.x, m.y)])
            // Sin -fdt: la ventana del vídeo NO es de tipo «escritorio» sino una ventana normal en la capa
            // «debajo» (-b), que Marco pone siempre por encima del escritorio. Antes, al ser las dos de tipo
            // escritorio, un clic en el escritorio subía Caja y sus iconos se veían un instante.
            .args(["-ni", "-b", "-un", "-s", "-st", "-sp", "-nf", "--"])
            // --no-config y --load-scripts=no: el fondo ignora la configuración personal de mpv del
            // usuario. Sin esto, un script de controles (ModernZ, uosc…) instalado en ~/.config/mpv/scripts
            // dibujaba su barra de reproducción encima del fondo (visto en el equipo de un amigo de Lost).
            .args(["mpv", "-wid", "WID", "--no-config", "--load-scripts=no", "--loop", "--no-audio", "--no-osc",
                   "--osd-level=0", "--no-osd-bar", "--no-input-default-bindings", "--no-input-cursor",
                   "--cursor-autohide=always", "--hwdec=auto-safe", "--panscan=1.0", "--really-quiet", "--no-terminal"])
            .arg(format!("--input-ipc-server={}", sock.display()))
            .arg(video)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("no se pudo lanzar xwinwrap/mpv: {e}"))?;
        // Ventana propia: nueva, sin WM_CLASS (xwinwrap no lo pone) y en la capa «debajo» (o de tipo
        // escritorio, en versiones anteriores). Se espera hasta 5 s.
        let mut own = None;
        for _ in 0..25 {
            std::thread::sleep(Duration::from_millis(200));
            own = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"])).into_iter().find(|id| {
                !before.contains(id) && {
                    let p = xprop(&["-id", id, "_NET_WM_WINDOW_TYPE", "_NET_WM_STATE", "WM_CLASS"]);
                    !p.contains("WM_CLASS(STRING)")
                        && (p.contains("_NET_WM_STATE_BELOW") || p.contains("_NET_WM_WINDOW_TYPE_DESKTOP"))
                }
            });
            if own.is_some() {
                break;
            }
        }
        Ok(Self { child, video: video.to_path_buf(), geom: m.clone(), own, paused: false })
    }

    /// Cierra xwinwrap y su mpv.
    fn stop(mut self) {
        let pid = self.child.id() as i32;
        let kids: Vec<String> = processes().iter().filter(|(_, pp, ..)| *pp == pid).map(|(p, ..)| p.to_string()).collect();
        let _ = Command::new("kill").arg("-TERM").args(&kids).arg(pid.to_string()).stderr(Stdio::null()).status();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(mpv_socket(&self.geom.name));
    }
}

fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Bucle del proceso `_mate-video`: mantiene un reproductor por cada pantalla con vídeo y los pausa o
/// reanuda. Termina cuando ninguna pantalla tiene vídeo, o con SIGTERM.
/// `legacy`: ruta que pasaban las versiones de una sola pantalla (autostart antiguo).
pub fn run_daemon(legacy: Option<&Path>) -> Result<(), String> {
    std::fs::create_dir_all(runtime_dir()).map_err(|e| e.to_string())?;
    if let Some(old) = running_pid() {
        if old != std::process::id() as i32 {
            return Err("ya hay un fondo de vídeo en marcha".into());
        }
    }
    // Versión antigua: un único vídeo para todas las pantallas.
    if let Some(v) = legacy {
        if assignments_in(&config_dir()).is_empty() {
            let a = monitors().into_iter().map(|m| (m.name, v.to_path_buf())).collect();
            save_assignments_in(&config_dir(), &a).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(pid_file(), std::process::id().to_string()).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(peek_file());

    let mut players: BTreeMap<String, Player> = BTreeMap::new();
    // Vídeos que no arrancan (archivo dañado…): no se reintentan hasta que cambie la asignación.
    let mut failed: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut screens = monitors();
    let mut seen_state = None;

    let (tx, rx) = std::sync::mpsc::channel();
    let mut spy = spy_stacking(tx);

    let mut peeking = false;
    let mut tick = 0u32;
    let mut settings = load_settings();
    loop {
        // 1) Pantallas o asignaciones cambiadas → crear, cambiar o cerrar reproductores.
        let state_now = mtime(&state_file());
        let screens_changed = tick % 25 == 0 && tick > 0 && {
            let now = monitors();
            let changed = now != screens;
            if changed {
                screens = now;
                crate::rerender_background(); // la imagen de Caja depende de la disposición
            }
            changed
        };
        if state_now != seen_state || screens_changed {
            seen_state = state_now;
            let want = video_assignments();
            failed.retain(|m, v| want.get(m) == Some(v));
            // Quitar los que sobran (la imagen de Caja ya se cambió antes de escribir el estado).
            let gone: Vec<String> = players
                .iter()
                .filter(|(m, p)| want.get(*m).is_none() || !screens.iter().any(|s| s == &p.geom))
                .map(|(m, _)| m.clone())
                .collect();
            for m in gone {
                if let Some(p) = players.remove(&m) {
                    p.stop();
                }
            }
            for (m, v) in &want {
                let Some(geom) = screens.iter().find(|s| &s.name == m) else { continue };
                match players.get_mut(m) {
                    Some(p) if &p.video != v => {
                        if mpv_loadfile(m, v) {
                            p.video = v.clone();
                        }
                    }
                    Some(_) => {}
                    None if failed.get(m) == Some(v) => {}
                    None => match Player::start(geom, v) {
                        Ok(p) => {
                            if peeking {
                                if let Some(id) = &p.own {
                                    xdotool(&["windowunmap", id]);
                                }
                                mpv_pause(m, true);
                            }
                            players.insert(m.clone(), p);
                        }
                        Err(_) => {
                            failed.insert(m.clone(), v.clone());
                        }
                    },
                }
            }
            if want.is_empty() {
                break;
            }
            let own: Vec<String> = players.values().filter_map(|p| p.own.clone()).collect();
            std::thread::sleep(Duration::from_millis(300));
            ensure_videos_above_caja(&own);
        }

        // 2) Reproductores que terminaron solos (archivo inválido…).
        let dead: Vec<String> = players
            .iter_mut()
            .filter_map(|(m, p)| matches!(p.child.try_wait(), Ok(Some(_))).then(|| m.clone()))
            .collect();
        for m in dead {
            if let Some(p) = players.remove(&m) {
                failed.insert(m, p.video.clone());
            }
        }
        if players.is_empty() && failed.len() >= video_assignments().len() {
            break;
        }
        let own: Vec<String> = players.values().filter_map(|p| p.own.clone()).collect();

        // 3) «Mostrar escritorio» (Meta+Alt+D): todas las pantallas a la vez.
        let peek = peek_file().exists();
        if peek != peeking {
            peeking = peek;
            for (m, p) in players.iter_mut() {
                if let Some(id) = &p.own {
                    if peek {
                        mpv_pause(m, true);
                        xdotool(&["windowunmap", id]);
                    } else {
                        show_video_window(id);
                    }
                }
                p.paused = !p.paused; // fuerza a reaplicar el estado de pausa abajo
            }
            if !peek {
                std::thread::sleep(Duration::from_millis(300));
                ensure_videos_above_caja(&own);
            }
        }

        // 4) Pausa: cada segundo, por pantalla.
        if !peeking && tick % 5 == 0 && !players.is_empty() {
            if tick % 50 == 0 {
                settings = load_settings(); // recoge cambios hechos desde la interfaz
            }
            let battery = settings.battery_pauses && battery_low(settings.battery_level);
            let multi = screens.len() > 1;
            let (wins, active) = if battery || settings.pause_mode == 3 {
                (Vec::new(), None)
            } else {
                let wins: Vec<Win> = window_ids(&xprop(&["-root", "_NET_CLIENT_LIST"]))
                    .iter()
                    .filter(|id| !own.contains(id))
                    .map(|id| window(id, multi))
                    .collect();
                let active = window_ids(&xprop(&["-root", "_NET_ACTIVE_WINDOW"])).into_iter().next();
                (wins, active)
            };
            for (m, p) in players.iter_mut() {
                let want = battery || should_pause(&settings, &wins, active.as_deref(), &p.geom, multi);
                // Cada 5 s se vuelve a enviar aunque no haya cambiado: si el vídeo quedó en pausa (o en
                // marcha) por otra causa, se corrige solo. Enviar el mismo estado no tiene efecto.
                if want != p.paused || tick % 25 == 0 {
                    mpv_pause(m, want);
                    p.paused = want;
                }
            }
        }
        if !peeking && tick % 25 == 0 && tick > 0 {
            ensure_videos_above_caja(&own);
        }
        tick = tick.wrapping_add(1);
        // Espera hasta 200 ms, o menos si cambió el orden de las ventanas.
        if rx.recv_timeout(Duration::from_millis(200)).is_ok() {
            while rx.try_recv().is_ok() {}
            if !peeking {
                // Subir los vídeos YA (sin comprobar antes): si ya estaban arriba no cambia nada.
                for id in &own {
                    xdotool(&["windowraise", id]);
                }
                ensure_videos_above_caja(&own);
            }
        }
    }
    for (_, p) in std::mem::take(&mut players) {
        p.stop();
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
    fn parses_xrandr_monitors() {
        let out = "Monitors: 2\n 0: +*eDP-1 1920/309x1080/174+0+0  eDP-1\n 1: +HDMI-1 2560/597x1440/336+1920+0  HDMI-1\n";
        let m = parse_listmonitors(out);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], Monitor { name: "eDP-1".into(), x: 0, y: 0, width: 1920, height: 1080, primary: true });
        assert_eq!(m[1], Monitor { name: "HDMI-1".into(), x: 1920, y: 0, width: 2560, height: 1440, primary: false });
        // Monitores virtuales (xrandr --setmonitor) y posiciones negativas.
        let m = parse_listmonitors("Monitors: 1\n 0: IZQ 1280/0x1080/0-1280+0\n");
        assert_eq!((m[0].name.as_str(), m[0].x, m[0].y, m[0].primary), ("IZQ", -1280, 0, true));
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
