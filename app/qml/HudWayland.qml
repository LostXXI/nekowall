// HUD en Wayland: el mismo Hud.qml más layer-shell (capa «overlay» anclada a los cuatro bordes y
// teclado en exclusiva). Se carga solo si el módulo org.kde.layershell existe (paquete layer-shell-qt);
// en X11 o sin ese módulo se usa Hud.qml directamente (ver main.cpp).
import QtQuick
import org.kde.layershell 1.0 as LayerShell

Hud {
    LayerShell.Window.layer: LayerShell.Window.LayerOverlay
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorBottom
                               | LayerShell.Window.AnchorLeft | LayerShell.Window.AnchorRight
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityExclusive
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.scope: "nekowall-hud"
    // Varias pantallas: que KWin la abra en la pantalla activa (la del ratón/foco), no siempre en la principal.
    LayerShell.Window.screenConfiguration: LayerShell.Window.ScreenFromCompositor
}
