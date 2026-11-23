// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Backend para compositores **wlroots** (Wayfire, sway, labwc, Hyprland, river…), fuera de neko-void.
//!
//! - Pantallas: `wlr-randr --json`.
//! - Imágenes: si **pcmanfm-qt** está dibujando el escritorio (iconos), se le pone el fondo a él
//!   (`pcmanfm-qt --set-wallpaper`): pinta su propio fondo opaco bajo los iconos, así que otro programa de
//!   fondos quedaría tapado o taparía los iconos. Si no, **awww** (si está) o **swaybg** (uno por pantalla).
//! - Vídeos: un **mpvpaper** por pantalla, sin audio y sin la configuración/scripts personales de mpv.
//!   Debajo se pone un fotograma del vídeo como imagen. Con pcmanfm-qt, su fondo pasa a ser un SVG
//!   transparente en mosaico y el vídeo va en la capa «background», debajo: se ven SUS iconos y el vídeo
//!   en movimiento. (Con un fondo de *color* transparente Wayfire deja de pedirle fotogramas al vídeo y se
//!   congela; con el SVG en mosaico no.)
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

/// Cómo se arranca el escritorio de pcmanfm-qt: variables de entorno delante (`env XDG_CURRENT_DESKTOP=LXQt …`)
/// y perfil (`--profile=lxqt`, `--profile lxqt`, `-p lxqt`).
#[derive(Debug, Default, PartialEq)]
pub struct PcmanfmLaunch {
    pub env: Vec<(String, String)>,
    pub profile: Option<String>,
}

pub fn parse_pcmanfm_command(cmd: &str) -> PcmanfmLaunch {
    let mut out = PcmanfmLaunch::default();
    let toks: Vec<String> = cmd.split_whitespace().map(|t| t.trim_matches(|c| c == '"' || c == '\'').to_string()).collect();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t == "env" {
            i += 1;
            continue;
        }
        match t.split_once('=') {
            Some((k, v)) if !k.is_empty() && !k.starts_with('-') && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
                out.env.push((k.to_string(), v.to_string()));
                i += 1;
            }
            _ => break,
        }
    }
    while i < toks.len() {
        let t = &toks[i];
        if let Some(p) = t.strip_prefix("--profile=") {
            out.profile = Some(p.to_string());
        } else if (t == "--profile" || t == "-p") && i + 1 < toks.len() {
            out.profile = Some(toks[i + 1].clone());
            i += 1;
        }
        i += 1;
    }
    out
}

/// Línea de pcmanfm-qt --desktop (activa o comentada por NekoWall) en el `[autostart]` de Wayfire o en un
/// autostart XDG; si no, la del proceso en marcha.
pub fn pcmanfm_launch() -> PcmanfmLaunch {
    for pid in pids_of("pcmanfm-qt") {
        let args = cmdline(pid);
        if args.iter().any(|a| a.starts_with("--desktop")) {
            let mut l = parse_pcmanfm_command(&args.join(" "));
            // El entorno del proceso no se ve en su línea de órdenes: se toma de la del arranque.
            if let Some(cfg) = pcmanfm_autostart_command() {
                l.env = parse_pcmanfm_command(&cfg).env;
            }
            return l;
        }
    }
    pcmanfm_autostart_command().map(|c| parse_pcmanfm_command(&c)).unwrap_or_default()
}

fn is_pcmanfm_desktop_cmd(c: &str) -> bool {
    c.contains("pcmanfm-qt") && (c.contains("--desktop") || c.split_whitespace().any(|t| t == "-d"))
        && !c.contains("--desktop-off") && !c.contains("--desktop-pref")
}

fn pcmanfm_autostart_command() -> Option<String> {
    let ini = std::fs::read_to_string(config_dir().join("wayfire.ini")).unwrap_or_default();
    for line in ini.lines() {
        let l = line.strip_prefix(OFF_MARK).unwrap_or(line).trim();
        if l.starts_with('#') {
            continue;
        }
        if let Some((_, cmd)) = l.split_once('=') {
            if is_pcmanfm_desktop_cmd(cmd) {
                return Some(cmd.trim().to_string());
            }
        }
    }
    for f in xdg_autostart_files() {
        if let Some(exec) = desktop_exec(&f) {
            if is_pcmanfm_desktop_cmd(&exec) {
                return Some(exec);
            }
        }
    }
    None
}

fn xdg_autostart_files() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for d in [config_dir().join("autostart"), PathBuf::from("/etc/xdg/autostart")] {
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut fs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "desktop")).collect();
            fs.sort();
            v.extend(fs);
        }
    }
    v
}

fn desktop_exec(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()?.lines().find_map(|l| l.trim().strip_prefix("Exec=").map(|e| e.trim().to_string()))
}

/// pcmanfm-qt pone su escritorio en la capa «background», la misma que mpvpaper, y dentro de una capa queda
/// encima lo último que se abre. Tras arrancar un vídeo se cierra y se vuelve a abrir su escritorio (en el
/// mismo proceso, con su perfil y entorno) para que sus iconos queden encima del vídeo.
fn raise_pcmanfm_desktop(monitors: &[String]) {
    if !pcmanfm_desktop() {
        return;
    }
    // Esperar a que mpvpaper esté en marcha (su socket) y a que muestre su capa.
    for _ in 0..40 {
        if monitors.iter().all(|m| mpv_socket(m).exists()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    let launch = pcmanfm_launch();
    for arg in ["--desktop-off", "--desktop"] {
        let mut c = Command::new("pcmanfm-qt");
        c.arg(arg).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(p) = &launch.profile {
            c.arg(format!("--profile={p}"));
        }
        for (k, v) in &launch.env {
            c.env(k, v);
        }
        let _ = c.status();
    }
}

/// SVG transparente (el truco de los usuarios de pcmanfm-qt para ver un fondo animado detrás de sus iconos).
fn transparent_svg() -> Result<PathBuf, WallpaperError> {
    let p = cache_dir().join("transparente.svg");
    if !p.is_file() {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"/>\n")?;
    }
    Ok(p)
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
        // P. ej. abierto desde un escritorio lanzado con XDG_CURRENT_DESKTOP=LXQt: el compositor en marcha.
        for (prog, name) in [("wayfire", "wayfire"), ("sway", "sway"), ("labwc", "labwc"), ("Hyprland", "hyprland"),
                             ("river", "river"), ("dwl", "dwl")] {
            if !pids_of(prog).is_empty() {
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
        // Con pcmanfm-qt (iconos): su fondo se vuelve transparente y el vídeo va debajo, así se ven sus iconos.
        if pcmanfm_desktop() {
            let svg = transparent_svg()?;
            run("pcmanfm-qt", &["--set-wallpaper", &svg.to_string_lossy(), "--wallpaper-mode", "tile"])?;
        }
        let pid = spawn_detached(Command::new("mpvpaper").args(["-l", "background", "-o", &opts, monitor]).arg(video))?;
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
        let mut videos = Vec::new();
        for (m, p) in saved {
            let one = [m.clone()];
            let ok = match WallpaperKind::from_path(&p) {
                Some(WallpaperKind::Video) => {
                    if let Some(f) = still_frame(&p).filter(|_| !pcmanfm_desktop()) {
                        let _ = self.set_image(&f, Some(&one), &all, &mut st);
                    }
                    let ok = self.start_video(&m, &p, &mut st).is_ok();
                    if ok { videos.push(m.clone()); }
                    ok
                }
                _ => self.set_image(&p, Some(&one), &all, &mut st).is_ok(),
            };
            if ok { n += 1; }
        }
        write_state(&st)?;
        if !videos.is_empty() {
            raise_pcmanfm_desktop(&videos);
        }
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
                // Con pcmanfm-qt el fotograma iría en SU fondo y taparía el vídeo.
                if let Some(frame) = still_frame(&wallpaper.path).filter(|_| !pcmanfm_desktop()) {
                    let _ = self.set_image(&frame, only, &all, &mut st);
                }
                for t in &targets {
                    self.start_video(t, &wallpaper.path, &mut st)?;
                }
                raise_pcmanfm_desktop(&targets);
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

// ── Iconos del escritorio de NekoWall ───────────────────────────────────────

const OFF_MARK: &str = "# nekowall (desactivado mientras se usan los iconos de NekoWall): ";

/// En `[autostart]`, comenta las líneas que arrancan el escritorio de pcmanfm-qt (`on`) o las restaura (`off`).
pub fn toggle_pcmanfm_autostart(text: &str, on: bool) -> String {
    let mut out = Vec::new();
    let mut in_auto = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_auto = t == "[autostart]";
        }
        if in_auto && on && !t.starts_with('#') && t.contains("pcmanfm-qt") && (t.contains("--desktop") || t.contains(" -d")) {
            out.push(format!("{OFF_MARK}{line}"));
        } else if !on && line.starts_with(OFF_MARK) {
            out.push(line[OFF_MARK.len()..].to_string());
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n") + "\n"
}

/// Quita `clave = …` de una sección.
pub fn ini_remove(text: &str, section: &str, key: &str) -> String {
    let mut out = Vec::new();
    let mut in_sec = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_sec = t == format!("[{section}]");
        }
        if in_sec && t.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n") + "\n"
}

/// Activa o desactiva los iconos del escritorio de NekoWall al iniciar sesión (y ahora mismo).
/// Con `on`, el escritorio de pcmanfm-qt se desactiva (su línea queda comentada y se restaura con `off`).
pub fn set_desktop_icons(on: bool, gui: &Path) -> Result<String, WallpaperError> {
    // Mismo entorno y perfil con que arranca el escritorio de pcmanfm-qt (p. ej. env XDG_CURRENT_DESKTOP=LXQt
    // … --profile=lxqt): así los iconos de NekoWall usan el mismo tema, los mismos ajustes y posiciones.
    let launch = pcmanfm_launch();
    let mut st = read_state();
    st.insert("desktop_icons".into(), on.to_string());
    write_state(&st)?;
    let env_prefix: String = if launch.env.is_empty() {
        String::new()
    } else {
        format!("env {} ", launch.env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "))
    };
    let profile_arg = launch.profile.as_ref().map(|p| format!(" --profile={p}")).unwrap_or_default();
    // Autostart XDG (sway, labwc, Hyprland con xdg-autostart…).
    let auto = config_dir().join("autostart");
    std::fs::create_dir_all(&auto)?;
    let f = auto.join("nekowall-escritorio.desktop");
    if on {
        std::fs::write(&f, format!(
            "[Desktop Entry]\nType=Application\nName=NekoWall (iconos del escritorio)\nExec={env_prefix}\"{}\" --desktop{profile_arg}\nNoDisplay=true\n",
            gui.display()))?;
    } else {
        let _ = std::fs::remove_file(&f);
    }
    toggle_xdg_pcmanfm(on)?;
    // Wayfire: su propio [autostart].
    let ini = config_dir().join("wayfire.ini");
    if ini.is_file() {
        let text = std::fs::read_to_string(&ini)?;
        let mut new = toggle_pcmanfm_autostart(&text, on);
        new = if on {
            ini_set(&new, "autostart", "nekowall_desktop", &format!("{env_prefix}{} --desktop{profile_arg}", gui.display()))
        } else {
            ini_remove(&new, "autostart", "nekowall_desktop")
        };
        if new != text {
            std::fs::write(&ini, new)?;
        }
    }
    // Ahora mismo: cambiar un escritorio por el otro.
    let with_env = |c: &mut Command| {
        for (k, v) in &launch.env {
            c.env(k, v);
        }
    };
    if on {
        for pid in pids_of("pcmanfm-qt") {
            if cmdline(pid).iter().any(|a| a.starts_with("--desktop")) {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
        let mut c = Command::new(gui);
        c.arg("--desktop");
        if let Some(p) = &launch.profile {
            c.arg(format!("--profile={p}"));
        }
        with_env(&mut c);
        let _ = spawn_detached(&mut c);
    } else {
        for pid in pids_of("nekowall-gui") {
            if cmdline(pid).iter().any(|a| a == "--desktop") {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
        if has("pcmanfm-qt") && pcmanfm_autostart_command().is_some() {
            // Si hay un vídeo de fondo, el escritorio de pcmanfm-qt vuelve transparente para que se vea.
            if read_state().keys().any(|k| k.starts_with("pid.")) {
                let _ = set_pcmanfm_wallpaper_file(launch.profile.as_deref(), &transparent_svg()?, "tile");
            }
            let mut c = Command::new("pcmanfm-qt");
            c.arg("--desktop");
            if let Some(p) = &launch.profile {
                c.arg(format!("--profile={p}"));
            }
            with_env(&mut c);
            let _ = spawn_detached(&mut c);
        }
    }
    Ok(if on {
        "Iconos del escritorio de NekoWall activados (encima del fondo animado)".into()
    } else {
        "Iconos del escritorio de NekoWall desactivados: vuelve el escritorio de pcmanfm-qt".into()
    })
}

/// Fondo de pcmanfm-qt escrito en su settings.conf (cuando no está en marcha).
fn set_pcmanfm_wallpaper_file(profile: Option<&str>, file: &Path, mode: &str) -> Result<(), WallpaperError> {
    let conf = config_dir().join("pcmanfm-qt").join(profile.unwrap_or("default")).join("settings.conf");
    let text = std::fs::read_to_string(&conf).unwrap_or_default();
    let new = ini_set(&ini_set(&text, "Desktop", "Wallpaper", &file.to_string_lossy()), "Desktop", "WallpaperMode", mode);
    if let Some(d) = conf.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(conf, new)?;
    Ok(())
}

const XDG_MARK: &str = "X-NekoWall-Disabled=true";

/// Autostart XDG del escritorio de pcmanfm-qt: se desactiva con Hidden=true (en ~/.config/autostart, con una
/// copia si el original es de /etc/xdg/autostart) y se restaura al desactivar los iconos de NekoWall.
fn toggle_xdg_pcmanfm(on: bool) -> Result<(), WallpaperError> {
    let user = config_dir().join("autostart");
    for f in xdg_autostart_files() {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let ours = text.contains(XDG_MARK);
        let is_pcm = desktop_exec(&f).is_some_and(|e| is_pcmanfm_desktop_cmd(&e));
        if !is_pcm {
            continue;
        }
        let in_user = f.starts_with(&user);
        if on && !ours {
            let target = user.join(f.file_name().unwrap_or_default());
            if !in_user && target.exists() {
                continue;                       // ya hay una copia del usuario: se trata esa
            }
            std::fs::create_dir_all(&user)?;
            let mut t = text.trim_end().to_string();
            t.push_str(&format!("\nHidden=true\n{XDG_MARK}\n"));
            std::fs::write(&target, t)?;
        } else if !on && ours && in_user {
            let system = PathBuf::from("/etc/xdg/autostart").join(f.file_name().unwrap_or_default());
            if system.is_file() && std::fs::read_to_string(&system).is_ok_and(|s| !s.contains(XDG_MARK)) {
                let _ = std::fs::remove_file(&f);   // era una copia nuestra del de /etc
            } else {
                let restored: String = text.lines().filter(|l| l.trim() != "Hidden=true" && l.trim() != XDG_MARK)
                    .map(|l| format!("{l}\n")).collect();
                std::fs::write(&f, restored)?;
            }
        }
    }
    Ok(())
}

pub fn desktop_icons_enabled() -> bool {
    read_state().get("desktop_icons").is_some_and(|v| v == "true")
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
    fn swaps_pcmanfm_desktop_in_wayfire_ini() {
        let t = "[autostart]\ndesktop = pcmanfm-qt --desktop\npanel = waybar\n\n[core]\nplugins = x\n";
        let on = toggle_pcmanfm_autostart(t, true);
        assert!(on.contains("# nekowall (desactivado"), "{on}");
        assert!(on.contains("panel = waybar"));
        let on = ini_set(&on, "autostart", "nekowall_desktop", "/b/nekowall-gui --desktop");
        let off = ini_remove(&toggle_pcmanfm_autostart(&on, false), "autostart", "nekowall_desktop");
        assert_eq!(off, t);
    }

    #[test]
    fn parses_pcmanfm_launch_line() {
        let l = parse_pcmanfm_command("env XDG_CURRENT_DESKTOP=LXQt pcmanfm-qt --desktop --profile=lxqt");
        assert_eq!(l.env, vec![("XDG_CURRENT_DESKTOP".to_string(), "LXQt".to_string())]);
        assert_eq!(l.profile.as_deref(), Some("lxqt"));
        let l = parse_pcmanfm_command("/usr/bin/pcmanfm-qt -d -p mio");
        assert!(l.env.is_empty());
        assert_eq!(l.profile.as_deref(), Some("mio"));
        assert_eq!(parse_pcmanfm_command("pcmanfm-qt --desktop").profile, None);
        assert!(is_pcmanfm_desktop_cmd("env A=B pcmanfm-qt --desktop --profile=lxqt"));
        assert!(!is_pcmanfm_desktop_cmd("pcmanfm-qt --desktop-pref"));
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
