// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Backend para compositores **wlroots** (Wayfire, sway, labwc, Hyprland, river…), fuera de neko-void.
//!
//! - Pantallas: `wlr-randr --json`.
//! - Imágenes: si **pcmanfm-qt** está dibujando el escritorio (iconos), se le pone el fondo a él
//!   (`pcmanfm-qt --set-wallpaper`): pinta su propio fondo opaco bajo los iconos, así que otro programa de
//!   fondos quedaría tapado o taparía los iconos. Si no, **awww** (si está) o **swaybg** (uno por pantalla).
//! - Vídeos: un **mpvpaper** por pantalla, sin audio y sin la configuración/scripts personales de mpv.
//!   Debajo se pone un fotograma del vídeo como imagen.
//! - Pausa: el vigilante `nekowall _wlr-video` mira las ventanas con **lswt** (protocolo
//!   foreign-toplevel) y pausa el vídeo según los Ajustes.
//! - Estado: `~/.config/nekowall/wlr.conf`. Al iniciar sesión, `nekowall _wlr-restore` vuelve a poner
//!   los fondos (autostart XDG y, en Wayfire, también su `[autostart]`; ahí además el atajo Meta+Alt+W).

use nekowall_backend::{
    BackendCapabilities, Monitor, Wallpaper, WallpaperBackend, WallpaperError, WallpaperKind,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct WlrBackend;

impl Default for WlrBackend {
    fn default() -> Self {
        Self::new()
    }
}

const COMPOSITORS: &[&str] = &["wayfire", "sway", "labwc", "hyprland", "river", "wlroots", "dwl", "cagebreak"];

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn state_path() -> PathBuf {
    config_dir().join("nekowall/wlr.conf")
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nekowall")
}

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

/// Argumentos de un proceso (separados).
fn cmdline(pid: u32) -> Vec<String> {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|c| c.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect())
        .unwrap_or_default()
}

fn is_program(pid: u32, name: &str) -> bool {
    cmdline(pid).first().is_some_and(|a| a.rsplit('/').next() == Some(name))
}

/// Procesos cuyo programa es `name`.
fn pids_of(name: &str) -> Vec<u32> {
    std::fs::read_dir("/proc").map(|rd| rd.flatten()
        .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
        .filter(|pid| is_program(*pid, name))
        .collect()).unwrap_or_default()
}

/// ¿pcmanfm-qt está dibujando el escritorio (iconos)?
pub fn pcmanfm_desktop() -> bool {
    pids_of("pcmanfm-qt").into_iter().any(|p| cmdline(p).iter().any(|a| a == "--desktop" || a.starts_with("--desktop")))
}

/// Parsea `wlr-randr --json`. Solo pantallas encendidas; la primera (de izquierda a derecha) es la principal.
pub fn parse_outputs(json: &str) -> Vec<Monitor> {
    let Ok(Value::Array(list)) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut mons: Vec<Monitor> = list
        .iter()
        .filter(|o| o["enabled"].as_bool().unwrap_or(true))
        .filter_map(|o| {
            let mode = o["modes"].as_array()?.iter().find(|m| m["current"].as_bool() == Some(true))?;
            let (mut w, mut h) = (mode["width"].as_u64()? as u32, mode["height"].as_u64()? as u32);
            let t = o["transform"].as_str().unwrap_or("normal");
            if t.contains("90") || t.contains("270") {
                std::mem::swap(&mut w, &mut h);
            }
            Some(Monitor {
                name: o["name"].as_str()?.to_string(),
                x: o["position"]["x"].as_i64().unwrap_or(0) as i32,
                y: o["position"]["y"].as_i64().unwrap_or(0) as i32,
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

fn spawn_detached(cmd: &mut Command) -> Result<u32, WallpaperError> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
    let child = cmd.spawn().map_err(|e| WallpaperError::CommandFailed {
        command: format!("{:?}", cmd.get_program()),
        detail: e.to_string(),
    })?;
    Ok(child.id())
}

fn kill_if(st: &mut BTreeMap<String, String>, key: &str, program: &str) {
    if let Some(pid) = st.remove(key).and_then(|p| p.parse::<u32>().ok()) {
        if is_program(pid, program) {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
}

impl WlrBackend {
    pub fn new() -> Self {
        Self
    }

    /// Compositor en uso (minúsculas), según XDG_CURRENT_DESKTOP o sus variables propias.
    pub fn compositor() -> Option<String> {
        if let Ok(d) = std::env::var("XDG_CURRENT_DESKTOP") {
            for part in d.to_ascii_lowercase().split(':') {
                if COMPOSITORS.contains(&part) {
                    return Some(part.to_string());
                }
            }
        }
        for (var, name) in [("WAYFIRE_SOCKET", "wayfire"), ("SWAYSOCK", "sway"),
                            ("HYPRLAND_INSTANCE_SIGNATURE", "hyprland"), ("LABWC_PID", "labwc")] {
            if std::env::var_os(var).is_some() {
                return Some(name.into());
            }
        }
        None
    }

    pub fn is_current_desktop() -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some() && Self::compositor().is_some()
    }

    /// Pone una imagen (`targets` = esas pantallas; `None` = todas).
    fn set_image(&self, path: &Path, targets: Option<&[String]>, all: &[String], st: &mut BTreeMap<String, String>) -> Result<(), WallpaperError> {
        let p = path.to_string_lossy();
        if pcmanfm_desktop() {
            // pcmanfm-qt tiene un solo fondo para todas las pantallas (por línea de órdenes).
            run("pcmanfm-qt", &["--set-wallpaper", &p, "--wallpaper-mode", "zoom"])?;
            return Ok(());
        }
        if has("awww") {
            if run("awww", &["query"]).is_err() {
                let _ = spawn_detached(&mut Command::new("awww-daemon"));
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
            return Ok(());
        }
        if !has("swaybg") {
            return Err(WallpaperError::BackendUnavailable(
                "falta swaybg (o awww) para poner imágenes de fondo".into(),
            ));
        }
        for t in targets.unwrap_or(all) {
            kill_if(st, &format!("bg.{t}"), "swaybg");
            let pid = spawn_detached(Command::new("swaybg").args(["-o", t, "-m", "fill", "-i"]).arg(path))?;
            st.insert(format!("bg.{t}"), pid.to_string());
        }
        Ok(())
    }

    fn stop_video(&self, monitor: &str, st: &mut BTreeMap<String, String>) {
        kill_if(st, &format!("pid.{monitor}"), "mpvpaper");
        let _ = std::fs::remove_file(mpv_socket(monitor));
    }

    fn start_video(&self, monitor: &str, video: &Path, st: &mut BTreeMap<String, String>) -> Result<(), WallpaperError> {
        if !has("mpvpaper") {
            return Err(WallpaperError::BackendUnavailable(
                "falta mpvpaper para los fondos de vídeo (en Arch/Artix está en AUR: yay -S mpvpaper)".into(),
            ));
        }
        self.stop_video(monitor, st);
        let sock = mpv_socket(monitor);
        if let Some(d) = sock.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let opts = format!(
            "config=no load-scripts=no osc=no osd-level=0 osd-bar=no input-cursor=no input-default-bindings=no \
             no-audio loop-file=inf hwdec=auto panscan=1.0 input-ipc-server={}",
            sock.display()
        );
        // Con pcmanfm-qt (iconos), el vídeo tiene que ir por encima de su escritorio para verse.
        let layer = if pcmanfm_desktop() { "bottom" } else { "background" };
        let pid = spawn_detached(Command::new("mpvpaper").args(["-l", layer, "-o", &opts, monitor]).arg(video))?;
        st.insert(format!("pid.{monitor}"), pid.to_string());
        Ok(())
    }

    /// Al iniciar sesión: vuelve a poner los fondos guardados (swaybg y mpvpaper no se recuerdan solos).
    pub fn restore(&self) -> Result<usize, WallpaperError> {
        let mut st = read_state();
        let all: Vec<String> = self.monitors().into_iter().map(|m| m.name).collect();
        let saved: Vec<(String, PathBuf)> = st
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix("mon.")?.to_string(), PathBuf::from(v))))
            .filter(|(m, p)| all.contains(m) && p.is_file())
            .collect();
        let mut n = 0;
        for (m, p) in saved {
            let one = [m.clone()];
            let ok = match WallpaperKind::from_path(&p) {
                Some(WallpaperKind::Video) => {
                    if let Some(f) = still_frame(&p) {
                        let _ = self.set_image(&f, Some(&one), &all, &mut st);
                    }
                    self.start_video(&m, &p, &mut st).is_ok()
                }
                _ => self.set_image(&p, Some(&one), &all, &mut st).is_ok(),
            };
            if ok { n += 1; }
        }
        write_state(&st)?;
        if st.keys().any(|k| k.starts_with("pid.")) {
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

impl WallpaperBackend for WlrBackend {
    fn name(&self) -> &'static str {
        "wlroots"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            static_wallpapers: true,
            video_wallpapers: has("mpvpaper"),
            // Con pcmanfm-qt hay un solo fondo para todas las pantallas.
            per_monitor: !pcmanfm_desktop(),
            pause_video: has("lswt"),
        }
    }

    fn monitors(&self) -> Vec<Monitor> {
        run("wlr-randr", &["--json"]).map(|j| parse_outputs(&j)).unwrap_or_default()
    }

    fn apply(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), WallpaperError> {
        if !wallpaper.path.is_file() {
            return Err(WallpaperError::NotFound(wallpaper.path.clone()));
        }
        let mons = self.monitors();
        if mons.is_empty() {
            return Err(WallpaperError::BackendUnavailable(
                "no puedo leer las pantallas (instala wlr-randr)".into(),
            ));
        }
        let monitor = if pcmanfm_desktop() { None } else { monitor };
        let all: Vec<String> = mons.iter().map(|m| m.name.clone()).collect();
        let targets = self.validate(monitor, &mons)?;
        let only = monitor.map(|_| targets.as_slice());
        let mut st = read_state();
        match wallpaper.kind {
            WallpaperKind::Static => {
                for t in &targets {
                    self.stop_video(t, &mut st);
                }
                self.set_image(&wallpaper.path, only, &all, &mut st)?;
            }
            WallpaperKind::Video => {
                if let Some(frame) = still_frame(&wallpaper.path) {
                    let _ = self.set_image(&frame, only, &all, &mut st);
                }
                for t in &targets {
                    self.start_video(t, &wallpaper.path, &mut st)?;
                }
            }
        }
        for t in &targets {
            st.insert(format!("mon.{t}"), wallpaper.path.to_string_lossy().into_owned());
        }
        write_state(&st)?;
        if wallpaper.kind == WallpaperKind::Video {
            spawn_watcher();
        }
        if let Ok(exe) = std::env::current_exe() {
            let gui = read_state().get("gui").map(PathBuf::from);
            let _ = write_autostart(&exe, gui.as_deref());
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
        Ok(st.get(&format!("mon.{name}")).and_then(|p| Wallpaper::from_path(Path::new(p))))
    }
}

// ── Arranque y atajo ────────────────────────────────────────────────────────

/// Añade (o actualiza) `clave = valor` en la sección `[sección]` de un .ini (lo crea si no existe).
pub fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_sec = false;
    let mut done = false;
    let header = format!("[{section}]");
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if in_sec && !done {
                out.push(format!("{key} = {value}"));
                done = true;
            }
            in_sec = t == header;
        } else if in_sec && t.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            out.push(format!("{key} = {value}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if in_sec && !done {
        out.push(format!("{key} = {value}"));
        done = true;
    }
    if !done {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(header);
        out.push(format!("{key} = {value}"));
    }
    out.join("\n") + "\n"
}

/// Arranque al iniciar sesión (XDG y, en Wayfire, `[autostart]`) y atajo Meta+Alt+W (en Wayfire).
pub fn write_autostart(cli: &Path, gui: Option<&Path>) -> Result<(), WallpaperError> {
    let auto = config_dir().join("autostart");
    std::fs::create_dir_all(&auto)?;
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=NekoWall (fondo de pantalla)\nExec=\"{}\" _wlr-restore\n\
         NoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
        cli.display()
    );
    let f = auto.join("nekowall-fondo.desktop");
    if std::fs::read_to_string(&f).ok().as_deref() != Some(desktop.as_str()) {
        std::fs::write(&f, desktop)?;
    }
    // Wayfire no lee el autostart XDG: su propio wayfire.ini.
    let ini = config_dir().join("wayfire.ini");
    if WlrBackend::compositor().as_deref() == Some("wayfire") || ini.is_file() {
        let text = std::fs::read_to_string(&ini).unwrap_or_default();
        let mut new = ini_set(&text, "autostart", "nekowall", &format!("{} _wlr-restore", cli.display()));
        if let Some(g) = gui {
            new = ini_set(&new, "command", "binding_nekowall", "<super> <alt> KEY_W");
            new = ini_set(&new, "command", "command_nekowall", &format!("{} --hud", g.display()));
        }
        if new != text {
            std::fs::write(&ini, new)?;
        }
    }
    Ok(())
}

/// Guarda la ruta de nekowall-gui (para el atajo) y escribe el arranque y el atajo.
pub fn register_shortcut(cli: &Path, gui: &Path) -> Result<String, WallpaperError> {
    let mut st = read_state();
    st.insert("gui".into(), gui.to_string_lossy().into_owned());
    write_state(&st)?;
    write_autostart(cli, Some(gui))?;
    Ok(match WlrBackend::compositor().as_deref() {
        Some("wayfire") | None => "Atajo Meta+Alt+W registrado en ~/.config/wayfire.ini (Wayfire lo aplica al momento)".into(),
        Some(c) => format!(
            "En {c} añade el atajo a mano: Meta+Alt+W → \"{}\" --hud (ver docs/DETALLES.md)",
            gui.display()
        ),
    })
}

// ── Pausa del vídeo (lswt) ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseSettings {
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

/// Una ventana según lswt: (¿maximizada o a pantalla completa?, ¿activa?, ¿minimizada?, pantallas).
#[derive(Debug, Clone, PartialEq)]
pub struct Toplevel {
    pub big: bool,
    pub active: bool,
    pub minimized: bool,
    pub outputs: Vec<String>,
}

/// Parsea `lswt -j`. Acepta una lista o un objeto con `toplevels`.
pub fn parse_toplevels(json: &str) -> Vec<Toplevel> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let list = v.get("toplevels").cloned().unwrap_or(v);
    let b = |t: &Value, k: &str| t[k].as_bool().unwrap_or(false);
    list.as_array().map(|a| a.iter().map(|t| Toplevel {
        big: b(t, "maximized") || b(t, "fullscreen"),
        active: b(t, "activated"),
        minimized: b(t, "minimized"),
        outputs: t["outputs"].as_array().map(|o| o.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
    }).collect()).unwrap_or_default()
}

/// ¿Pausar el vídeo de la pantalla `m`? Las ventanas sin pantalla conocida cuentan en todas.
pub fn should_pause(s: &PauseSettings, wins: &[Toplevel], m: &str, low_battery: bool) -> bool {
    if s.battery_pauses && low_battery {
        return true;
    }
    let here = |w: &&Toplevel| !w.minimized && (w.outputs.is_empty() || w.outputs.iter().any(|o| o == m));
    match s.pause_mode {
        3 => false,
        1 => wins.iter().filter(here).any(|w| w.active),
        2 => wins.iter().filter(here).next().is_some(),
        _ => wins.iter().filter(here).any(|w| w.big),
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
        pid != me && cmdline(pid).get(1).map(String::as_str) == Some("_wlr-video")
    }))
}

pub fn spawn_watcher() {
    if watcher_running() || !has("lswt") {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = spawn_detached(Command::new(exe).arg("_wlr-video"));
    }
}

/// `nekowall _wlr-video`: pausa y reanuda cada vídeo según las ventanas. Termina cuando no queda ningún
/// mpvpaper de NekoWall. Cada ~5 s reenvía el estado (por si el vídeo quedó en pausa por otra causa).
pub fn run_watcher() -> Result<(), String> {
    let mut paused: BTreeMap<String, bool> = BTreeMap::new();
    let mut ticks: u32 = 0;
    loop {
        let st = read_state();
        let videos: Vec<String> = st
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix("pid.")?, v.parse::<u32>().ok()?)))
            .filter(|(_, pid)| is_program(*pid, "mpvpaper"))
            .map(|(m, _)| m.to_string())
            .collect();
        if videos.is_empty() {
            return Ok(());
        }
        let settings = load_settings();
        let low = settings.battery_pauses && battery_low(settings.battery_level);
        let wins = run("lswt", &["-j"]).map(|j| parse_toplevels(&j)).unwrap_or_default();
        for m in &videos {
            let want = should_pause(&settings, &wins, m, low);
            if (paused.get(m) != Some(&want) || ticks % 6 == 0) && mpv_pause(m, want) {
                paused.insert(m.clone(), want);
            }
        }
        ticks = ticks.wrapping_add(1);
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wlr_randr() {
        let j = r#"[
          {"name":"HDMI-A-1","enabled":true,"modes":[{"width":2560,"height":1440,"refresh":60.0,"preferred":true,"current":true}],
           "position":{"x":1920,"y":0},"transform":"normal","scale":1.0},
          {"name":"eDP-1","enabled":true,"modes":[{"width":1280,"height":800,"current":false},{"width":1920,"height":1080,"current":true}],
           "position":{"x":0,"y":0},"transform":"normal","scale":1.25},
          {"name":"DP-2","enabled":false,"modes":[{"width":1920,"height":1080,"current":true}],"position":{"x":0,"y":0}},
          {"name":"DP-3","enabled":true,"modes":[{"width":1920,"height":1080,"current":true}],"position":{"x":4480,"y":0},"transform":"90"}
        ]"#;
        let m = parse_outputs(j);
        assert_eq!(m.iter().map(|m| (m.name.as_str(), m.width, m.height, m.primary)).collect::<Vec<_>>(),
                   vec![("eDP-1", 1920, 1080, true), ("HDMI-A-1", 2560, 1440, false), ("DP-3", 1080, 1920, false)]);
        assert!(parse_outputs("basura").is_empty());
    }

    #[test]
    fn edits_wayfire_ini() {
        let t = "[core]\nplugins = autostart command\n\n[autostart]\nbg = swaybg -i x\n";
        let r = ini_set(t, "autostart", "nekowall", "/b/nekowall _wlr-restore");
        assert!(r.contains("[autostart]\nbg = swaybg -i x\nnekowall = /b/nekowall _wlr-restore\n"), "{r}");
        let r2 = ini_set(&r, "autostart", "nekowall", "/c/nekowall _wlr-restore");
        assert_eq!(r2.matches("nekowall =").count(), 1);
        assert!(r2.contains("/c/nekowall"));
        let r3 = ini_set(&r2, "command", "binding_nekowall", "<super> <alt> KEY_W");
        assert!(r3.ends_with("[command]\nbinding_nekowall = <super> <alt> KEY_W\n"), "{r3}");
    }

    #[test]
    fn pauses_per_output() {
        let s = PauseSettings::default();
        let max_a = Toplevel { big: true, active: true, minimized: false, outputs: vec!["A".into()] };
        let min_a = Toplevel { big: true, active: false, minimized: true, outputs: vec!["A".into()] };
        let unknown = Toplevel { big: true, active: false, minimized: false, outputs: vec![] };
        assert!(should_pause(&s, &[max_a.clone()], "A", false));
        assert!(!should_pause(&s, &[max_a.clone()], "B", false));
        assert!(!should_pause(&s, &[min_a], "A", false));
        assert!(should_pause(&s, &[unknown], "B", false));
        assert!(!should_pause(&PauseSettings { pause_mode: 3, ..s }, &[max_a], "A", false));
        assert!(should_pause(&s, &[], "A", true));
        let j = r#"{"toplevels":[{"title":"t","app-id":"foot","maximized":true,"activated":false,"minimized":false,"fullscreen":false}]}"#;
        assert_eq!(parse_toplevels(j).len(), 1);
        assert!(parse_toplevels(j)[0].big);
    }
}
