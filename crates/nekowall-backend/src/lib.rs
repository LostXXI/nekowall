//! Modelos y contrato común de backends. Independiente de Qt y del compositor.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WallpaperKind {
    Static,
    Video,
}

const STATIC_EXT: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "avif", "tiff"];
const VIDEO_EXT: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v"];

impl WallpaperKind {
    /// Detecta el tipo por extensión. `None` si no está soportado.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        if STATIC_EXT.contains(&ext.as_str()) {
            Some(Self::Static)
        } else if VIDEO_EXT.contains(&ext.as_str()) {
            Some(Self::Video)
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Video => "video",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "static" => Some(Self::Static),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wallpaper {
    pub id: String,
    pub path: PathBuf,
    pub kind: WallpaperKind,
}

impl Wallpaper {
    /// Crea un fondo a partir de una ruta; el id es estable (FNV-1a de la ruta).
    pub fn from_path(path: &Path) -> Option<Self> {
        let kind = WallpaperKind::from_path(path)?;
        Some(Self {
            id: id_for_path(path),
            path: path.to_path_buf(),
            kind,
        })
    }
}

/// FNV-1a de 64 bits: estable entre versiones de Rust (a diferencia de DefaultHasher).
pub fn id_for_path(path: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.as_os_str().to_string_lossy().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub static_wallpapers: bool,
    pub video_wallpapers: bool,
    pub per_monitor: bool,
    pub pause_video: bool,
}

#[derive(Debug)]
pub enum WallpaperError {
    NotFound(PathBuf),
    Unsupported(String),
    BackendUnavailable(String),
    CommandFailed { command: String, detail: String },
    Io(io::Error),
}

impl fmt::Display for WallpaperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "el archivo no existe: {}", p.display()),
            Self::Unsupported(m) => write!(f, "no soportado por este backend: {m}"),
            Self::BackendUnavailable(m) => write!(f, "backend no disponible: {m}"),
            Self::CommandFailed { command, detail } => {
                write!(f, "falló `{command}`: {detail}")
            }
            Self::Io(e) => write!(f, "error de E/S: {e}"),
        }
    }
}

impl std::error::Error for WallpaperError {}

impl From<io::Error> for WallpaperError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub trait WallpaperBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> BackendCapabilities;
    fn apply(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), WallpaperError>;
    fn current(&self, monitor: Option<&str>) -> Result<Option<Wallpaper>, WallpaperError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kinds() {
        assert_eq!(WallpaperKind::from_path(Path::new("a.PNG")), Some(WallpaperKind::Static));
        assert_eq!(WallpaperKind::from_path(Path::new("a.mp4")), Some(WallpaperKind::Video));
        assert_eq!(WallpaperKind::from_path(Path::new("a.txt")), None);
        assert_eq!(WallpaperKind::from_path(Path::new("sin_ext")), None);
    }

    #[test]
    fn id_is_stable() {
        assert_eq!(id_for_path(Path::new("/a/b.png")), id_for_path(Path::new("/a/b.png")));
        assert_ne!(id_for_path(Path::new("/a/b.png")), id_for_path(Path::new("/a/c.png")));
    }
}
