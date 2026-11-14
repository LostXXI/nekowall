//! Análisis (ffprobe) y conversión (ffmpeg) de vídeos.
//! La conversión crea una copia junto al original («<nombre> (1080p30).mp4»); el original no se toca.

use nekowall_backend::WallpaperKind;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
}

impl VideoInfo {
    pub fn to_json(&self, path: &Path) -> Value {
        json!({
            "path": path.to_string_lossy(),
            "name": path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            "width": self.width,
            "height": self.height,
            "fps": (self.fps * 100.0).round() / 100.0,
            "duration": self.duration,
        })
    }
}

/// «30000/1001» → 29.97. Devuelve 0 si no se puede leer.
fn parse_rate(s: &str) -> f64 {
    match s.split_once('/') {
        Some((a, b)) => {
            let (a, b): (f64, f64) = (a.parse().unwrap_or(0.0), b.parse().unwrap_or(0.0));
            if b > 0.0 { a / b } else { 0.0 }
        }
        None => s.parse().unwrap_or(0.0),
    }
}

pub fn probe(path: &Path) -> Result<VideoInfo, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries",
               "stream=width,height,avg_frame_rate,r_frame_rate:format=duration", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let st = &v["streams"][0];
    let width = st["width"].as_u64().unwrap_or(0) as u32;
    let height = st["height"].as_u64().unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return Err("no tiene pista de vídeo".into());
    }
    let mut fps = parse_rate(st["avg_frame_rate"].as_str().unwrap_or(""));
    if fps <= 0.0 {
        fps = parse_rate(st["r_frame_rate"].as_str().unwrap_or(""));
    }
    let duration = v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0);
    Ok(VideoInfo { width, height, fps, duration })
}

/// Vídeos (por extensión) dentro de las rutas dadas, recorriendo carpetas.
pub fn collect_videos(paths: &[String]) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    walk(&e.path(), out);
                }
            }
        } else if WallpaperKind::from_path(p) == Some(WallpaperKind::Video) {
            out.push(p.to_path_buf());
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
        walk(&p, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

/// Tamaño de salida: altura máxima `max_h` sin agrandar nunca, ancho par y proporcional.
pub fn target_size(info: &VideoInfo, max_h: u32) -> (u32, u32) {
    if info.height <= max_h {
        return (info.width & !1, info.height & !1);
    }
    let w = (info.width as f64 * max_h as f64 / info.height as f64 / 2.0).round() as u32 * 2;
    (w, max_h & !1)
}

/// fps de salida: nunca más que el original.
pub fn target_fps(info: &VideoInfo, max_fps: u32) -> u32 {
    let src = info.fps.round() as u32;
    if src == 0 { max_fps } else { src.min(max_fps) }
}

/// «/carpeta/Video 4K.mp4» → «/carpeta/Video 4K (1080p30).mp4»
pub fn output_path(src: &Path, h: u32, fps: u32) -> PathBuf {
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    src.with_file_name(format!("{stem} ({h}p{fps}).mp4"))
}

pub struct Converted {
    pub out: PathBuf,
    pub encoder: &'static str,
    pub reused: bool,
}

/// Convierte con libx264 (ver la medición dentro). Escribe «progress N» (0-100) en stdout para que la interfaz muestre una barra.
pub fn convert(src: &Path, max_h: u32, max_fps: u32) -> Result<Converted, String> {
    let info = probe(src)?;
    let (w, h) = target_size(&info, max_h);
    let fps = target_fps(&info, max_fps);
    let out = output_path(src, h, fps);
    let change_fps = (info.fps - fps as f64).abs() > 0.5;
    if !change_fps && h >= (info.height & !1) {
        return Err(format!(
            "no hace falta: el vídeo ya es {}×{} a {} fps",
            info.width, info.height, info.fps.round()
        ));
    }
    if probe(&out).is_ok() {
        return Ok(Converted { out, encoder: "existente", reused: true });
    }
    let tmp = out.with_file_name(format!(
        ".{}.part.mp4",
        out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    ));

    // Medido en el equipo (T480s, 4K→1080p de 14 s): libx264 veryfast 9,6 s y 3,7 MB con SSIM 0,987,
    // frente a 14-15 s y 7-12 MB por VA-API (scale_vaapi falla al redimensionar con iHD + ffmpeg 6.1,
    // y el camino híbrido es más lento). Por eso se convierte con el procesador, a prioridad baja.
    let filter = match change_fps {
        true => format!("fps={fps},scale={w}:{h}"),
        false => format!("scale={w}:{h}"),
    };
    let args: Vec<String> = vec![
        "-i".into(), src.to_string_lossy().into_owned(),
        "-vf".into(), filter,
        "-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), "21".into(),
        "-pix_fmt".into(), "yuv420p".into(),
    ];
    match run_ffmpeg(&args, &tmp, info.duration) {
        Ok(()) => {
            std::fs::rename(&tmp, &out).map_err(|e| format!("{}: {e}", out.display()))?;
            Ok(Converted { out, encoder: "libx264", reused: false })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn run_ffmpeg(input_args: &[String], tmp: &Path, duration: f64) -> Result<(), String> {
    // `nice`: la conversión usa todos los núcleos; con prioridad baja el escritorio no se traba.
    let mut child = Command::new("nice")
        .args(["-n", "10", "ffmpeg", "-hide_banner", "-v", "error", "-nostdin", "-y", "-nostats", "-progress", "pipe:1"])
        .args(input_args)
        .args(["-map", "0:v:0", "-map", "0:a?", "-c:a", "copy", "-movflags", "+faststart"])
        .arg(tmp)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar ffmpeg: {e}"))?;

    // stderr en un hilo para que no se llene el búfer mientras leemos el progreso.
    let mut stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
        s
    });

    let mut last = -1i64;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
            if duration > 0.0 {
                let pct = ((us / 1e6 / duration) * 100.0).clamp(0.0, 99.0) as i64;
                if pct != last {
                    last = pct;
                    println!("progress {pct}");
                    let _ = std::io::stdout().flush();
                }
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    let ok = status.success() && std::fs::metadata(tmp).map(|m| m.len() > 0).unwrap_or(false);
    if ok {
        println!("progress 100");
        Ok(())
    } else {
        Err(err.lines().last().unwrap_or("ffmpeg falló").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(w: u32, h: u32, fps: f64) -> VideoInfo {
        VideoInfo { width: w, height: h, fps, duration: 10.0 }
    }

    #[test]
    fn sizes_never_upscale_and_stay_even() {
        assert_eq!(target_size(&info(3840, 2160, 30.0), 1080), (1920, 1080));
        assert_eq!(target_size(&info(1920, 1080, 30.0), 1080), (1920, 1080));
        assert_eq!(target_size(&info(1280, 720, 30.0), 1080), (1280, 720));
        assert_eq!(target_size(&info(3440, 1440, 30.0), 1080), (2580, 1080));
        assert_eq!(target_size(&info(1081, 1081, 30.0), 1080), (1080, 1080));
    }

    #[test]
    fn fps_never_increases() {
        assert_eq!(target_fps(&info(1920, 1080, 29.97), 60), 30);
        assert_eq!(target_fps(&info(1920, 1080, 60.0), 30), 30);
        assert_eq!(target_fps(&info(1920, 1080, 24.0), 30), 24);
        assert_eq!(target_fps(&info(1920, 1080, 0.0), 30), 30);
    }

    #[test]
    fn rates_and_names() {
        assert!((parse_rate("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_rate("0/0"), 0.0);
        assert_eq!(parse_rate("25"), 25.0);
        assert_eq!(
            output_path(Path::new("/a/Cherry Pop - 4k.mp4"), 1080, 30),
            PathBuf::from("/a/Cherry Pop - 4k (1080p30).mp4")
        );
    }
}
