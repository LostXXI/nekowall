// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
//! Descargar un fondo desde un enlace («Pegar enlace» en Workshop).
//!
//! Con yt-dlp (si está instalado) se entienden cientos de sitios (YouTube, Reddit, X, Vimeo, páginas con
//! un vídeo incrustado…) y se listan sus formatos. Sin yt-dlp, o si yt-dlp no reconoce la página, solo
//! funcionan los enlaces directos a un archivo de vídeo o imagen.
//!
//! Para cada altura se ofrece un único formato, el más liviano de reproducir: H.264 si existe (casi
//! todos los equipos lo decodifican por hardware), si no VP9; AV1 solo si no hay otro (muchos equipos
//! no lo decodifican por hardware y gastaría mucha CPU). Siempre sin audio: un fondo no lo necesita.
//! Si el sitio da una sola versión, se ofrecen además versiones más pequeñas que NekoWall crea
//! convirtiendo el vídeo tras descargarlo.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VIDEO_EXT: &[&str] = &["mp4", "webm", "mkv", "mov", "m4v"];
const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp"];
const HEIGHTS: &[u32] = &[480, 720, 1080, 1440, 2160];

#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub ext: String,
    pub codec: String,      // «H.264», «VP9», «AV1», «imagen»…
    pub size: Option<u64>,  // bytes (aproximado si `approx`)
    pub approx: bool,
    pub convert_to: u32,    // >0 = versión creada por NekoWall convirtiendo el original a esa altura
    pub reencode: bool,     // misma altura, pero pasada a H.264 (el original es AV1/H.265)
}

impl Format {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id, "width": self.width, "height": self.height, "fps": self.fps.round(),
            "ext": self.ext, "codec": self.codec, "size": self.size, "approx": self.approx,
            "convert_to": self.convert_to, "reencode": self.reencode,
        })
    }
}

fn has_ytdlp() -> bool {
    Command::new("yt-dlp").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn ext_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    path.rsplit('.').next().filter(|e| e.len() <= 5 && !e.contains('/')).unwrap_or("").to_ascii_lowercase()
}

fn codec_name(vcodec: &str) -> &'static str {
    let v = vcodec.to_ascii_lowercase();
    if v.starts_with("avc") || v.starts_with("h264") { "H.264" }
    else if v.starts_with("vp09") || v.starts_with("vp9") { "VP9" }
    else if v.starts_with("av01") || v.starts_with("av1") { "AV1" }
    else if v.starts_with("hev") || v.starts_with("hvc") || v.starts_with("h265") { "H.265" }
    else if v.starts_with("vp8") { "VP8" }
    else { "vídeo" }
}

/// Menor = mejor para un fondo (lo que menos cuesta reproducir).
fn codec_rank(codec: &str) -> u8 {
    match codec { "H.264" => 0, "VP9" => 1, "H.265" => 2, "VP8" => 3, "AV1" => 4, _ => 5 }
}

/// Elige un formato por altura de la lista de yt-dlp (solo vídeo, descarga directa por http).
pub fn pick_formats(formats: &[Value]) -> Vec<Format> {
    let mut best: Vec<Format> = Vec::new();
    for f in formats {
        let vcodec = f["vcodec"].as_str().unwrap_or("");
        let height = f["height"].as_u64().unwrap_or(0) as u32;
        let proto = f["protocol"].as_str().unwrap_or("https");
        // Menos de 360p no sirve como fondo.
        if vcodec == "none" || height < 360 || !(proto == "https" || proto == "http") {
            continue;
        }
        let fmt = Format {
            id: f["format_id"].as_str().unwrap_or("").to_string(),
            width: f["width"].as_u64().unwrap_or(0) as u32,
            height,
            fps: f["fps"].as_f64().unwrap_or(0.0),
            ext: f["ext"].as_str().unwrap_or("mp4").to_string(),
            codec: codec_name(vcodec).to_string(),
            size: f["filesize"].as_u64().or(f["filesize_approx"].as_u64()),
            approx: f["filesize"].as_u64().is_none(),
            convert_to: 0,
            reencode: false,
        };
        match best.iter_mut().find(|b| b.height == height) {
            Some(b) => {
                // Mejor códec; a igualdad, más fps.
                let better = (codec_rank(&fmt.codec), -(fmt.fps as i64)) < (codec_rank(&b.codec), -(b.fps as i64));
                if better { *b = fmt; }
            }
            None => best.push(fmt),
        }
    }
    best.sort_by_key(|f| f.height);
    best
}

/// Si solo hay una versión, se añaden versiones más pequeñas creadas por NekoWall.
pub fn add_conversions(mut list: Vec<Format>) -> Vec<Format> {
    if list.len() != 1 || list[0].codec == "imagen" {
        return list;
    }
    let src = list[0].clone();
    // AV1 y H.265 se reproducen mal sin decodificación por hardware (y el plugin de vídeo de KDE no los
    // reproduce en esos equipos): se ofrece la misma altura pasada a H.264.
    if src.codec == "AV1" || src.codec == "H.265" {
        list.insert(0, Format {
            codec: "H.264".into(), ext: "mp4".into(), convert_to: src.height, reencode: true, approx: true,
            ..src.clone()
        });
    }
    for &h in HEIGHTS.iter().rev() {
        if h + 16 < src.height {
            let ratio = (h as f64 / src.height as f64).powi(2);
            list.insert(0, Format {
                width: ((src.width as f64 * h as f64 / src.height as f64) as u32) & !1,
                height: h,
                size: src.size.map(|s| (s as f64 * ratio).max(1.0) as u64),
                approx: true,
                convert_to: h,
                codec: "H.264".into(),
                ext: "mp4".into(),
                reencode: false,
                ..src.clone()
            });
        }
    }
    // De menor a mayor; a igual altura, primero la versión H.264.
    list.sort_by_key(|f| (f.height, f.codec != "H.264"));
    list
}

/// Datos de un enlace directo (sin yt-dlp): tamaño con ffprobe.
fn probe_direct(url: &str) -> Result<Value, String> {
    let ext = ext_of(url);
    let is_image = IMAGE_EXT.contains(&ext.as_str());
    if !is_image && !VIDEO_EXT.contains(&ext.as_str()) {
        return Err(if has_ytdlp() {
            "no encontré ningún vídeo en ese enlace".into()
        } else {
            "ese enlace no es un archivo de vídeo. Para páginas (YouTube, Reddit…) instala yt-dlp".into()
        });
    }
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries",
               "stream=width,height,avg_frame_rate,codec_name:format=duration,size", "-of", "json", url])
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffprobe: {e}"))?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "no se pudo leer el archivo del enlace".to_string())?;
    let st = &v["streams"][0];
    let (w, h) = (st["width"].as_u64().unwrap_or(0) as u32, st["height"].as_u64().unwrap_or(0) as u32);
    if w == 0 || h == 0 {
        return Err("no se pudo leer el archivo del enlace".into());
    }
    let fps = st["avg_frame_rate"].as_str().and_then(|r| {
        let (a, b) = r.split_once('/')?;
        let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
        (b > 0.0).then(|| a / b)
    }).unwrap_or(0.0);
    let name = url.split(['?', '#']).next().unwrap_or(url).rsplit('/').next().unwrap_or("fondo").to_string();
    let f = Format {
        id: "directo".into(), width: w, height: h, fps,
        ext: ext.clone(),
        codec: if is_image { "imagen".into() } else { codec_name(st["codec_name"].as_str().unwrap_or("")).into() },
        size: v["format"]["size"].as_str().and_then(|s| s.parse().ok()),
        approx: false, convert_to: 0, reencode: false,
    };
    let preview_codec = f.codec.clone();
    Ok(json!({
        "title": name.rsplit_once('.').map(|(a, _)| a).unwrap_or(&name),
        "duration": v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()),
        "thumbnail": if is_image { url } else { "" },
        "preview": if is_image { "" } else { url },
        "preview_codec": preview_codec,
        "image": is_image,
        "webpage_url": url,
        "formats": add_conversions(vec![f]).iter().map(Format::to_json).collect::<Vec<_>>(),
    }))
}

/// `nekowall link-info <url>`: título, miniatura, vista previa y formatos (de menor a mayor).
pub fn info(url: &str) -> Result<Value, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("pega un enlace que empiece por https://".into());
    }
    // Enlace directo a un archivo: ffprobe lo lee en ~0,2 s (yt-dlp tardaría ~5 s en rendirse).
    let ext = ext_of(url);
    if VIDEO_EXT.contains(&ext.as_str()) || IMAGE_EXT.contains(&ext.as_str()) {
        if let Ok(v) = probe_direct(url) {
            return Ok(v);
        }
    }
    if !has_ytdlp() {
        return probe_direct(url);
    }
    let out = Command::new("yt-dlp")
        .args(["-J", "--no-playlist", "--no-warnings", "--socket-timeout", "20", url])
        .output()
        .map_err(|e| format!("no se pudo ejecutar yt-dlp: {e}"))?;
    let Ok(d) = serde_json::from_slice::<Value>(&out.stdout) else {
        // yt-dlp no reconoce la página: quizá es un enlace directo.
        return probe_direct(url);
    };
    let d = if d["_type"] == "playlist" { d["entries"][0].clone() } else { d };
    let raw: Vec<Value> = match d["formats"].as_array() {
        Some(a) => a.clone(),
        None => vec![d.clone()],
    };
    let formats = pick_formats(&raw);
    // Páginas genéricas: yt-dlp encuentra el vídeo pero no su tamaño → se mira con ffprobe.
    if formats.is_empty() {
        if let Some(u) = raw.iter().filter_map(|f| f["url"].as_str()).find(|u| VIDEO_EXT.contains(&ext_of(u).as_str())) {
            let mut v = probe_direct(u)?;
            v["title"] = d["title"].clone();
            v["thumbnail"] = d["thumbnail"].clone();
            v["webpage_url"] = json!(url);
            v["direct_url"] = json!(u);
            return Ok(v);
        }
        return Err("no encontré ningún vídeo en ese enlace".into());
    }
    let formats = add_conversions(formats);
    // Vista previa: el formato más pequeño de al menos 360p en H.264 (carga rápido y casi todo lo reproduce).
    let preview = raw.iter()
        .filter(|f| f["protocol"].as_str().is_some_and(|p| p == "https" || p == "http"))
        .filter(|f| f["vcodec"].as_str().is_some_and(|v| v != "none"))
        .filter(|f| f["height"].as_u64().unwrap_or(0) >= 360)
        .min_by_key(|f| (codec_rank(codec_name(f["vcodec"].as_str().unwrap_or(""))), f["height"].as_u64().unwrap_or(0)))
        ;
    let preview_codec = preview.map(|f| codec_name(f["vcodec"].as_str().unwrap_or(""))).unwrap_or("");
    let preview = preview.and_then(|f| f["url"].as_str()).unwrap_or("");
    Ok(json!({
        "title": d["title"], "duration": d["duration"], "thumbnail": d["thumbnail"],
        "webpage_url": d["webpage_url"].as_str().unwrap_or(url), "preview": preview,
        "preview_codec": preview_codec, "image": false,
        "formats": formats.iter().map(Format::to_json).collect::<Vec<_>>(),
    }))
}

/// `nekowall link-preview <url>`: vista previa corta (10 s, 360p, H.264) hecha con ffmpeg, para
/// vídeos que el reproductor de Qt no puede abrir (p. ej. AV1 sin decodificación por hardware).
pub fn make_preview(url: &str, out: &Path) -> Result<PathBuf, String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = out.with_extension("part.mp4");
    let res = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-nostdin", "-y", "-t", "10", "-i", url,
               "-vf", "scale=-2:360", "-an", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30",
               "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(&tmp)
        .output()
        .map_err(|e| format!("no se pudo ejecutar ffmpeg: {e}"))?;
    if !res.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(String::from_utf8_lossy(&res.stderr).lines().last().unwrap_or("ffmpeg falló").to_string());
    }
    std::fs::rename(&tmp, out).map_err(|e| e.to_string())?;
    Ok(out.to_path_buf())
}

/// Nombre de archivo seguro a partir del título.
pub fn safe_name(title: &str) -> String {
    let mut s: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_()[].,'!".contains(c) { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    s = s.trim_matches(['.', ' ']).to_string();
    if s.chars().count() > 80 {
        s = s.chars().take(80).collect::<String>().trim_end().to_string();
    }
    if s.is_empty() { "fondo".into() } else { s }
}

fn free_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem} ({n}).{ext}"))).find(|p| !p.exists()).unwrap()
}

/// Tamaño del archivo según el servidor (cabecera Content-Length tras las redirecciones).
fn content_length(url: &str) -> Option<u64> {
    let out = Command::new("curl").args(["-sIL", "-A", "NekoWall/0.1", "--max-time", "15", url]).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<u64>().ok()).flatten()
        })
        .last()
}

fn progress(p: i64) {
    println!("progress {}", p.clamp(0, 100));
    let _ = std::io::stdout().flush();
}

/// `nekowall link-download <url> --format ID [--convert H] --title T --out DIR`.
/// Descarga a una carpeta temporal (para que la carpeta vigilada no vea archivos a medias), convierte
/// si hace falta y mueve el resultado a `out_dir`. Devuelve la ruta final.
pub fn download(url: &str, format_id: &str, convert_to: u32, reencode: bool, title: &str, out_dir: &Path, tmp_dir: &Path) -> Result<PathBuf, String> {
    let r = download_inner(url, format_id, convert_to, reencode, title, out_dir, tmp_dir);
    let _ = std::fs::remove_dir_all(tmp_dir);   // también si falló: no deja restos
    r
}

fn download_inner(url: &str, format_id: &str, convert_to: u32, reencode: bool, title: &str, out_dir: &Path, tmp_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let _ = std::fs::remove_dir_all(tmp_dir);
    std::fs::create_dir_all(tmp_dir).map_err(|e| format!("{}: {e}", tmp_dir.display()))?;
    // Con conversión, la descarga es solo la mitad del trabajo.
    let scale = |p: f64| if convert_to > 0 { (p * 0.6) as i64 } else { p as i64 };

    let downloaded: PathBuf = if format_id == "directo" || !has_ytdlp() {
        let ext = match ext_of(url).as_str() { "" => "mp4".to_string(), e => e.to_string() };
        let dest = tmp_dir.join(format!("descarga.{ext}"));
        let total = content_length(url);
        let mut child = Command::new("curl")
            .args(["-fL", "--retry", "3", "-sS", "-A", "NekoWall/0.1", "-o"])
            .arg(&dest)
            .arg(url)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("no se pudo ejecutar curl: {e}"))?;
        // Avance: tamaño ya descargado frente al total (si el servidor lo dice).
        let mut last = -1;
        let status = loop {
            if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
                break st;
            }
            if let (Some(t), Ok(m)) = (total, std::fs::metadata(&dest)) {
                let v = scale((m.len() as f64 / t as f64 * 100.0).min(99.0));
                if v != last { last = v; progress(v); }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        };
        if !status.success() {
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut e, &mut err);
            }
            return Err(format!("no se pudo descargar el archivo del enlace{}",
                err.lines().last().map(|l| format!(" ({l})")).unwrap_or_default()));
        }
        progress(scale(100.0));
        dest
    } else {
        let mut child = Command::new("yt-dlp")
            .args(["--no-playlist", "--no-warnings", "--newline", "--no-part", "--socket-timeout", "20",
                   "-f", format_id, "-o"])
            .arg(tmp_dir.join("descarga.%(ext)s"))
            .args(["--progress-template", "download:avance %(progress._percent_str)s",
                   "--print", "after_move:filepath", url])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("no se pudo ejecutar yt-dlp: {e}"))?;
        let mut stderr = child.stderr.take().unwrap();
        let err_thread = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr, &mut s);
            s
        });
        let mut path = None;
        let mut last = -1;
        for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            if let Some(p) = line.strip_prefix("avance ") {
                if let Ok(v) = p.trim().trim_end_matches('%').trim().parse::<f64>() {
                    let v = scale(v);
                    if v != last { last = v; progress(v); }
                }
            } else if !line.trim().is_empty() {
                path = Some(PathBuf::from(line.trim()));
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        let err = err_thread.join().unwrap_or_default();
        match path {
            Some(p) if status.success() && p.is_file() => p,
            _ => {
                let msg = err.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("yt-dlp falló").to_string();
                return Err(msg.trim_start_matches("ERROR:").trim().to_string());
            }
        }
    };

    let stem = safe_name(title);
    let result = if convert_to > 0 {
        crate::media::set_progress_range(60, 40);   // la barra sigue desde donde quedó la descarga
        let c = crate::media::convert_opts(&downloaded, convert_to, 60, reencode)?;
        c.out
    } else {
        downloaded
    };
    let ext = result.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_else(|| "mp4".into());
    let final_stem = if convert_to > 0 { format!("{stem} ({convert_to}p)") } else { stem };
    let dest = free_path(out_dir, &final_stem, &ext);
    // rename si es el mismo disco; si no, copiar y borrar.
    if std::fs::rename(&result, &dest).is_err() {
        std::fs::copy(&result, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    let _ = std::fs::remove_dir_all(tmp_dir);
    progress(100);
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: &str, h: u64, fps: f64, vcodec: &str, proto: &str) -> Value {
        json!({"format_id": id, "height": h, "width": h * 16 / 9, "fps": fps, "vcodec": vcodec,
               "ext": "mp4", "protocol": proto, "filesize": h * 1000})
    }

    #[test]
    fn one_format_per_height_lightest_codec() {
        let raw = vec![
            f("a", 1080, 60.0, "av01.0.09M", "https"),
            f("b", 1080, 60.0, "avc1.64002", "https"),
            f("c", 1080, 60.0, "vp09.00.41", "https"),
            f("d", 1080, 60.0, "avc1.64002", "m3u8_native"),   // por trozos: no
            f("e", 2160, 60.0, "av01.0.13M", "https"),
            f("g", 2160, 60.0, "vp9", "https"),
            f("h", 720, 30.0, "none", "https"),                 // solo audio: no
        ];
        let got = pick_formats(&raw);
        assert_eq!(got.iter().map(|x| (x.height, x.id.as_str(), x.codec.as_str())).collect::<Vec<_>>(),
                   vec![(1080, "b", "H.264"), (2160, "g", "VP9")]);
    }

    #[test]
    fn single_source_gets_smaller_conversions() {
        let src = Format { id: "x".into(), width: 3840, height: 2160, fps: 60.0, ext: "mp4".into(),
                           codec: "H.264".into(), size: Some(100_000_000), approx: false, convert_to: 0, reencode: false };
        let list = add_conversions(vec![src]);
        assert_eq!(list.iter().map(|f| (f.height, f.convert_to)).collect::<Vec<_>>(),
                   vec![(480, 480), (720, 720), (1080, 1080), (1440, 1440), (2160, 0)]);
        assert_eq!(list[2].width, 1920);
    }

    #[test]
    fn av1_source_gets_h264_version_first() {
        let src = Format { id: "x".into(), width: 1280, height: 720, fps: 30.0, ext: "webm".into(),
                           codec: "AV1".into(), size: Some(4_000_000), approx: false, convert_to: 0, reencode: false };
        let list = add_conversions(vec![src]);
        assert_eq!(list.iter().map(|f| (f.height, f.codec.as_str(), f.reencode)).collect::<Vec<_>>(),
                   vec![(480, "H.264", false), (720, "H.264", true), (720, "AV1", false)]);
    }

    #[test]
    fn names_are_safe() {
        assert_eq!(safe_name("Miku in the Room 4K 60FPS"), "Miku in the Room 4K 60FPS");
        assert_eq!(safe_name("../../etc/passwd"), "etc passwd");
        assert_eq!(safe_name("初音ミク / Night"), "初音ミク Night");
        assert_eq!(safe_name("   "), "fondo");
        assert_eq!(ext_of("https://x.com/a/b.MP4?x=1"), "mp4");
        assert_eq!(ext_of("https://x.com/watch?v=1"), "");
    }
}
