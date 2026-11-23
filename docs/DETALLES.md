# NekoWall · Detalles

Todo lo que no hace falta para instalar NekoWall con `./install.sh`: funciones, instalación manual,
uso desde la terminal y notas de desarrollo. Para instalar, mira el [README](../README.md).

## Funciones

- **Selector rápido** (`Meta+Alt+W`): vídeos en un carrusel horizontal (←/→) e imágenes en uno vertical (↑/↓);
  Enter aplica, F marca favorito, Esc cierra. Fondo de cristal desenfocado e ilustración opcional.
- **Biblioteca**: búsqueda, filtros (animados / estáticos / favoritos), orden, miniaturas en caché,
  importar archivos o carpetas (también arrastrando), quitar con «deshacer». Los archivos **no se copian**
  ni se modifican: se indexan donde están.
- **Varias pantallas**: cada pantalla tiene su fondo. Por defecto se aplica en la pantalla de la ventana
  (el selector se abre en la pantalla del ratón); la tecla **P** del selector o la lista «Esta pantalla /
  Pantalla N / Todas» de la biblioteca eligen otra. En MATE se compone una imagen para todo el escritorio
  (Caja solo admite una) y hay un reproductor de vídeo por pantalla, cada uno con su propia pausa. En KDE
  se cambia el escritorio de Plasma de esa pantalla. *Probado con dos pantallas en KDE (equipo real) y
  en MATE (máquina virtual).*
- **Carpetas vigiladas**: las carpetas añadidas se revisan al abrir NekoWall y, mientras está abierto, al
  cambiar: lo que copies aparece solo y lo que borres desaparece. Lo que quitas a mano no vuelve.
- **Carpeta propia**: `nekowallpapers` dentro de la carpeta de Imágenes (`XDG_PICTURES_DIR`, normalmente
  `~/Pictures`). Se crea la primera vez y se vigila siempre; el botón **Mis fondos** la abre. Si la dejas de
  vigilar (`nekowall folders --forget …`), no se vuelve a crear.
- **Workshop**: busca en [Wallhaven](https://wallhaven.cc) (API pública, sin cuenta ni clave) por texto o
  etiqueta, con orden (populares, recientes, top del mes, aleatorio), categorías (anime, general, personas) y
  «Para mi pantalla» (tu resolución o más). El panel de cada fondo muestra la imagen grande, sus etiquetas
  (clic = buscar esa etiqueta), autor y enlace a Wallhaven. Las descargas se guardan como
  `nekowallpapers/wallhaven-<id>.jpg|png` (mientras bajan, como archivo oculto `.part`) y aparecen solas en la
  biblioteca; «Descargar y aplicar» lo pone de fondo al terminar. **Siempre solo contenido apto (SFW)**, sin
  opción para cambiarlo. No se conecta a internet hasta que abres la página. Para que vaya rápido: las búsquedas se
  guardan en `~/.cache/nekowall/busquedas` (repetir una búsqueda es instantáneo; «Recientes» caduca a los 15 min
  y «Aleatorio» no se guarda), la página siguiente se pide por adelantado, las miniaturas se guardan en
  `~/.cache/nekowall/workshop` (máx. 200 MB) y si Wallhaven falla (502/503) se reintenta cada medio segundo.
  **Cada día a las 23:59** se borran las búsquedas y las miniaturas guardadas (si NekoWall estaba cerrado, al
  abrirlo). Las descargas no se tocan.
- **Descargar con enlace** (botón azul en la cabecera de la Biblioteca): descarga un fondo desde un enlace (YouTube, Reddit, X, Vimeo, una página
  con un vídeo incrustado o un enlace directo a un vídeo o imagen) con **yt-dlp**. La ventana tiene la vista
  previa fija a la izquierda (miniatura y, con ▶, el vídeo en baja calidad sin descargarlo; necesita
  `qt6-multimedia`) y a la derecha los formatos de menor a mayor, con el de tu pantalla ya marcado. Por cada
  altura se elige el formato más liviano de reproducir (H.264, luego VP9; AV1 solo si no hay otro). Se
  descarga sin audio a `nekowallpapers`. Si el sitio da una sola versión, NekoWall ofrece versiones más
  pequeñas convirtiendo el vídeo tras descargarlo. MoeWalls no se puede analizar (su botón de descarga usa un
  código propio): descarga en su web y arrastra el archivo. También: `nekowall-gui --link=<enlace>` abre esa ventana con el enlace ya analizado.
- **Fondos oficiales de neko-void**: en neko-void, `/usr/share/backgrounds` (paquete `neko-backgrounds`) se
  vincula sola la primera vez. Si la quitas, no vuelve.
- **Vídeos grandes**: al importar un vídeo mayor que tu pantalla (p. ej. 4K) ofrece crear una copia a tu
  resolución (30 o 60 fps) junto al original; también desde clic derecho → «Optimizar para mi pantalla…».
- **Imágenes grandes**: las imágenes más grandes que tu pantalla muestran el botón **Ajustar** (también en
  clic derecho → «Ajustar a mi pantalla»). Crea una copia recortada al tamaño exacto de la pantalla más grande
  (como la muestra el escritorio: escalada hasta cubrirla y centrada), en JPG de máxima calidad, p. ej.
  `fondo (1920x1080).jpg`, junto al original (o en `nekowallpapers` si la carpeta es del sistema). La biblioteca
  usa la copia, conserva el favorito y, si era el fondo puesto, lo cambia por la copia. El original no se borra.
  Ejemplo medido: 1915×2456 PNG de 10,1 MB → 1,7 MB (SSIM 0,979) en 0,7 s.
- **Ahorro**: pausa del vídeo con ventanas maximizadas / activas / visibles y con batería baja.
- **CLI** (`nekowall`) para usar todo desde la terminal o desde scripts.
- Cerrado no ocupa memoria: no deja procesos en segundo plano.

## Opciones de install.sh

`--dry-run` (muestra lo que haría sin tocar nada) · `--kde` / `--mate` / `--niri` (forzar escritorio) ·
`--deps-only` (solo dependencias) · `--yes` o `-y` (sin preguntas) · `--help`. Los nombres en español de
versiones anteriores (`--simular`, `--solo-dependencias`, `--si`) siguen funcionando. Usa `sudo` o `doas` solo para instalar paquetes; no lo
ejecutes como root.

## Instalación manual

Solo si no quieres usar `./install.sh` o no estás en Void (cambia `xbps-install` por el instalador de tu
distribución; los nombres de paquetes pueden variar).

### Instalar en KDE Plasma

#### 1. Dependencias

```bash
sudo xbps-install -S git rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel \
    kf6-kwindowsystem-devel qt6-wayland layer-shell-qt kf6-qqc2-desktop-style \
    ffmpeg6 glib qt6-multimedia libkf6screen yt-dlp
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
    ffmpeg6 glib mpv xwinwrap xprop xdotool xrandr wmctrl qt6-multimedia yt-dlp
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

### Instalar en niri

#### 1. Dependencias

```bash
sudo xbps-install -S git rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel \
    ffmpeg6 glib yt-dlp mpvpaper awww layer-shell-qt qt6-wayland qt6-multimedia
```

#### 2. Compilar e instalar

Igual que en los otros escritorios (abajo). Después registra el atajo y el arranque:

```bash
~/.local/bin/nekowall niri-shortcut --gui ~/.local/bin/nekowall-gui
```

Eso crea `~/.config/niri/nekowall.kdl` (atajo **Meta+Alt+W**, `spawn-at-startup` que restaura el vídeo de
fondo al iniciar sesión y una regla para que la ventana principal se abra maximizada) y añade `include "nekowall.kdl"` al final de `config.kdl`. niri lo aplica al momento.
Para quitarlo: borra ese archivo y la línea `include`.

#### Cómo funciona en niri

- **Pantallas**: `niri msg -j outputs`.
- **Imágenes**: si **Noctalia** está en marcha (la trae neko-void niri), `noctalia msg wallpaper-set
  [pantalla] <ruta>`: Noctalia hace la transición y genera los colores del sistema a partir del fondo. Sin
  Noctalia, **awww** (`awww img`, NekoWall arranca `awww-daemon` si hace falta).
- **Vídeos**: un **mpvpaper** por pantalla en la capa `bottom` (encima del fondo y debajo de las ventanas),
  sin audio y en bucle. Debajo se pone un fotograma del vídeo, así los colores de Noctalia siguen al vídeo.
- **Pausa**: el vigilante `nekowall _niri-video` mira cada segundo las ventanas del escritorio activo de cada
  pantalla y pausa ese vídeo según Ajustes (ventana maximizada / activa / cualquiera / nunca, y batería
  baja). Medido en la VM: 40 % de CPU con el vídeo a la vista y 0 % con una ventana maximizada encima.
- **Estado**: `~/.config/nekowall/niri.conf`. Al iniciar sesión, `nekowall _niri-restore` vuelve a poner los
  vídeos (las imágenes las recuerdan Noctalia/awww).
- **Selector rápido**: la versión Wayland (layer-shell), igual que en KDE.
- *Probado en una máquina virtual con neko-void niri (niri 26.04, Noctalia 5.2.1), también con dos pantallas:
  imagen en una y vídeo en la otra, dos vídeos distintos, pausa por pantalla y el selector rápido abriéndose
  en la pantalla activa. Workshop, Descargar con enlace (con vista previa) y la descarga también funcionan.*

### Instalar con Wayfire, sway, labwc, Hyprland o river (wlroots) — también en Arch / Artix

`./install.sh` detecta estos compositores (o usa `--wlroots`). En **Arch/Artix** instala con `pacman`:

```bash
sudo pacman -S --needed git base-devel rust cmake qt6-base qt6-declarative ffmpeg glib2 yt-dlp \
    swaybg wlr-randr layer-shell-qt qt6-wayland qt6-multimedia
yay -S mpvpaper lswt        # están en AUR (vídeos de fondo y su pausa)
```

(En Artix, algunos vienen de los repos de Arch: instala `artix-archlinux-support` si pacman no los encuentra.)
En Void: `sudo xbps-install -S mpvpaper swaybg wlr-randr lswt layer-shell-qt qt6-wayland qt6-multimedia`.

Después de compilar, `~/.local/bin/nekowall wlr-shortcut --gui ~/.local/bin/nekowall-gui` deja el fondo
restaurándose al iniciar sesión (autostart XDG y, en Wayfire, `[autostart]` de `wayfire.ini`) y, en
**Wayfire**, el atajo **Meta+Alt+W** (`[command]` de `wayfire.ini`). En sway/labwc/Hyprland el atajo se añade
a mano, por ejemplo en sway: `bindsym Mod4+Mod1+w exec ~/.local/bin/nekowall-gui --hud`.

#### Cómo funciona con wlroots

- **Pantallas**: `wlr-randr --json`.
- **Iconos del escritorio con pcmanfm-qt** (`pcmanfm-qt --desktop`): pcmanfm-qt pinta su propio fondo bajo
  los iconos, así que cualquier otro programa de fondos (swaybg, Waywallen…) o queda tapado o tapa los
  iconos. NekoWall lo detecta y le pone el fondo **a pcmanfm-qt** (`--set-wallpaper`): iconos y fondo se ven a
  la vez. Con pcmanfm-qt hay un solo fondo para todas las pantallas. Con un **vídeo**, el vídeo va encima
  de su escritorio (como en MATE): para ver los iconos, pon una imagen.
- **Iconos del escritorio de NekoWall** (alternativa; Ajustes → «Usar los iconos del escritorio de NekoWall», o
  `nekowall wlr-desktop-icons on --gui ~/.local/bin/nekowall-gui`): NekoWall muestra la carpeta **Escritorio**
  en una capa transparente (layer-shell «bottom») **encima del vídeo** (mpvpaper en «background»), así iconos y
  fondo animado se ven a la vez. **Imita al escritorio de pcmanfm-qt 2.5** y usa sus mismos archivos
  (`~/.config/pcmanfm-qt/default/`), así que se puede volver a pcmanfm-qt sin perder nada:
  - `settings.conf`: tamaño de icono, letra, color del texto y su sombra, márgenes de celda y del área de
    trabajo, orden (nombre/tipo/tamaño/fecha, ascendente, carpetas primero, ocultos al final), mostrar ocultos,
    clic simple, descripciones emergentes, «fijar todos», `HideItems-<pantalla>`, confirmar papelera/borrado,
    miniaturas, accesos Inicio/Papelera/Equipo/Red (la Papelera se actualiza: vacía / N elementos).
  - `desktop-items-<pantalla>.conf`: la posición de cada icono fijado. Un escritorio por pantalla, como
    pcmanfm-qt en Wayland. La rejilla se calcula igual (13 caracteres de ancho × 3 líneas, `removeBottomGap`,
    `alignToGrid`), así que los iconos quedan en el mismo sitio (comprobado lado a lado en la VM).
  - Ratón: clic selecciona, doble clic abre (o un clic con «clic simple»), Ctrl/Mayús, recuadro de selección,
    arrastrar para recolocar (queda fijo y se guarda), soltar sobre una carpeta (mueve) o la Papelera,
    soltar archivos desde otro programa (mueve si es el mismo disco, copia si no; se leen `text/uri-list` y
    `libfm/files` porque sin xdg-desktop-portal Qt da la lista vacía).
  - Teclado: flechas, Inicio/Fin, RePág/AvPág, Intro, Supr (papelera), Mayús+Supr (borrar), F2 (renombrar
    en el sitio; en lanzadores cambia `Name=`), Ctrl+A/C/X/V, Ctrl+Mayús+N, Ctrl+H, Esc, tecla Menú.
  - Menús de pcmanfm-qt en español: archivo (Abrir, Abrir con ▸ según mimeapps, Cortar, Copiar, Copiar ruta,
    Pegar en carpeta, Papelera, Eliminar, Renombrar, Fijar en la posición actual, Propiedades), Papelera
    (Abrir, Fijar, Vaciar) y escritorio (Pegar, Seleccionar todo, Invertir, Crear nuevo ▸, Ordenar ▸,
    Mostrar ocultos, Ocultar iconos, Crear lanzador…, Preferencias del escritorio…) + abrir en el gestor de
    archivos, terminal aquí y cambiar el fondo. Las carpetas se abren con pcmanfm-qt si está (como él).
  - «Preferencias del escritorio» propias (las de pcmanfm-qt no abren sin su escritorio) que escriben en su
    `settings.conf`. Sin pcmanfm-qt se usa `~/.config/nekowall/escritorio/` con el mismo formato.
  - Al activarlo, en Wayfire la línea de `pcmanfm-qt --desktop` de `[autostart]` queda comentada (vuelve al
    desactivarlo) y se añade `nekowall_desktop = … --desktop`; en otros, autostart XDG. Proceso aparte
    (`nekowall-gui --desktop`), una sola instancia. No hace: arrastrar iconos hacia otros programas.
- **Sin pcmanfm-qt**: imágenes con **awww** (si está) o **swaybg** (uno por pantalla).
- **Vídeos**: **mpvpaper** por pantalla, sin audio ni configuración personal de mpv; debajo, un fotograma.
- **Pausa**: `nekowall _wlr-video` mira las ventanas con **lswt** (maximizadas, activas, visibles) y pausa
  según Ajustes.
- **No uses otro gestor de fondos a la vez** (Waywallen, swaybg en el autostart del compositor…): pelearían
  por el fondo.
- **Vídeo con los iconos de pcmanfm-qt**: al poner un vídeo, NekoWall le pone a pcmanfm-qt un SVG
  transparente en mosaico (`~/.cache/nekowall/transparente.svg`, el mismo truco que usan los de Waywallen) y
  mpvpaper va en la capa «background», debajo. Como pcmanfm-qt también usa esa capa y queda encima lo último
  que se abre, si su escritorio es anterior al vídeo NekoWall lo reabre: con ventanas de pcmanfm-qt abiertas,
  `--desktop-off` / `--desktop` (mismo proceso); sin ninguna, lo reinicia con su misma línea de arranque
  (con `--desktop-off` se cerraría entero), comprueba que sigue vivo y reintenta (salida en
  `~/.cache/nekowall/pcmanfm.log`). Nunca le manda órdenes a un pcmanfm-qt con menos de 3 s en marcha: al
  iniciar sesión, una orden adelantada se quedaba como instancia principal y su escritorio se cerraba. Resultado: SUS iconos encima del vídeo en
  movimiento. Ojo: con un fondo de *color* transparente (`BgColor=#00000000`) Wayfire deja de pedirle
  fotogramas al vídeo y se congela; con el SVG en mosaico no. Con una imagen, se la pone a pcmanfm-qt.
- **Perfil y entorno de pcmanfm-qt**: se leen de su línea de arranque (`[autostart]` de Wayfire o autostart
  XDG), p. ej. `env XDG_CURRENT_DESKTOP=LXQt pcmanfm-qt --desktop --profile=lxqt`, o del proceso en marcha.
- *Probado en una máquina virtual con Artix (runit) + Wayfire 0.11 + pcmanfm-qt 2.5, también con la
  configuración de un usuario real (perfil `lxqt`, `XDG_CURRENT_DESKTOP=LXQt`, «fijar todos», márgenes
  propios): `./install.sh --yes` (pacman + yay), imagen, vídeo con sus iconos encima, pausa con ventana
  maximizada, Meta+Alt+W, ventana principal y restaurar al reiniciar.*

### Compilar (todos los escritorios)

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

### Instalar el programa (todos los escritorios)

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
nekowall apply <id|ruta> [--monitor PANTALLA]   Aplica un fondo (en todas las pantallas o en una)
nekowall monitors                    Pantallas conectadas y el fondo de cada una
nekowall next | previous | restore [--monitor PANTALLA]   Siguiente / anterior / volver al anterior
nekowall favorite <id> [--off]       Marca o desmarca favorito
nekowall remove <id>                 Quita de la biblioteca (no borra el archivo)
nekowall sync                        Busca fondos nuevos (o borrados) en las carpetas añadidas
nekowall folders [--forget <ruta>]   Carpetas que se revisan (o deja de revisar una)
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
- En KDE, los vídeos necesitan el plugin Smart Video Wallpaper Reborn.

## Licencia

**PolyForm Noncommercial 1.0.0** (texto completo en [`LICENSE`](../LICENSE)). © 2026 Lost (LostXXI).

- Puedes usar, estudiar, modificar y compartir NekoWall para **cualquier fin no comercial** (uso personal,
  estudio, organizaciones sin ánimo de lucro…).
- **No** se puede vender ni usar para ganar dinero sin permiso del autor.
- Quien lo comparta (o comparta una versión modificada) debe incluir la licencia y la línea
  `Required Notice: Copyright © 2026 Lost (LostXXI) …` del archivo `LICENSE`.
- Cada archivo de código lleva su identificador `SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0` y la
  línea de autoría.

Las bibliotecas y programas que usa NekoWall (Qt, mpv, ffmpeg, yt-dlp…) conservan sus propias licencias.

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
