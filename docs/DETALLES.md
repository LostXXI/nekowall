# NekoWall · Detalles

Todo lo que no hace falta para instalar NekoWall con `./instalar.sh`: funciones, instalación manual,
uso desde la terminal y notas de desarrollo. Para instalar, mira el [README](../README.md).

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

## Opciones de instalar.sh

`--simular` (muestra lo que haría sin tocar nada) · `--kde` / `--mate` (forzar escritorio) ·
`--solo-dependencias` · `--si` (sin preguntas). Usa `sudo` o `doas` solo para instalar paquetes; no lo
ejecutes como root.

## Instalación manual

Solo si no quieres usar `./instalar.sh` o no estás en Void (cambia `xbps-install` por el instalador de tu
distribución; los nombres de paquetes pueden variar).

### Instalar en KDE Plasma

#### 1. Dependencias

```bash
sudo xbps-install -S git rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel \
    kf6-kwindowsystem-devel qt6-wayland layer-shell-qt kf6-qqc2-desktop-style \
    ffmpeg6 glib qt6-multimedia
```

Para vídeo en KDE, mira [el plugin](../README.md#paso-4--solo-kde-cierra-sesión-y-vuelve-a-entrar) en el README.

#### 2. Compilar e instalar

Ver [Compilar](#compilar-los-dos-escritorios) e [Instalar el programa](#instalar-el-programa-los-dos-escritorios).

#### 3. Atajo Meta+Alt+W

El acceso del menú ya lleva el atajo: **cierra sesión y vuelve a entrar** una vez para que KDE lo active.
Para cambiarlo: *Ajustes del sistema → Teclado → Atajos → NekoWall*.

### Instalar en MATE

#### 1. Dependencias

```bash
sudo xbps-install -S git rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel \
    ffmpeg6 glib mpv xwinwrap xprop xdotool
```

(En neko-void usa `doas` en lugar de `sudo` si así lo elegiste al instalar.)

#### 2. Compilar e instalar

Ver [Compilar](#compilar-los-dos-escritorios) e [Instalar el programa](#instalar-el-programa-los-dos-escritorios).

#### 3. Atajo Meta+Alt+W

Abre NekoWall → **Ajustes → Registrar atajo**: funciona al momento, sin cerrar sesión. Registra dos atajos:
**Meta+Alt+W** (selector) y **Meta+Alt+D** (mostrar escritorio, ver abajo). También puedes crearlos a mano en
*Centro de control → Atajos de teclado* con los comandos `nekowall-gui --hud` y `nekowall peek`.

#### Cosas a saber en MATE

- Con un **vídeo** de fondo, el vídeo **tapa los iconos del escritorio**, pero siguen ahí: el **clic derecho**
  del escritorio y el doble clic sobre los iconos funcionan (los clics atraviesan el vídeo hasta Caja).
  Caja no puede dibujar sus iconos con fondo transparente, así que no se pueden ver a la vez.
- **Meta+Alt+D** (`nekowall peek`) pausa y oculta el vídeo para ver los iconos encima de un fotograma del mismo
  vídeo; vuelve a pulsarlo para seguir.
- El vídeo se pausa solo según tus ventanas y la batería (*Ajustes → Vídeo: pausa y ahorro*) y vuelve a
  arrancar al iniciar sesión.
- Sin compositor con desenfoque, el selector usa un fondo oscuro en lugar de cristal.

### Compilar (los dos escritorios)

```bash
git clone https://github.com/LostXXI/nekowall.git
cd nekowall

# 1) Núcleo y CLI (Rust) → target/release/nekowall
cargo build --release

# 2) Interfaz gráfica (Qt/QML) → app/build/nekowall-gui
cmake -S app -B app/build -DCMAKE_BUILD_TYPE=Release
cmake --build app/build -j"$(nproc)"
```

Comprobar que todo funciona (opcional): `cargo test`. Probarlo sin instalar:

```bash
./target/release/nekowall import ~/Pictures/Wallpapers   # indexa tu carpeta de fondos
./app/build/nekowall-gui                                  # ventana principal
./app/build/nekowall-gui --hud                            # selector rápido
```

### Instalar el programa (los dos escritorios)

Para tu usuario, sin `sudo`. Copia los dos programas **a la misma carpeta** (la interfaz busca `nekowall` a su
lado) y crea el acceso del menú:

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
Categories=Settings;DesktopSettings;Qt;
Actions=hud;

[Desktop Action hud]
Name=Selector rápido
Exec=$HOME/.local/bin/nekowall-gui --hud
X-KDE-Shortcuts=Meta+Alt+W
EOF
```

En KDE ejecuta además `kbuildsycoca6` para que el menú lo vea al momento.

Para desinstalar: borra `~/.local/bin/nekowall`, `~/.local/bin/nekowall-gui` y
`~/.local/share/applications/nekowall.desktop` (en MATE también `~/.config/autostart/nekowall-video.desktop`
si había un vídeo puesto). Tus datos están en `~/.local/share/nekowall/`, `~/.cache/nekowall/` y
`~/.config/nekowall/`.

## Atajos de la ventana

Ctrl+F buscar · Ctrl+O añadir archivos · Ctrl+Mayús+O añadir carpeta · Ctrl+H selector · F5 recargar ·
Ctrl+Z deshacer «quitar» · en la rejilla: Enter aplicar, F favorito, Supr quitar. En **Ajustes** puedes elegir
cuándo se pausa el vídeo, qué hacer con vídeos grandes y la ilustración del selector.

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

- Escritorios soportados: **KDE Plasma 6 (Wayland)** y **MATE (X11)**. XFCE, LXQt, i3, Openbox, labwc, niri… aún no.
- En MATE, con un vídeo de fondo los iconos del escritorio quedan tapados (funcionan, pero no se ven):
  Meta+Alt+D los muestra.
- En X11 sin compositor con desenfoque el selector usa un fondo oscuro en lugar de cristal.
- Un solo monitor: el mismo fondo se aplica a todos los escritorios.
- En KDE, los vídeos necesitan el plugin Smart Video Wallpaper Reborn.

## Licencia

Pendiente de definir (el `Cargo.toml` indica provisionalmente GPL-3.0-or-later).

---

## Notas de desarrollo

### Arquitectura
- `crates/nekowall-backend`: modelos (`Wallpaper`, `WallpaperKind`) y el contrato `WallpaperBackend`.
- `crates/nekowall-library`: biblioteca SQLite (`rusqlite` con SQLite embebido).
- `crates/nekowall-core`: orquestación (`Service`: aplicar, siguiente/anterior, restaurar).
- `crates/nekowall-mate`: backend de MATE. Imágenes: gsettings `org.mate.background picture-filename`. Vídeo
  (`video.rs`): `xwinwrap` (ventana tipo escritorio, sin entrada `-ni`) + `mpv` en bucle sin audio, ENCIMA de la
  ventana de escritorio de Caja. Marco sube Caja al cerrarse cualquier ventana: el vigilante escucha los cambios
  de apilado (`xprop -spy _NET_CLIENT_LIST_STACKING`) y sube el vídeo con `xdotool windowraise` (~50 ms, sin tocar
  Caja; recrear el escritorio de Caja queda como último recurso). Vídeo→vídeo se cambia por IPC (`loadfile`)
  en el mismo mpv; al pasar a imagen se cambia primero el fondo de Caja y luego se quita el vídeo. `stop` mata
  el vigilante y todos sus descendientes por /proc (no depende del grupo de procesos). Los clics atraviesan
  el vídeo hasta Caja. Caja pinta el fondo estático bajo los iconos (pseudo-transparencia), por eso no puede
  mostrarlos sobre el vídeo: `peek` desmapea y pausa el vídeo; bajo él Caja muestra un fotograma del vídeo. Un proceso propio `nekowall _mate-video` pausa
  por IPC de mpv según las ventanas (xprop: maximizada/completa, activa, cualquiera, nunca) y la batería; se
  relanza al iniciar sesión con `~/.config/autostart/nekowall-video.desktop`. Estado y ajustes en
  `~/.config/nekowall/mate.conf`.
- `crates/nekowall-plasma`: backend de KDE Plasma (`plasma-apply-wallpaperimage` para imágenes; script por
  D-Bus a `org.kde.plasmashell /PlasmaShell evaluateScript` para el plugin de vídeo).
- `crates/nekowall-cli`: la CLI `nekowall` (y `media.rs`: ffprobe/ffmpeg).
- `app/`: interfaz Qt 6/QML en C++. El selector es `Hud.qml`; en Wayland, si existe `org.kde.layershell`, se carga
  `HudWayland.qml` (el mismo HUD + layer-shell). En X11 es una ventana a pantalla completa siempre encima. `Bridge` llama a la CLI con `--json` mediante `QProcess` asíncrono
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

### Pruebas en MATE
Máquina virtual QEMU/KVM con la ISO de neko-void MATE (X11, Caja + Marco + mate-panel): aplicar imágenes
(CLI, ventana y selector con teclado), «Registrar atajo» y Meta+Alt+W, vídeo de fondo (se mueve, pausa con
ventana maximizada y en modo «ventana activa», vuelve a imagen restaurando iconos, sobrevive a un reinicio)
comprobados con capturas de la VM. El rendimiento del vídeo no se midió: la VM decodifica por software.

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
