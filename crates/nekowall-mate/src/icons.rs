// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Iconos del escritorio de NekoWall en MATE: sustituyen al escritorio de Caja (que no puede ser
//! transparente, así que el vídeo de fondo lo tapa) por `nekowall-gui --desktop`, que imita a Caja con sus
//! mismos ajustes, posiciones (sus metadatos), menús y textos, y se dibuja ENCIMA del vídeo.
//! Al desactivarlo vuelve el escritorio de Caja exactamente como estaba (las posiciones son las suyas).

use crate::video::{config_dir, get_state, set_state};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const STATE_ON: &str = "neko_icons";
const STATE_PREV: &str = "neko_icons_prev";

fn autostart_file() -> PathBuf {
    config_dir().parent().unwrap_or(Path::new(".")).join("autostart/nekowall-escritorio.desktop")
}

fn gsettings(args: &[&str]) -> Option<String> {
    let out = Command::new("gsettings").args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Procesos `nekowall-gui --desktop` de este usuario.
fn our_desktops() -> Vec<i32> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return out };
    for e in rd.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() else { continue };
        let Ok(cmd) = std::fs::read(format!("/proc/{pid}/cmdline")) else { continue };
        let args: Vec<String> = cmd.split(|b| *b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
        let exe_is_gui = args.first().is_some_and(|a| a.ends_with("nekowall-gui"));
        if exe_is_gui && args.iter().any(|a| a == "--desktop") && !args.iter().any(|a| a == "--capture-caja") {
            out.push(pid);
        }
    }
    out
}

fn caja_running() -> bool {
    Command::new("pgrep").args(["-x", "caja"]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Ejecuta `cmd` con un límite de tiempo (si se pasa, se mata).
fn run_bounded(cmd: &mut Command, limit: Duration) -> bool {
    let Ok(mut child) = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return false;
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// El vídeo de fondo cambia de tipo de ventana con los iconos (ver video.rs): se reinicia su vigilante.
fn restart_video() {
    if !crate::video::running() {
        return;
    }
    crate::video::stop();
    if let Ok(exe) = std::env::current_exe() {
        let _ = crate::video::spawn(&exe);
    }
}

pub fn desktop_icons_enabled() -> bool {
    get_state(STATE_ON).is_some_and(|v| v == "true")
}

/// Activa o desactiva los iconos de NekoWall (ahora y al iniciar sesión).
pub fn set_desktop_icons(on: bool, gui: &Path) -> Result<String, String> {
    let f = autostart_file();
    if on {
        let caja_desktop = gsettings(&["get", "org.mate.background", "show-desktop-icons"]);
        // Antes de apagar el escritorio de Caja: guardar dónde tiene ahora cada icono (lo que el usuario ve),
        // porque Caja no siempre guarda las posiciones de los iconos que coloca él solo.
        if caja_desktop.as_deref() == Some("true") && caja_running() {
            run_bounded(Command::new(gui).args(["--desktop", "--capture-caja"]), Duration::from_secs(20));
        }
        if get_state(STATE_PREV).is_none() {
            set_state(STATE_PREV, Some(caja_desktop.as_deref().unwrap_or("true"))).map_err(|e| e.to_string())?;
        }
        set_state(STATE_ON, Some("true")).map_err(|e| e.to_string())?;
        gsettings(&["set", "org.mate.background", "show-desktop-icons", "false"]);
        std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
        let q = format!("\"{}\"", gui.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""));
        std::fs::write(
            &f,
            format!(
                "[Desktop Entry]\nType=Application\nName=NekoWall (iconos del escritorio)\n\
                 Exec={q} --desktop\nOnlyShowIn=MATE;\nNoDisplay=true\nX-MATE-Autostart-Delay=2\n"
            ),
        )
        .map_err(|e| e.to_string())?;
        restart_video();
        if our_desktops().is_empty() {
            Command::new("setsid")
                .arg("-f")
                .arg(gui)
                .arg("--desktop")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|e| format!("no se pudo lanzar {}: {e}", gui.display()))?;
        }
        Ok("Iconos del escritorio de NekoWall activados (encima del fondo animado)".into())
    } else {
        for pid in our_desktops() {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        let _ = std::fs::remove_file(&f);
        let prev = get_state(STATE_PREV).unwrap_or_else(|| "true".into());
        let _ = set_state(STATE_ON, None);
        let _ = set_state(STATE_PREV, None);
        gsettings(&["set", "org.mate.background", "show-desktop-icons", &prev]);
        restart_video();
        Ok("Iconos del escritorio de NekoWall desactivados: vuelve el escritorio de Caja".into())
    }
}
