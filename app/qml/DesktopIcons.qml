// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Iconos del escritorio de NekoWall en Wayland (nekowall-gui --desktop): una capa transparente de layer-shell
// en la capa «bottom», ENCIMA del vídeo de fondo (mpvpaper en «background») y debajo de las ventanas.
// Imita al escritorio de pcmanfm-qt (ver DesktopIconsContent.qml).
import QtQuick
import org.kde.layershell 1.0 as LayerShell

Window {
    id: win
    color: "transparent"
    title: "NekoWall · escritorio"
    required property var dm             // un DesktopModel por pantalla (lo crea main.cpp)

    LayerShell.Window.layer: LayerShell.Window.LayerBottom
    LayerShell.Window.anchors: LayerShell.Window.AnchorTop | LayerShell.Window.AnchorBottom
                               | LayerShell.Window.AnchorLeft | LayerShell.Window.AnchorRight
    LayerShell.Window.exclusionZone: -1
    LayerShell.Window.keyboardInteractivity: LayerShell.Window.KeyboardInteractivityOnDemand
    LayerShell.Window.scope: "nekowall-desktop"

    DesktopIconsContent { anchors.fill: parent; dm: win.dm }
}
