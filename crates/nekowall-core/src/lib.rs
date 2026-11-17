//! Orquestación: une biblioteca + backend. Sin dependencias de Qt ni del compositor.

use nekowall_backend::{
    BackendCapabilities, Monitor, Wallpaper, WallpaperBackend, WallpaperError, WallpaperKind,
};
use nekowall_library::{Counts, Filter, Library, LibraryError};
use std::fmt;
use std::path::Path;

const KEY_CURRENT: &str = "current_path";
const KEY_PREVIOUS: &str = "previous_path";

#[derive(Debug)]
pub enum ServiceError {
    Wallpaper(WallpaperError),
    Library(LibraryError),
    NotInLibrary(String),
    NoCandidates,
    NoPrevious,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wallpaper(e) => write!(f, "{e}"),
            Self::Library(e) => write!(f, "{e}"),
            Self::NotInLibrary(s) => write!(f, "no está en la biblioteca: {s}"),
            Self::NoCandidates => write!(f, "no hay fondos aplicables en la biblioteca"),
            Self::NoPrevious => write!(f, "no hay un fondo anterior registrado"),
        }
    }
}
impl std::error::Error for ServiceError {}
impl From<WallpaperError> for ServiceError {
    fn from(e: WallpaperError) -> Self {
        Self::Wallpaper(e)
    }
}
impl From<LibraryError> for ServiceError {
    fn from(e: LibraryError) -> Self {
        Self::Library(e)
    }
}

pub struct Status {
    pub backend: &'static str,
    pub capabilities: BackendCapabilities,
    pub current: Option<Wallpaper>,
    pub counts: Counts,
    /// Cada pantalla con su fondo (vacío si el backend no distingue pantallas).
    pub monitors: Vec<(Monitor, Option<Wallpaper>)>,
}

/// Claves de estado de una pantalla («current_path@HDMI-1»); sin pantalla, las de siempre.
fn key(base: &str, monitor: Option<&str>) -> String {
    match monitor {
        Some(m) => format!("{base}@{m}"),
        None => base.to_string(),
    }
}

pub struct Service {
    pub library: Library,
    backend: Box<dyn WallpaperBackend>,
}

impl Service {
    pub fn new(library: Library, backend: Box<dyn WallpaperBackend>) -> Self {
        Self { library, backend }
    }

    /// Nombre del backend en uso («plasma», «mate»…).
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    pub fn capabilities(&self) -> BackendCapabilities {
        self.backend.capabilities()
    }

    /// Aplica un fondo en todas las pantallas registrando el anterior para poder restaurarlo.
    /// El estado solo se actualiza si el backend tuvo éxito.
    pub fn apply(&self, wallpaper: &Wallpaper) -> Result<(), ServiceError> {
        self.apply_on(wallpaper, None)
    }

    /// Igual, en una pantalla (`Some(nombre)`) o en todas (`None`).
    pub fn apply_on(&self, wallpaper: &Wallpaper, monitor: Option<&str>) -> Result<(), ServiceError> {
        let before = self.backend.current(monitor).ok().flatten();
        self.backend.apply(wallpaper, monitor)?;
        if let Some(prev) = before.filter(|p| p.path != wallpaper.path) {
            self.library.set_state(&key(KEY_PREVIOUS, monitor), &prev.path.to_string_lossy())?;
        }
        self.library.set_state(&key(KEY_CURRENT, monitor), &wallpaper.path.to_string_lossy())?;
        if monitor.is_some() {
            // «Actual» global = último aplicado en cualquier pantalla.
            self.library.set_state(KEY_CURRENT, &wallpaper.path.to_string_lossy())?;
        }
        Ok(())
    }

    /// Acepta un id de la biblioteca o una ruta (que se indexa si hace falta).
    pub fn apply_target(&mut self, target: &str) -> Result<Wallpaper, ServiceError> {
        self.apply_target_on(target, None)
    }

    pub fn apply_target_on(&mut self, target: &str, monitor: Option<&str>) -> Result<Wallpaper, ServiceError> {
        let wp = if let Some(e) = self.library.get(target)? {
            e.wallpaper
        } else if Path::new(target).exists() {
            self.library.import_path(Path::new(target))?;
            let canon = std::fs::canonicalize(target).map_err(LibraryError::from)?;
            self.library
                .get_by_path(&canon)?
                .ok_or_else(|| ServiceError::NotInLibrary(target.into()))?
                .wallpaper
        } else {
            return Err(ServiceError::NotInLibrary(target.into()));
        };
        self.apply_on(&wp, monitor)?;
        Ok(wp)
    }

    pub fn next(&self) -> Result<Wallpaper, ServiceError> {
        self.step(1, None)
    }

    pub fn previous(&self) -> Result<Wallpaper, ServiceError> {
        self.step(-1, None)
    }

    /// Siguiente (+1) o anterior (−1) en una pantalla o en todas.
    pub fn step(&self, delta: isize, monitor: Option<&str>) -> Result<Wallpaper, ServiceError> {
        let kind = (!self.capabilities().video_wallpapers).then_some(WallpaperKind::Static);
        let candidates: Vec<Wallpaper> = self
            .library
            .list(&Filter { kind, ..Default::default() })?
            .into_iter()
            .filter(|e| !e.missing)
            .map(|e| e.wallpaper)
            .collect();
        if candidates.is_empty() {
            return Err(ServiceError::NoCandidates);
        }

        let current = match self.library.state(&key(KEY_CURRENT, monitor))? {
            Some(p) => Some(p),
            None => self.backend.current(monitor).ok().flatten().map(|w| w.path.to_string_lossy().into_owned()),
        };
        let n = candidates.len() as isize;
        let idx = current
            .and_then(|c| candidates.iter().position(|w| w.path.to_string_lossy() == c.as_str()))
            .map(|i| (i as isize + delta).rem_euclid(n))
            .unwrap_or(if delta > 0 { 0 } else { n - 1 });

        let wp = candidates[idx as usize].clone();
        self.apply_on(&wp, monitor)?;
        Ok(wp)
    }

    /// Vuelve al fondo anterior (y el actual pasa a ser "anterior": alterna).
    pub fn restore_previous(&self) -> Result<Wallpaper, ServiceError> {
        self.restore_previous_on(None)
    }

    pub fn restore_previous_on(&self, monitor: Option<&str>) -> Result<Wallpaper, ServiceError> {
        let prev = self.library.state(&key(KEY_PREVIOUS, monitor))?.ok_or(ServiceError::NoPrevious)?;
        let wp = Wallpaper::from_path(Path::new(&prev)).ok_or(ServiceError::NoPrevious)?;
        self.apply_on(&wp, monitor)?;
        Ok(wp)
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        self.backend.monitors()
    }

    pub fn status(&self) -> Result<Status, ServiceError> {
        Ok(Status {
            backend: self.backend.name(),
            capabilities: self.backend.capabilities(),
            current: self.backend.current(None)?,
            counts: self.library.counts()?,
            monitors: self
                .backend
                .monitors()
                .into_iter()
                .map(|m| {
                    let cur = self.backend.current(Some(&m.name)).ok().flatten();
                    (m, cur)
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct State {
        current: Option<Wallpaper>,
        fail: bool,
    }
    struct Fake(Arc<Mutex<State>>);

    impl WallpaperBackend for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities { static_wallpapers: true, ..Default::default() }
        }
        fn apply(&self, w: &Wallpaper, _: Option<&str>) -> Result<(), WallpaperError> {
            let mut s = self.0.lock().unwrap();
            if s.fail {
                return Err(WallpaperError::BackendUnavailable("simulado".into()));
            }
            s.current = Some(w.clone());
            Ok(())
        }
        fn current(&self, _: Option<&str>) -> Result<Option<Wallpaper>, WallpaperError> {
            Ok(self.0.lock().unwrap().current.clone())
        }
    }

    fn setup(names: &[&str]) -> (Service, Arc<Mutex<State>>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nw-core-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for n in names {
            fs::write(dir.join(n), b"x").unwrap();
        }
        let st = Arc::new(Mutex::new(State::default()));
        let mut lib = Library::open_in_memory().unwrap();
        lib.import_path(&dir).unwrap();
        (Service::new(lib, Box::new(Fake(st.clone()))), st, dir)
    }

    fn cur(st: &Arc<Mutex<State>>) -> String {
        st.lock().unwrap().current.as_ref().unwrap().path.file_name().unwrap().to_string_lossy().into()
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let (svc, st, _) = setup(&["a.png", "b.png", "c.png"]);
        svc.next().unwrap();
        assert_eq!(cur(&st), "a.png");
        svc.next().unwrap();
        svc.next().unwrap();
        assert_eq!(cur(&st), "c.png");
        svc.next().unwrap();
        assert_eq!(cur(&st), "a.png");
        svc.previous().unwrap();
        assert_eq!(cur(&st), "c.png");
    }

    #[test]
    fn skips_videos_when_backend_lacks_support() {
        let (svc, st, _) = setup(&["a.mp4", "b.png"]);
        svc.next().unwrap();
        assert_eq!(cur(&st), "b.png");
        svc.next().unwrap();
        assert_eq!(cur(&st), "b.png");
    }

    #[test]
    fn restore_previous_toggles() {
        let (mut svc, st, dir) = setup(&["a.png", "b.png"]);
        svc.apply_target(dir.join("a.png").to_str().unwrap()).unwrap();
        svc.apply_target(dir.join("b.png").to_str().unwrap()).unwrap();
        assert_eq!(cur(&st), "b.png");
        svc.restore_previous().unwrap();
        assert_eq!(cur(&st), "a.png");
        svc.restore_previous().unwrap();
        assert_eq!(cur(&st), "b.png");
    }

    #[test]
    fn failed_apply_does_not_change_state() {
        let (mut svc, st, dir) = setup(&["a.png"]);
        st.lock().unwrap().fail = true;
        assert!(svc.apply_target(dir.join("a.png").to_str().unwrap()).is_err());
        assert_eq!(svc.library.state(KEY_CURRENT).unwrap(), None);
        assert!(matches!(svc.restore_previous(), Err(ServiceError::NoPrevious)));
    }

    #[test]
    fn unknown_target_and_empty_library() {
        let (mut svc, _, _) = setup(&[]);
        assert!(matches!(svc.apply_target("/no/existe.png"), Err(ServiceError::NotInLibrary(_))));
        assert!(matches!(svc.next(), Err(ServiceError::NoCandidates)));
    }
}
