#!/bin/sh
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
# install.sh — instala las dependencias de NekoWall en Void Linux / neko-void (xbps) o en Arch / Artix
# (pacman; solo con compositores wlroots) y, si quieres,
# compila e instala el programa para tu usuario.
#
#   ./install.sh                 detecta el escritorio y pregunta en cada paso
#   ./install.sh --kde | --mate | --niri | --wlroots  fuerza el escritorio
#                                (wlroots = Wayfire, sway, labwc, Hyprland, river…)
#   ./install.sh --deps-only     solo instala las dependencias
#   ./install.sh --yes  (-y)     responde «sí» a todo (sin preguntas)
#   ./install.sh --dry-run       muestra lo que haría, sin instalar ni cambiar nada
#   ./install.sh --help  (-h)    muestra esta ayuda
#
# No lo ejecutes como root: compila con tu usuario y solo usa sudo/doas para xbps-install.

set -eu

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

# ── Paquetes ────────────────────────────────────────────────────────────────
COMUNES="git rust cargo gcc cmake make qt6-base-devel qt6-declarative-devel qt6-svg xdg-utils ffmpeg6 glib yt-dlp"
KDE="kf6-kwindowsystem-devel qt6-wayland layer-shell-qt kf6-qqc2-desktop-style qt6-multimedia libkf6screen"
# Los iconos de NekoWall en MATE (imitan a Caja) miden y dibujan los nombres con Pango, leen los ajustes con
# GIO, el DPI de XSETTINGS con xcb y las posiciones que tiene Caja con AT-SPI: se compilan con sus cabeceras.
MATE="mpv xwinwrap xprop xdotool xrandr wmctrl qt6-multimedia pango-devel cairo-devel glib-devel libxcb-devel at-spi2-core-devel"
NIRI="mpvpaper awww layer-shell-qt qt6-wayland qt6-multimedia"
WLR="mpvpaper swaybg wlr-randr lswt layer-shell-qt qt6-wayland qt6-multimedia"
# Arch / Artix (pacman). mpvpaper y lswt están en AUR: se instalan con yay o paru si los hay.
ARCH_COMUNES="git base-devel rust cmake qt6-base qt6-declarative qt6-svg xdg-utils ffmpeg glib2 yt-dlp"
ARCH_WLR="swaybg wlr-randr layer-shell-qt qt6-wayland qt6-multimedia"
ARCH_AUR="mpvpaper lswt"

# ── Opciones ────────────────────────────────────────────────────────────────
ESCRITORIO=""
SOLO_DEPS=0
SI=0
SIMULAR=0
for a in "$@"; do
    case "$a" in
        --kde) ESCRITORIO=kde ;;
        --mate) ESCRITORIO=mate ;;
        --niri) ESCRITORIO=niri ;;
        --wlroots|--wayfire|--sway|--labwc|--hyprland|--river) ESCRITORIO=wlroots ;;
        # (los nombres en español siguen funcionando, por compatibilidad)
        --deps-only|--solo-dependencias) SOLO_DEPS=1 ;;
        --yes|-y|--si|--sí) SI=1 ;;
        --dry-run|--simular) SIMULAR=1 ;;
        -h|--help|--ayuda) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Opción desconocida: $a (prueba --help)" >&2; exit 1 ;;
    esac
done

# ── Utilidades ──────────────────────────────────────────────────────────────
if [ -t 1 ]; then V='\033[1;32m'; A='\033[1;33m'; R='\033[1;31m'; N='\033[0m'; else V=''; A=''; R=''; N=''; fi
paso()  { printf "${V}==>${N} %s\n" "$*"; }
aviso() { printf "${A}aviso:${N} %s\n" "$*"; }
error() { printf "${R}error:${N} %s\n" "$*" >&2; exit 1; }

# Ejecuta un comando, o solo lo muestra con --dry-run.
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
if command -v xbps-install >/dev/null 2>&1; then
    GESTOR=xbps
elif command -v pacman >/dev/null 2>&1; then
    GESTOR=pacman
else
    error "este script es para Void Linux (xbps) o Arch/Artix (pacman). En otro sistema instala a mano lo de docs/DETALLES.md."
fi

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
        *niri*) ESCRITORIO=niri ;;
        *wayfire*|*sway*|*labwc*|*hyprland*|*river*|*wlroots*) ESCRITORIO=wlroots ;;
    esac
fi
# Si XDG_CURRENT_DESKTOP no lo dice (vacío, o p. ej. «LXQt» puesto para pcmanfm-qt), el compositor en marcha.
if [ -z "$ESCRITORIO" ]; then
    for c in wayfire sway labwc Hyprland river dwl; do
        pgrep -x "$c" >/dev/null 2>&1 && { ESCRITORIO=wlroots; break; }
    done
    [ -z "$ESCRITORIO" ] && pgrep -x niri >/dev/null 2>&1 && ESCRITORIO=niri
fi
if [ -z "$ESCRITORIO" ]; then
    aviso "no reconozco tu escritorio (XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-vacío})."
    echo "NekoWall funciona en KDE Plasma, MATE, niri y compositores wlroots (Wayfire, sway, labwc…)."
    echo "Elige con --kde, --mate, --niri o --wlroots."
    exit 1
fi

case "$ESCRITORIO" in
    kde) NOMBRE="KDE Plasma"; PAQUETES="$COMUNES $KDE" ;;
    mate) NOMBRE="MATE"; PAQUETES="$COMUNES $MATE" ;;
    niri) NOMBRE="niri"; PAQUETES="$COMUNES $NIRI" ;;
    wlroots) NOMBRE="wlroots (${XDG_CURRENT_DESKTOP:-Wayfire/sway/labwc…})"; PAQUETES="$COMUNES $WLR" ;;
esac
if [ "$GESTOR" = pacman ]; then
    [ "$ESCRITORIO" = wlroots ] || error "en Arch/Artix, por ahora NekoWall solo se instala con compositores wlroots (Wayfire, sway, labwc…)."
    PAQUETES="$ARCH_COMUNES $ARCH_WLR"
fi

paso "Escritorio: $NOMBRE · paquetes con: $GESTOR · permisos con: $ROOT$( [ "$SIMULAR" = 1 ] && echo ' · MODO SIMULACIÓN')"

# ── 1. Dependencias (Arch / Artix) ──────────────────────────────────────────
if [ "$GESTOR" = pacman ]; then
    FALTAN=""
    for p in $PAQUETES; do
        pacman -Qq "$p" >/dev/null 2>&1 || pacman -Qqg "$p" >/dev/null 2>&1 || FALTAN="$FALTAN $p"
    done
    if [ -z "$FALTAN" ]; then
        paso "Todas las dependencias de los repositorios ya están instaladas."
    else
        paso "Faltan:$FALTAN"
        preguntar "¿Instalarlas ahora con $ROOT pacman?" || error "sin estas dependencias NekoWall no se puede compilar."
        # shellcheck disable=SC2086
        hacer $ROOT pacman -S --needed $( [ "$SI" = 1 ] && echo --noconfirm ) $FALTAN \
            || error "no se pudieron instalar (en Artix, algunos paquetes vienen de los repos de Arch: instala artix-archlinux-support)."
    fi
    FALTAN_AUR=""
    for p in $ARCH_AUR; do pacman -Qq "$p" >/dev/null 2>&1 || FALTAN_AUR="$FALTAN_AUR $p"; done
    if [ -n "$FALTAN_AUR" ]; then
        AUR=""
        command -v paru >/dev/null 2>&1 && AUR=paru
        command -v yay >/dev/null 2>&1 && AUR=yay
        if [ -n "$AUR" ]; then
            if preguntar "Faltan$FALTAN_AUR (de AUR: vídeos de fondo y su pausa). ¿Instalarlos con $AUR?"; then
                # shellcheck disable=SC2086
                hacer $AUR -S --needed $( [ "$SI" = 1 ] && echo --noconfirm ) $FALTAN_AUR || aviso "no se pudieron instalar$FALTAN_AUR"
            fi
        else
            aviso "faltan$FALTAN_AUR, que están en AUR (mpvpaper = vídeos de fondo; lswt = pausarlos)."
            echo "      Instálalos con tu ayudante de AUR (yay -S$FALTAN_AUR) o a mano con makepkg. Sin ellos,"
            echo "      NekoWall funciona igual con imágenes."
        fi
    fi
fi

# Tema de iconos: sin ninguno (solo hicolor), los iconos del escritorio de NekoWall saldrían vacíos.
if [ "$ESCRITORIO" = wlroots ] || [ "$ESCRITORIO" = niri ]; then
    TEMAS=$(for d in /usr/share/icons/* "$HOME/.local/share/icons"/* "$HOME/.icons"/*; do
                [ -f "$d/index.theme" ] && [ -d "$d/scalable" -o -d "$d/48x48" -o -d "$d/32x32" ] && basename "$d"
            done 2>/dev/null | grep -vx 'hicolor\|default\|locolor' | head -1)
    if [ -z "$TEMAS" ]; then
        aviso "no hay ningún tema de iconos instalado: se instalará Adwaita (para los iconos de archivos y carpetas)."
        if [ "$GESTOR" = pacman ]; then
            hacer $ROOT pacman -S --needed $( [ "$SI" = 1 ] && echo --noconfirm ) adwaita-icon-theme || aviso "no se pudo instalar adwaita-icon-theme"
        else
            hacer $ROOT xbps-install -Sy adwaita-icon-theme || aviso "no se pudo instalar adwaita-icon-theme"
        fi
    fi
fi

# ── 1. Dependencias (Void) ──────────────────────────────────────────────────
FALTAN=""
[ "$GESTOR" = xbps ] && {
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
        # («broken, unresolvable shlib …»). Lo único que se actualiza aquí es el propio xbps (no toca el
        # núcleo ni los drivers). Actualizar TODO el sistema NUNCA lo hace este script, ni con --yes:
        # puede instalar un núcleo nuevo y recompilar drivers DKMS (NVIDIA, Wi-Fi…); si uno falla, el
        # equipo puede no arrancar. Eso lo decide y lo hace el usuario.
        if xbps-install -nu 2>/dev/null | grep -q "^xbps-[0-9]"; then
            paso "Hay una versión nueva de xbps: actualizo solo xbps (no toca el núcleo ni los drivers) y reintento."
            hacer $ROOT xbps-install -Suy xbps || true
            # shellcheck disable=SC2086
            hacer $ROOT xbps-install -Sy $FALTAN && OK_XBPS=1
        fi
        if [ "${OK_XBPS:-0}" != 1 ]; then
            PENDIENTES=$(xbps-install -nu 2>/dev/null | wc -l)
            [ "$PENDIENTES" -gt 0 ] || error "no se pudieron instalar las dependencias (mira el mensaje de xbps arriba)."
            echo
            aviso "tu sistema tiene $PENDIENTES actualizaciones pendientes y Void no deja instalar paquetes nuevos"
            echo "      hasta ponerlo al día (por eso salen errores como «unresolvable shlib»)."
            echo "      Este instalador NO actualiza el sistema por ti, porque eso puede cambiar el núcleo y"
            echo "      recompilar drivers, y es mejor que lo hagas tú con calma:"
            NUCLEO=$(xbps-install -nu 2>/dev/null | awk '{print $1}' | grep -E '^linux[0-9.]*-[0-9]' | tr '\n' ' ')
            DKMS=$(xbps-query -l 2>/dev/null | awk '{print $2}' | grep -E -- '-dkms-[0-9]' | tr '\n' ' ')
            [ -n "$NUCLEO" ] && echo "        · Incluye un NÚCLEO NUEVO: $NUCLEO"
            [ -n "$DKMS" ] && echo "        · Tienes drivers DKMS que se recompilarán para él: $DKMS" \
                           && echo "          Si alguno falla al compilar, NO reinicies: arréglalo antes (o arranca luego el núcleo"\
                           && echo "          anterior desde GRUB → «Advanced options for Void»)."
            echo
            echo "      1. Actualiza:   $ROOT xbps-install -Su   (lee bien lo que muestra antes de aceptar)"
            echo "      2. Reinicia y comprueba que todo arranca bien."
            echo "      3. Vuelve a ejecutar:   ./install.sh"
            exit 1
        fi
    fi
fi
}

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
        # «De confianza»: metadata::trusted (Caja, Nautilus) y metadata::trust (pcmanfm-qt / libfm); necesita gvfs.
        if command -v gio >/dev/null 2>&1; then
            gio set "$CARPETA_ESC/nekowall.desktop" metadata::trusted true 2>/dev/null || true
            gio set "$CARPETA_ESC/nekowall.desktop" metadata::trust true 2>/dev/null || true
        fi
    fi
fi

# ── Otros gestores de fondos (se pelearían con NekoWall por el fondo) ──────────
# Waywallen, swww/awww, wpaperd, hyprpaper, mpvpaper… arrancados por el usuario: se ofrece desactivarlos
# (su línea queda comentada con una marca, así se puede volver atrás) y cerrarlos.
if [ "$ESCRITORIO" = wlroots ] || [ "$ESCRITORIO" = niri ]; then
    OTROS="waywallen swww-daemon wpaperd hyprpaper"
    MARCA="# nekowall (desactivado: otro gestor de fondos): "
    INI="${XDG_CONFIG_HOME:-$HOME/.config}/wayfire.ini"
    EN_INI=""
    [ -f "$INI" ] && EN_INI=$(grep -n -i -E "^[^#]*=.*($(echo $OTROS | tr ' ' '|'))" "$INI" | grep -v "_wlr-restore\|nekowall" || true)
    AUTO_XDG=$(grep -l -i -E "^Exec=.*($(echo $OTROS | tr ' ' '|'))" "${XDG_CONFIG_HOME:-$HOME/.config}"/autostart/*.desktop 2>/dev/null | xargs -r grep -L "^Hidden=true" 2>/dev/null || true)
    EN_MARCHA=""
    # Waywallen suele ser una AppImage: se busca en la línea de órdenes, sin distinguir mayúsculas.
    pgrep -i -f waywallen >/dev/null 2>&1 && EN_MARCHA=" waywallen"
    for o in swww-daemon wpaperd hyprpaper; do pgrep -x "$o" >/dev/null 2>&1 && EN_MARCHA="$EN_MARCHA $o"; done
    if [ -n "$EN_INI$AUTO_XDG$EN_MARCHA" ]; then
        aviso "hay otro gestor de fondos que se pelearía con NekoWall:"
        [ -n "$EN_INI" ] && printf '      %s: %s\n' "$INI" "$EN_INI"
        for f in $AUTO_XDG; do echo "      $f"; done
        [ -n "$EN_MARCHA" ] && echo "      en marcha:$EN_MARCHA"
        if preguntar "¿Desactivarlo? (su línea de arranque queda comentada, se puede volver a activar)"; then
            if [ -n "$EN_INI" ] && [ "$SIMULAR" = 0 ]; then
                cp "$INI" "$INI.antes-de-nekowall"
                for n in $(printf '%s\n' "$EN_INI" | cut -d: -f1); do
                    sed -i "${n}s|^|$MARCA|" "$INI"
                done
            fi
            for f in $AUTO_XDG; do
                if [ "$SIMULAR" = 1 ]; then echo "   [simulado] Hidden=true en $f"; else printf 'Hidden=true\n' >> "$f"; fi
            done
            for o in $EN_MARCHA; do
                if [ "$o" = waywallen ]; then hacer pkill -i -f waywallen || true; else hacer pkill -x "$o" || true; fi
            done
            paso "Desactivado (copia de wayfire.ini en $INI.antes-de-nekowall)."
        fi
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
elif [ "$ESCRITORIO" = wlroots ]; then
    # Arranque al iniciar sesión (autostart XDG y, en Wayfire, wayfire.ini) y atajo Meta+Alt+W en Wayfire.
    hacer "$BIN/nekowall" wlr-shortcut --gui "$BIN/nekowall-gui"
    ATAJOS="En Wayfire, Meta+Alt+W abre el selector. En sway/labwc/Hyprland añade el atajo a mano (docs/DETALLES.md)."
elif [ "$ESCRITORIO" = niri ]; then
    # ~/.config/niri/nekowall.kdl: atajo Meta+Alt+W y restaurar el fondo de vídeo al iniciar sesión.
    hacer "$BIN/nekowall" niri-shortcut --gui "$BIN/nekowall-gui"
    ATAJOS="Meta+Alt+W abre el selector (niri ya lo está usando)."
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
