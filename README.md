# NekoWall

Pon **imágenes y vídeos** como fondo de pantalla en **Void Linux** / neko-void, con **KDE Plasma**, **MATE** o **niri**.
También fuera de neko-void: **Arch / Artix** con **Wayfire**, sway, labwc u otro compositor wlroots.
Trae un buscador de fondos (**Workshop**) y un **instalador de fondos con enlaces**: pegas un enlace (YouTube,
Reddit…) y lo descarga en la calidad que elijas.

## Instalación paso a paso

### Paso 1 · Comprueba tu escritorio

Abre una terminal y escribe:

```bash
echo $XDG_CURRENT_DESKTOP
```

- Si sale **KDE**, **MATE**, **niri**, **wayfire**, **sway**, **labwc** o **Hyprland** → sigue al paso 2.
- Si sale otra cosa (XFCE, LXQt, i3…) → todavía no es compatible, lo siento.

### Paso 2 · Descarga NekoWall

```bash
git clone https://github.com/LostXXI/nekowall.git
cd nekowall
```

> ¿Sale `git: command not found`? Instálalo primero con `sudo xbps-install -S git` (o `doas` en vez de `sudo`).

### Paso 3 · Ejecuta el instalador

```bash
./install.sh
```

Responde **S** (sí) a todas las preguntas y escribe tu contraseña cuando la pida.
Tarda unos minutos: instala lo que falte y compila el programa. Cuando termine, NekoWall estará en el
menú de aplicaciones.

> Si sale un error con **«unresolvable shlib»**, tu sistema necesita actualizarse. El instalador **no lo hace por ti**
> (actualizar puede cambiar el núcleo y recompilar drivers como NVIDIA): te avisa de qué se actualizaría.
> Actualiza tú con `sudo xbps-install -Su`, reinicia, comprueba que todo arranca y vuelve a ejecutar `./install.sh`.

> **Arch / Artix con Wayfire, sway, labwc…:** el instalador detecta el compositor aunque `XDG_CURRENT_DESKTOP`
> diga otra cosa (p. ej. `LXQt`), instala lo de los repos con `pacman` y `mpvpaper`/`lswt` con `yay` o `paru`.
> Si usas **Waywallen** u otro gestor de fondos (swww, wpaperd, hyprpaper), te ofrece desactivarlo: su línea
> de arranque queda comentada (con copia `wayfire.ini.antes-de-nekowall`). Si usas los iconos de
> **pcmanfm-qt**, no hay que tocar nada: NekoWall pone el vídeo detrás de ellos.

**Opciones del instalador** (se pueden combinar, por ejemplo `./install.sh --yes --niri`):

| Opción | Qué hace |
|---|---|
| `--yes` o `-y` | Responde «sí» a todo: instala sin hacer preguntas |
| `--dry-run` | Muestra lo que haría, sin instalar ni cambiar nada (para mirar antes) |
| `--deps-only` | Solo instala los paquetes que faltan, sin compilar NekoWall |
| `--kde` / `--mate` / `--niri` / `--wlroots` | Elige el escritorio a mano (normalmente lo detecta solo; `--wlroots` = Wayfire, sway, labwc…) |
| `--help` o `-h` | Muestra esta ayuda |

### Paso 4 · Solo KDE: cierra sesión y vuelve a entrar

Hazlo **una vez**, para que funcione el atajo **Meta+Alt+W**. (En MATE y niri no hace falta: el atajo funciona al momento.)

**¿Quieres vídeos de fondo en KDE?** Instala el plugin *Smart Video Wallpaper Reborn*:
clic derecho en el escritorio → **Configurar el escritorio y el fondo de pantalla…** →
**Obtener nuevos complementos…** → búscalo → **Instalar**. Sin él, las imágenes funcionan igual.

### Paso 5 · Añade tus fondos

1. Abre **NekoWall** desde el menú.
2. Copia tus imágenes y vídeos en la carpeta **`nekowallpapers`**, dentro de tu carpeta de **Imágenes**
   (`~/Pictures/nekowallpapers`). NekoWall la crea solo y lo que copies ahí **aparece automáticamente**.
   El botón **Mis fondos** la abre. ¿Ya tienes tus fondos en otra carpeta? Usa **Añadir carpeta…**.
   ¿No tienes fondos? Entra en **Workshop** (menú de la izquierda) y descárgalos con un clic.
3. Pulsa **Aplicar** en el que quieras. ¡Listo! 🎉

## Atajos de teclado

*Meta = tecla Windows.*

| Atajo | Qué hace |
| --- | --- |
| **Meta+Alt+W** | Abre el selector rápido (desde cualquier sitio) |
| ← → / ↑ ↓ | En el selector: moverse entre vídeos / imágenes |
| **Enter** · **Esc** | En el selector: aplicar · cerrar |
| **P** | En el selector, con varias pantallas: elegir en cuál se aplica (o en todas) |
| **Meta+Alt+D** | Solo MATE con vídeo de fondo: ver los iconos del escritorio (otra vez para volver) |

## Si algo no funciona

| Problema | Solución |
| --- | --- |
| Meta+Alt+W no hace nada en KDE | Cierra sesión y vuelve a entrar una vez |
| En KDE los vídeos no se aplican | Falta el plugin de vídeo (paso 4) |
| En MATE no veo los iconos con un vídeo de fondo | Es normal: pulsa **Meta+Alt+D** |
| `nekowall: command not found` | Añade `export PATH="$HOME/.local/bin:$PATH"` a tu `~/.bashrc` (el menú y los atajos funcionan igual) |

¿Otra cosa? Abre un [*issue*](https://github.com/LostXXI/nekowall/issues) y pega lo que sale al ejecutar
`~/.local/bin/nekowall status`.

## Actualizar o desinstalar

**Actualizar:** dentro de la carpeta `nekowall`, ejecuta `git pull` y después `./install.sh`.

**Desinstalar:**

```bash
rm ~/.local/bin/nekowall ~/.local/bin/nekowall-gui ~/.local/share/applications/nekowall.desktop
```

(Si pusiste el icono en el escritorio, bórralo también. Tus fondos no se tocan: la carpeta
`~/Pictures/nekowallpapers` se queda donde está.)

---

## ¿Qué es NekoWall y cómo funciona?

NekoWall es un gestor de fondos de pantalla. Reúne tus imágenes y vídeos en una **biblioteca** con
miniaturas y te deja cambiarlos con un clic o con el **selector rápido** (Meta+Alt+W), que muestra tus
fondos en dos carruseles: vídeos en horizontal e imágenes en vertical.

- **No copia tus archivos**: solo recuerda dónde están.
- **Tiene su propia carpeta** (`~/Pictures/nekowallpapers`) y **vigila tus carpetas**: si copias o borras
  fondos en ellas, la biblioteca se actualiza sola.
- **Workshop**: busca y descarga fondos de [Wallhaven](https://wallhaven.cc) sin salir de NekoWall (solo
  contenido apto). Se guardan en `nekowallpapers` y aparecen solos en la biblioteca.
- **Instalador de fondos con enlaces** («Descargar con enlace», el botón azul de la Biblioteca): pega un
  enlace de YouTube, Reddit, una página con vídeo o un archivo directo, mira la vista previa y elige la
  calidad (de menor a mayor). Se descarga sin audio, listo para usar como fondo.
- **Varias pantallas**: cada una puede tener su propio fondo (imagen o vídeo). El fondo se aplica en la
  pantalla donde está el ratón, o eliges otra (o todas) con el botón del selector o de la biblioteca.
- **Ahorra batería**: pausa el vídeo de fondo cuando una ventana lo tapa o la batería está baja.
- **Cerrado no gasta memoria**: no deja nada funcionando en segundo plano.
- Detecta solo tu escritorio: en **KDE** usa las herramientas de Plasma, en **MATE** reproduce el vídeo con
  `mpv` detrás de tus ventanas y en **niri** pone las imágenes con Noctalia (o awww) y los vídeos con
  `mpvpaper`. Con Noctalia, los colores del sistema siguen al fondo.

Está hecho en Rust (el núcleo) y Qt/QML (la interfaz). Instalación manual, uso desde la terminal y detalles
técnicos: [docs/DETALLES.md](docs/DETALLES.md).

## Licencia

© 2026 **Lost (LostXXI)**. NekoWall usa la licencia **PolyForm Noncommercial 1.0.0** ([`LICENSE`](LICENSE)):
puedes usarlo, modificarlo y compartirlo libremente **sin fines comerciales**. Venderlo o usarlo para ganar
dinero requiere permiso del autor.
