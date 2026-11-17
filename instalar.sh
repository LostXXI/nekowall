#!/bin/sh
# instalar.sh — instala las dependencias de NekoWall en Void Linux / neko-void y, si quieres,
# compila e instala el programa para tu usuario.
#
#   ./instalar.sh                 detecta el escritorio y pregunta en cada paso
#   ./instalar.sh --kde | --mate  fuerza el escritorio
#   ./instalar.sh --solo-dependencias
#   ./instalar.sh --si            responde «sí» a todo (sin preguntas)
#   ./instalar.sh --simular       muestra lo que haría, sin instalar ni cambiar nada
#
# No lo ejecutes como root: compila con tu usuario y solo usa sudo/doas para xbps-install.

set -eu

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

# ── Paquetes ────────────────────────────────────────────────────────────────
COMUNES="git rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel ffmpeg6 glib"
KDE="kf6-kwindowsystem-devel qt6-wayland layer-shell-qt kf6-qqc2-desktop-style qt6-multimedia"
MATE="mpv xwinwrap xprop xdotool"

# ── Opciones ────────────────────────────────────────────────────────────────
ESCRITORIO=""
SOLO_DEPS=0
SI=0
SIMULAR=0
for a in "$@"; do
    case "$a" in
        --kde) ESCRITORIO=kde ;;
        --mate) ESCRITORIO=mate ;;
        --solo-dependencias) SOLO_DEPS=1 ;;
        --si|--sí|-y) SI=1 ;;
        --simular) SIMULAR=1 ;;
        -h|--help|--ayuda) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Opción desconocida: $a (prueba --ayuda)" >&2; exit 1 ;;
    esac
done

# ── Utilidades ──────────────────────────────────────────────────────────────
if [ -t 1 ]; then V='\033[1;32m'; A='\033[1;33m'; R='\033[1;31m'; N='\033[0m'; else V=''; A=''; R=''; N=''; fi
paso()  { printf "${V}==>${N} %s\n" "$*"; }
aviso() { printf "${A}aviso:${N} %s\n" "$*"; }
error() { printf "${R}error:${N} %s\n" "$*" >&2; exit 1; }

# Ejecuta un comando, o solo lo muestra con --simular.
hacer() {
    if [ "$SIMULAR" = 1 ]; then printf '   [simulado] %s\n' "$*"; else "$@"; fi
}

preguntar() { # preguntar "¿Texto?" → 0 = sí
    [ "$SI" = 1 ] && return 0
    [ "$SIMULAR" = 1 ] && { printf '%s [S/n] (simulado: sí)\n' "$1"; return 0; }
    printf '%s [S/n] ' "$1"
    read -r r || r=n
    case "$r" in ""|s|S|si|sí|Si|Sí|y|Y) return 0 ;; *) return 1 ;; esac
}

# ── Comprobaciones ──────────────────────────────────────────────────────────
[ "$(id -u)" = 0 ] && error "no lo ejecutes como root: usa tu usuario (el script pedirá sudo/doas cuando haga falta)."
command -v xbps-install >/dev/null 2>&1 || error "este script es para Void Linux (no encontré xbps-install). Instala a mano: $COMUNES y los del README."

# doas si está configurado (lo usa neko-void), si no sudo.
if command -v doas >/dev/null 2>&1 && [ -e /etc/doas.conf ]; then
    ROOT=doas
elif command -v sudo >/dev/null 2>&1; then
    ROOT=sudo
elif command -v doas >/dev/null 2>&1; then
    ROOT=doas
else
    error "no encontré sudo ni doas para instalar paquetes."
fi

if [ -z "$ESCRITORIO" ]; then
    case "$(printf '%s' "${XDG_CURRENT_DESKTOP:-}" | tr '[:upper:]' '[:lower:]')" in
        *kde*) ESCRITORIO=kde ;;
        *mate*) ESCRITORIO=mate ;;
    esac
fi
if [ -z "$ESCRITORIO" ]; then
    aviso "no reconozco tu escritorio (XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-vacío})."
    echo "NekoWall funciona en KDE Plasma (Wayland) y MATE (X11). Elige con --kde o --mate."
    exit 1
fi

case "$ESCRITORIO" in
    kde) NOMBRE="KDE Plasma"; PAQUETES="$COMUNES $KDE" ;;
    mate) NOMBRE="MATE"; PAQUETES="$COMUNES $MATE" ;;
esac

paso "Escritorio: $NOMBRE · permisos con: $ROOT$( [ "$SIMULAR" = 1 ] && echo ' · MODO SIMULACIÓN')"

# ── 1. Dependencias ─────────────────────────────────────────────────────────
FALTAN=""
for p in $PAQUETES; do
    xbps-query "$p" >/dev/null 2>&1 || FALTAN="$FALTAN $p"
done
if [ -z "$FALTAN" ]; then
    paso "Todas las dependencias ya están instaladas."
else
    paso "Faltan:$FALTAN"
    preguntar "¿Instalarlas ahora con $ROOT xbps-install?" || error "sin estas dependencias NekoWall no se puede compilar."
    # shellcheck disable=SC2086
    if ! hacer $ROOT xbps-install -Sy $FALTAN; then
        # Causa típica en Void: el sistema no está al día y los repos ya traen otras versiones
        # («broken, unresolvable shlib …»). Hay que actualizar primero.
        PENDIENTES=$(xbps-install -nu 2>/dev/null | wc -l)
        [ "$PENDIENTES" -gt 0 ] || error "no se pudieron instalar las dependencias (mira el mensaje de xbps arriba)."
        aviso "tu sistema tiene $PENDIENTES actualizaciones pendientes: Void no deja instalar paquetes nuevos"
        echo "      hasta ponerlo al día (por eso salen errores como «unresolvable shlib»)."
        preguntar "¿Actualizar el sistema ahora ($ROOT xbps-install -Su) y reintentar?" \
            || error "actualiza con «$ROOT xbps-install -Su» y vuelve a ejecutar este script."
        hacer $ROOT xbps-install -Syu -y xbps          # primero el propio xbps, por si cambió
        hacer $ROOT xbps-install -Syu -y
        # shellcheck disable=SC2086
        hacer $ROOT xbps-install -Sy $FALTAN || error "siguen sin poder instalarse; mira el mensaje de xbps arriba."
    fi
fi

if [ "$ESCRITORIO" = kde ] && [ ! -d /usr/share/plasma/wallpapers/luisbocanegra.smart.video.wallpaper.reborn ] \
   && [ ! -d "$HOME/.local/share/plasma/wallpapers/luisbocanegra.smart.video.wallpaper.reborn" ]; then
    aviso "para fondos de VÍDEO en KDE falta el plugin «Smart Video Wallpaper Reborn» (no está en los repos de Void)."
    echo "      Clic derecho en el escritorio → Configurar escritorio y fondo → Obtener nuevos complementos… → búscalo."
    echo "      Sin él, NekoWall funciona igual con imágenes."
fi

[ "$SOLO_DEPS" = 1 ] && { paso "Listo (solo dependencias)."; exit 0; }

# ── 2. Compilar ─────────────────────────────────────────────────────────────
[ -f "$DIR/Cargo.toml" ] && [ -d "$DIR/app" ] || error "ejecuta el script desde la carpeta del código de NekoWall."
preguntar "¿Compilar NekoWall ahora? (unos minutos la primera vez)" || { paso "Listo. Para compilar más tarde, mira el README."; exit 0; }

cd "$DIR"
paso "Compilando el núcleo (Rust)…"
hacer cargo build --release
paso "Compilando la interfaz (Qt)…"
hacer cmake -S app -B app/build -DCMAKE_BUILD_TYPE=Release
hacer cmake --build app/build -j"$(nproc 2>/dev/null || echo 2)"

# ── 3. Instalar para tu usuario ─────────────────────────────────────────────
preguntar "¿Instalar NekoWall en ~/.local/bin y añadirlo al menú?" || { paso "Compilado en $DIR (sin instalar)."; exit 0; }

BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
hacer mkdir -p "$BIN" "$APPS"
hacer install -m755 target/release/nekowall app/build/nekowall-gui "$BIN/"

DESKTOP="[Desktop Entry]
Type=Application
Name=NekoWall
GenericName=Gestor de fondos de pantalla
Comment=Cambia fondos de pantalla estáticos y animados
Exec=$BIN/nekowall-gui
Icon=preferences-desktop-wallpaper
Terminal=false
Categories=Settings;DesktopSettings;Qt;
Actions=hud;

[Desktop Action hud]
Name=Selector rápido
Exec=$BIN/nekowall-gui --hud
X-KDE-Shortcuts=Meta+Alt+W"
if [ "$SIMULAR" = 1 ]; then
    echo "   [simulado] escribir $APPS/nekowall.desktop"
else
    printf '%s\n' "$DESKTOP" > "$APPS/nekowall.desktop"
fi

# Icono en el escritorio (opcional). La carpeta cambia según el idioma: «Escritorio», «Desktop»…
CARPETA_ESC=$(xdg-user-dir DESKTOP 2>/dev/null || true)
{ [ -n "$CARPETA_ESC" ] && [ "$CARPETA_ESC" != "$HOME" ]; } || CARPETA_ESC="$HOME/Desktop"
if [ -d "$CARPETA_ESC" ] && preguntar "¿Poner también un icono de NekoWall en el escritorio ($CARPETA_ESC)?"; then
    if [ "$SIMULAR" = 1 ]; then
        echo "   [simulado] escribir $CARPETA_ESC/nekowall.desktop"
    else
        printf '%s\n' "$DESKTOP" > "$CARPETA_ESC/nekowall.desktop"
        chmod +x "$CARPETA_ESC/nekowall.desktop"   # sin esto MATE y KDE lo tratan como «no confiable»
        command -v gio >/dev/null 2>&1 && gio set "$CARPETA_ESC/nekowall.desktop" metadata::trusted true 2>/dev/null || true
    fi
fi

# ── 4. Atajos ───────────────────────────────────────────────────────────────
if [ "$ESCRITORIO" = mate ]; then
    KB=org.mate.control-center.keybinding
    hacer gsettings set "$KB:/org/mate/desktop/keybindings/nekowall/" name "'NekoWall · Selector rápido'"
    hacer gsettings set "$KB:/org/mate/desktop/keybindings/nekowall/" action "'\"$BIN/nekowall-gui\" --hud'"
    hacer gsettings set "$KB:/org/mate/desktop/keybindings/nekowall/" binding "'<Mod4><Alt>w'"
    hacer gsettings set "$KB:/org/mate/desktop/keybindings/nekowall-peek/" name "'NekoWall · Mostrar escritorio'"
    hacer gsettings set "$KB:/org/mate/desktop/keybindings/nekowall-peek/" action "'\"$BIN/nekowall\" peek'"
    hacer gsettings set "$KB:/org/mate/desktop/keybindings/nekowall-peek/" binding "'<Mod4><Alt>d'"
    ATAJOS="Meta+Alt+W abre el selector y Meta+Alt+D muestra los iconos con un vídeo de fondo (ya funcionan)."
else
    command -v kbuildsycoca6 >/dev/null 2>&1 && hacer kbuildsycoca6 >/dev/null 2>&1 || true
    ATAJOS="Cierra sesión y vuelve a entrar una vez para que KDE active Meta+Alt+W (selector)."
fi

# ── Fin ─────────────────────────────────────────────────────────────────────
echo
if [ "$SIMULAR" = 1 ]; then paso "Simulación terminada: no se ha instalado ni cambiado nada."; exit 0; fi
paso "NekoWall instalado."
echo "   • Ábrelo desde el menú (NekoWall) o con:  $BIN/nekowall-gui"
echo "   • $ATAJOS"
echo "   • Primer paso: «Añadir carpeta…» con tus fondos."
case ":$PATH:" in *":$BIN:"*) ;; *) aviso "$BIN no está en tu PATH: para usar «nekowall» en la terminal, añádelo a tu ~/.bashrc." ;; esac
