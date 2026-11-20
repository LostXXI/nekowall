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
    convert_opts(src, max_h, max_fps, false)
}

/// `force`: convertir aunque no cambie el tamaño ni los fps (para pasar AV1/H.265 a H.264, que se
/// reproduce bien en casi cualquier equipo).
pub fn convert_opts(src: &Path, max_h: u32, max_fps: u32, force: bool) -> Result<Converted, String> {
    let info = probe(src)?;
    let (w, h) = target_size(&info, max_h);
    let fps = target_fps(&info, max_fps);
    let out = output_path(src, h, fps);
    let change_fps = (info.fps - fps as f64).abs() > 0.5;
    if !force && !change_fps && h >= (info.height & !1) {
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


/// ¿Se puede crear un archivo en `dir`? (Se prueba de verdad: los permisos solos no bastan.)
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".nekowall-escritura-{}", std::process::id()));
    let ok = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// ¿Hace falta ajustar una imagen de `w`×`h` a una pantalla de `sw`×`sh`? Solo si sobra por algún lado.
pub fn image_needs_fit(w: u32, h: u32, sw: u32, sh: u32) -> bool {
    w > sw || h > sh
}

/// Relleno detrás de la imagen cuando no cubre toda la pantalla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Blur,
    Black,
}

/// Cómo queda la imagen en la pantalla: `sx`/`sy` = píxeles de pantalla por píxel de la imagen en
/// horizontal y en vertical (distintos = imagen estirada); (`x`, `y`) = esquina superior izquierda
/// de la imagen en la pantalla (puede ser negativa).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub sw: u32,
    pub sh: u32,
    pub sx: f64,
    pub sy: f64,
    pub x: f64,
    pub y: f64,
    pub fill: Fill,
}

/// Lo que hace el escritorio por defecto: cubrir la pantalla, centrada.
pub fn cover_frame(iw: u32, ih: u32, sw: u32, sh: u32) -> Frame {
    let scale = (sw as f64 / iw as f64).max(sh as f64 / ih as f64);
    Frame {
        sw, sh, sx: scale, sy: scale,
        x: (sw as f64 - iw as f64 * scale) / 2.0,
        y: (sh as f64 - ih as f64 * scale) / 2.0,
        fill: Fill::Blur,
    }
}

/// Filtro de ffmpeg para un encuadre. Solo se escala la parte visible de la imagen (rápido aunque
/// el zoom sea grande).
pub fn frame_filter(iw: u32, ih: u32, f: &Frame) -> Result<String, String> {
    if f.sx <= 0.0 || f.sy <= 0.0 || !f.sx.is_finite() || !f.sy.is_finite() {
        return Err("escala no válida".into());
    }
    // Parte de la imagen que cae dentro de la pantalla, en píxeles de la imagen.
    let sx0 = (-f.x / f.sx).max(0.0);
    let sy0 = (-f.y / f.sy).max(0.0);
    let sx1 = ((f.sw as f64 - f.x) / f.sx).min(iw as f64);
    let sy1 = ((f.sh as f64 - f.y) / f.sy).min(ih as f64);
    if sx1 - sx0 < 1.0 || sy1 - sy0 < 1.0 {
        return Err("la imagen quedó fuera de la pantalla".into());
    }
    let (cx, cy) = (sx0.floor() as u32, sy0.floor() as u32);
    let cw = ((sx1.ceil() as u32).min(iw) - cx).max(1);
    let ch = ((sy1.ceil() as u32).min(ih) - cy).max(1);
    let dw = ((cw as f64 * f.sx).round() as u32).max(1);
    let dh = ((ch as f64 * f.sy).round() as u32).max(1);
    let px = (f.x + cx as f64 * f.sx).round() as i64;
    let py = (f.y + cy as f64 * f.sy).round() as i64;
    let (sw, sh) = (f.sw, f.sh);
    let fg = format!("crop={cw}:{ch}:{cx}:{cy},scale={dw}:{dh}:flags=lanczos");
    Ok(match f.fill {
        Fill::Blur => format!(
            "[0:v]split=2[a][b];\
             [a]scale={sw}:{sh}:force_original_aspect_ratio=increase,crop={sw}:{sh},gblur=sigma=30,eq=brightness=-0.06[bg];\
             [b]{fg}[fg];[bg][fg]overlay={px}:{py},format=yuvj444p"
        ),
        Fill::Black => format!(
            "color=c=black:s={sw}x{sh}:d=1[bg];[0:v]{fg}[fg];[bg][fg]overlay={px}:{py}:shortest=1,format=yuvj444p"
        ),
    })
}

/// Primer nombre libre: «fondo (1920x1080).jpg», «fondo (1920x1080-2).jpg»… (así un ajuste nuevo
/// nunca pisa el archivo que el escritorio está mostrando).
pub fn free_output_path(dir: &Path, stem: &str, sw: u32, sh: u32) -> PathBuf {
    let first = dir.join(format!("{stem} ({sw}x{sh}).jpg"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem} ({sw}x{sh}-{n}).jpg"))).find(|p| !p.exists()).unwrap()
}

/// Ajusta una imagen a la pantalla y guarda un JPG de máxima calidad junto al original (que no se
/// toca). Sin `frame`: como la muestra el escritorio (cubrir y centrar), y solo si sobra imagen.
/// Si la carpeta del original no admite escritura (p. ej. /usr/share/backgrounds, los fondos del
/// sistema), la copia va a `fallback_dir` (la carpeta nekowallpapers).
pub fn fit_image(src: &Path, sw: u32, sh: u32, fallback_dir: Option<&Path>, frame: Option<Frame>) -> Result<Converted, String> {
    let info = probe(src)?;
    let frame = match frame {
        Some(f) => f,
        None => {
            if !image_needs_fit(info.width, info.height, sw, sh) {
                return Err(format!("no hace falta: la imagen ya es {}×{} (tu pantalla es {sw}×{sh})", info.width, info.height));
            }
            cover_frame(info.width, info.height, sw, sh)
        }
    };
    let filter = frame_filter(info.width, info.height, &frame)?;

    let mut dir = src.parent().unwrap_or(Path::new(".")).to_path_buf();
    if !dir_writable(&dir) {
        dir = fallback_dir.ok_or("no se puede escribir en la carpeta de la imagen")?.to_path_buf();
    }
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let out = free_output_path(&dir, &stem, frame.sw, frame.sh);
    // Oculto mientras se escribe: la revisión de carpetas se salta los archivos que empiezan por «.».
    let tmp = out.with_file_name(format!(
        ".{}.part.jpg",
        out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    ));
    report(10);
    let res = Command::new("nice")
        .args(["-n", "10", "ffmpeg", "-hide_banner", "-v", "error", "-nostdin", "-y", "-i"])
        .arg(src)
        // Máxima calidad de JPG: medido con una ilustración 1915×2456 → 1,7 MB y SSIM 0,979 (con -q:v 2:
        // 0,97 MB pero SSIM 0,956, se notaba en los detalles finos).
        .args(["-filter_complex", &filter, "-frames:v", "1", "-qmin", "1", "-q:v", "1"])
        .arg(&tmp)
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffmpeg: {e}"))?;
    let ok = res.status.success() && std::fs::metadata(&tmp).map(|m| m.len() > 0).unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        let err = String::from_utf8_lossy(&res.stderr);
        return Err(err.lines().last().unwrap_or("ffmpeg falló").to_string());
    }
    std::fs::rename(&tmp, &out).map_err(|e| format!("{}: {e}", out.display()))?;
    report(100);
    Ok(Converted { out, encoder: "ffmpeg (jpg)", reused: false })
}

// Rango de la barra que corresponde a la conversión (p. ej. 60-100 % cuando antes hubo una descarga).
static PROGRESS_BASE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
static PROGRESS_SPAN: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(100);

/// La conversión informará su avance dentro de [base, base+span] de la barra.
pub fn set_progress_range(base: i64, span: i64) {
    PROGRESS_BASE.store(base, std::sync::atomic::Ordering::Relaxed);
    PROGRESS_SPAN.store(span, std::sync::atomic::Ordering::Relaxed);
}

fn report(pct: i64) {
    let base = PROGRESS_BASE.load(std::sync::atomic::Ordering::Relaxed);
    let span = PROGRESS_SPAN.load(std::sync::atomic::Ordering::Relaxed);
    println!("progress {}", base + pct.clamp(0, 100) * span / 100);
    let _ = std::io::stdout().flush();
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
                    report(pct);
                }
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    let ok = status.success() && std::fs::metadata(tmp).map(|m| m.len() > 0).unwrap_or(false);
    if ok {
        report(100);
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
    fn images_fit_only_when_bigger() {
        assert!(image_needs_fit(1915, 2456, 1920, 1080));   // vertical: sobra alto
        assert!(image_needs_fit(3840, 2160, 1920, 1080));
        assert!(!image_needs_fit(1920, 1080, 1920, 1080));
        assert!(!image_needs_fit(1280, 720, 1920, 1080));
    }

    #[test]
    fn frame_filter_cover_and_contain() {
        // Imagen vertical 1915×2456 en 1920×1080, cubriendo: se recorta el centro.
        let f = cover_frame(1915, 2456, 1920, 1080);
        let filt = frame_filter(1915, 2456, &f).unwrap();
        assert!(filt.contains("crop=1915:"), "{filt}");
        assert!(filt.contains("overlay=0:0") || filt.contains("overlay=-1:0") || filt.contains("overlay=0:-1"), "{filt}");
        // Entera (contain): se ve toda, centrada, con relleno a los lados.
        let s = 1080.0 / 2456.0;
        let c = Frame { sw: 1920, sh: 1080, sx: s, sy: s, x: (1920.0 - 1915.0 * s) / 2.0, y: 0.0, fill: Fill::Black };
        let filt = frame_filter(1915, 2456, &c).unwrap();
        assert!(filt.starts_with("color=c=black:s=1920x1080"), "{filt}");
        assert!(filt.contains("crop=1915:2456:0:0"), "{filt}");
        assert!(filt.contains(&format!("overlay={}:0", ((1920.0 - 1915.0 * s) / 2.0).round() as i64)), "{filt}");
        // Estirada: ancho y alto independientes (la imagen entera ocupa justo la pantalla).
        let st = Frame { sw: 1920, sh: 1080, sx: 1920.0 / 1915.0, sy: 1080.0 / 2456.0, x: 0.0, y: 0.0, fill: Fill::Black };
        let filt = frame_filter(1915, 2456, &st).unwrap();
        assert!(filt.contains("crop=1915:2456:0:0,scale=1920:1080"), "{filt}");
        // Fuera de la pantalla: error.
        let out = Frame { x: 5000.0, ..c };
        assert!(frame_filter(1915, 2456, &out).is_err());
    }

    #[test]
    fn free_names_never_overwrite() {
        let d = std::env::temp_dir().join(format!("nekowall-media-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(free_output_path(&d, "f", 1920, 1080), d.join("f (1920x1080).jpg"));
        std::fs::write(d.join("f (1920x1080).jpg"), b"x").unwrap();
        assert_eq!(free_output_path(&d, "f", 1920, 1080), d.join("f (1920x1080-2).jpg"));
        let _ = std::fs::remove_dir_all(&d);
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
