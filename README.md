# NekoWall

Gestor de fondos de pantalla **estáticos y animados** para **KDE Plasma 6 (Wayland)**, pensado para
**Void Linux**. Incluye una biblioteca con miniaturas y un **selector rápido** a pantalla completa
(estilo «Dark Aero») que se abre con un atajo de teclado desde cualquier lugar.

> Estado: versión 0.1 (temprana). Funciona en el equipo del autor (Void Linux, Plasma 6.7.5, Wayland).
> Por ahora **solo KDE Plasma en Wayland**: no hay soporte para X11 ni para otros escritorios.

## Funciones

- **Selector rápido** (`Meta+Alt+W`): vídeos en un carrusel horizontal (←/→) e imágenes en uno vertical (↑/↓);
  Enter aplica, F marca favorito, Esc cierra. Fondo de cristal desenfocado e ilustración opcional.
- **Biblioteca**: búsqueda, filtros (animados / estáticos / favoritos), orden, miniaturas en caché,
  importar archivos o carpetas (también arrastrando), quitar con «deshacer». Los archivos **no se copian**
  ni se modifican: se indexan donde están.
- **Vídeos grandes**: al importar un vídeo mayor que tu pantalla (p. ej. 4K) ofrece crear una copia a tu
  resolución (30 o 60 fps) junto al original; también desde clic derecho → «Optimizar para mi pantalla…».
- **Ahorro**: pausa del vídeo con ventanas maximizadas / activas / visibles y con batería baja.
- **CLI** (`nekowall`) para usar todo desde la terminal o desde scripts.
- Cerrado no ocupa memoria: no deja procesos en segundo plano.

## Requisitos

| Qué | Paquete(s) de Void |
| --- | --- |
| Compilar el núcleo (Rust) | `rust` `cargo` `gcc` |
| Compilar la interfaz (Qt 6 ≥ 6.5) | `cmake` `make` `qt6-base-devel` `qt6-declarative-devel` |
| Desenfoque de KWin (opcional, recomendado) | `kf6-kwindowsystem-devel` |
| Ejecutar | `qt6-wayland` `layer-shell-qt` `kf6-qqc2-desktop-style` `ffmpeg6` `glib` `plasma-workspace` |
| Fondos de **vídeo** | `qt6-multimedia` + el plugin **Smart Video Wallpaper Reborn** (ver abajo) |

Instalar todo de una vez:

```bash
sudo xbps-install -S rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel \
    kf6-kwindowsystem-devel qt6-wayland layer-shell-qt kf6-qqc2-desktop-style \
    ffmpeg6 glib qt6-multimedia
```

(`plasma-workspace` ya viene con KDE Plasma.)

### Plugin de vídeo (solo para fondos animados)

NekoWall aplica los vídeos con el plugin de fondo **Smart Video Wallpaper Reborn**
(`luisbocanegra.smart.video.wallpaper.reborn`), que no está en los repositorios de Void. La forma más
sencilla de instalarlo es desde el propio Plasma:

1. Clic derecho en el escritorio → **Configurar el escritorio y el fondo de pantalla…**
2. **Obtener nuevos complementos…** → busca **Smart Video Wallpaper Reborn** → **Instalar**.

Sin el plugin, NekoWall funciona igual con imágenes y avisa de que el vídeo no está disponible.

## Compilar

```bash
git clone https://github.com/LostXXI/nekowall.git
cd nekowall

# 1) Núcleo y CLI (Rust) → target/release/nekowall
cargo build --release

# 2) Interfaz gráfica (Qt/QML) → app/build/nekowall-gui
cmake -S app -B app/build -DCMAKE_BUILD_TYPE=Release
cmake --build app/build -j"$(nproc)"
```

Comprobar que todo funciona (opcional):

```bash
cargo test
```

Probarlo sin instalar:

```bash
./target/release/nekowall import ~/Pictures/Wallpapers   # indexa tu carpeta de fondos
./app/build/nekowall-gui                                  # ventana principal
./app/build/nekowall-gui --hud                            # selector rápido
```

## Instalar (para tu usuario, sin `sudo`)

Copia los dos programas **a la misma carpeta** (la interfaz busca `nekowall` a su lado) y crea el acceso
del menú con el atajo del selector:

```bash
mkdir -p ~/.local/bin ~/.local/share/applications
install -m755 target/release/nekowall app/build/nekowall-gui ~/.local/bin/

cat > ~/.local/share/applications/nekowall.desktop <<EOF
[Desktop Entry]
Type=Application
Name=NekoWall
GenericName=Gestor de fondos de pantalla
Comment=Cambia fondos de pantalla estáticos y animados
Exec=$HOME/.local/bin/nekowall-gui
Icon=preferences-desktop-wallpaper
Terminal=false
Categories=Utility;Settings;DesktopSettings;Qt;KDE;
Actions=hud;

[Desktop Action hud]
Name=Selector rápido
Exec=$HOME/.local/bin/nekowall-gui --hud
X-KDE-Shortcuts=Meta+Alt+W
EOF

kbuildsycoca6
```

**Cierra sesión y vuelve a entrar**: KDE registra el atajo **Meta+Alt+W** (tecla Windows + Alt + W) al
iniciar la sesión. Si quieres otra combinación: *Ajustes del sistema → Teclado → Atajos → NekoWall*.

Para desinstalar: borra `~/.local/bin/nekowall`, `~/.local/bin/nekowall-gui` y
`~/.local/share/applications/nekowall.desktop`. Tus datos están en `~/.local/share/nekowall/`,
`~/.cache/nekowall/` y `~/.config/nekowall/` (bórralos si no los quieres).

## Primer uso

1. Abre **NekoWall** desde el menú → **Añadir carpeta…** (o arrastra archivos a la ventana).
2. Pulsa **Aplicar** en cualquier fondo, o abre el selector con **Meta+Alt+W**.
3. En **Ajustes** puedes elegir cuándo se pausa el vídeo, qué hacer con vídeos grandes y la
   ilustración del selector.

Atajos de la ventana: Ctrl+F buscar · Ctrl+O añadir archivos · Ctrl+Mayús+O añadir carpeta ·
Ctrl+H selector · F5 recargar · Ctrl+Z deshacer «quitar» · en la rejilla: Enter aplicar, F favorito,
Supr quitar.

## Uso desde la terminal

```text
nekowall import <ruta>...            Indexa archivos o carpetas (sin copiar)
nekowall list [--video|--static] [--favorites] [--search TEXTO] [--json]
nekowall apply <id|ruta>             Aplica un fondo
nekowall next | previous | restore   Siguiente / anterior / volver al fondo anterior
nekowall favorite <id> [--off]       Marca o desmarca favorito
nekowall remove <id>                 Quita de la biblioteca (no borra el archivo)
nekowall status [--json]             Escritorio, capacidades y fondo actual
nekowall thumbs [--force]            Genera las miniaturas que falten
nekowall optimize <id|ruta> [--height 1080] [--fps 30] [--replace-in-library]
nekowall video-settings [--pause-mode 0-3] [--battery on|off] [--battery-level N]
nekowall relocate <carpeta-vieja> <carpeta-nueva>   Tras mover una carpeta de fondos
nekowall help
```

Variables: `NEKOWALL_DB` (ruta de la biblioteca), `NEKOWALL_CLI` (ruta de `nekowall` para la interfaz),
`NEKOWALL_REDUCED_MOTION=1` (sin animaciones; también se respeta «Velocidad de animación = 0» de KDE).

## Limitaciones conocidas

- Solo **KDE Plasma 6 en Wayland**. X11, GNOME, Sway/Hyprland, etc. no están soportados todavía.
- Un solo monitor: el mismo fondo se aplica a todos los escritorios.
- Los vídeos necesitan el plugin Smart Video Wallpaper Reborn.

## Licencia

Pendiente de definir (el `Cargo.toml` indica provisionalmente GPL-3.0-or-later).

---

## Notas de desarrollo

### Arquitectura
- `crates/nekowall-backend`: modelos (`Wallpaper`, `WallpaperKind`) y el contrato `WallpaperBackend`.
- `crates/nekowall-library`: biblioteca SQLite (`rusqlite` con SQLite embebido).
- `crates/nekowall-core`: orquestación (`Service`: aplicar, siguiente/anterior, restaurar).
- `crates/nekowall-plasma`: backend de KDE Plasma (`plasma-apply-wallpaperimage` para imágenes; script por
  D-Bus a `org.kde.plasmashell /PlasmaShell evaluateScript` para el plugin de vídeo).
- `crates/nekowall-cli`: la CLI `nekowall` (y `media.rs`: ffprobe/ffmpeg).
- `app/`: interfaz Qt 6/QML en C++. `Bridge` llama a la CLI con `--json` mediante `QProcess` asíncrono
  (en lugar de CXX-Qt): compila en segundos y la interfaz nunca se bloquea.

### Decisiones
- La CLI se analiza a mano (sin `clap`) para minimizar dependencias.
- Plasma conserva la configuración de plugins inactivos (p. ej. `Image=`), por eso `current()` se guía por
  `wallpaperplugin=` y no por la última clave vista.
- Si `plasmashell` está colgado, `/PlasmaShell` no existe y NekoWall informa «plasmashell no responde por D-Bus».
- Conversión de vídeo con `libx264` a prioridad baja (`nice -n 10`): en Intel UHD 620 + iHD + ffmpeg 6.1,
  `scale_vaapi` falla al redimensionar y el camino híbrido VA-API fue más lento y con archivos más grandes.
- Un atajo de servicio registrado en caliente por D-Bus en kglobalaccel no lanza el programa hasta volver a
  iniciar sesión; por eso el `.desktop` lleva `X-KDE-Shortcuts`.

### Estilo del selector (Dark Aero)
Ventana transparente con desenfoque de KWin (`KWindowEffects::enableBlurBehind`, opcional vía
KF6WindowSystem), paneles de cristal con brillo, acento turquesa (#39C5BB), halo (`RectangularShadow`) en la
tarjeta activa. La ilustración opcional la elige el usuario en Ajustes; se copia a
`~/.local/share/nekowall/mascota.*` y se «hornea» una vez (recortada y fundida) en
`~/.cache/nekowall/hud-art-*.png`, de modo que el selector dibuja una sola imagen sin efectos.

### Rendimiento (Intel i7-8550U, UHD 620, 1920×1080)
- NekoWall cerrado: 0 MiB. Selector: ~62 MiB (PSS) y ~190 ms hasta el primer fotograma (enchufado).
  Ventana principal: ~73 MiB, sin crecimiento apreciable tras horas abierta. Cada llamada a la CLI: 7-8 ms.
- Optimización del selector: 87,5 → 61,5 MiB y 211 → 198 ms (medianas de 8 aperturas alternadas) al hornear
  la ilustración, sustituir el `MultiEffect` del fondo por una miniatura de 48 px ampliada y decodificar las
  tarjetas a 420 px.
- Fondo de vídeo (plasmashell): imagen estática 0,6 % CPU / 361 MiB · vídeo 4K 25,8 % / 497 MiB ·
  el mismo vídeo a 1080p30 22,0 % / 397 MiB · en pausa 3,1 %. Decodificar por VA-API cuesta ~3 %; el resto
  es la tubería de Qt Multimedia del plugin.
- Probado sin ganancia útil: estilo Fusion (−2 MiB), `MALLOC_ARENA_MAX=2`, `QSG_RENDER_LOOP=basic`.
- En batería (gobernador `powersave`) los tiempos de apertura varían mucho (280-800 ms).
