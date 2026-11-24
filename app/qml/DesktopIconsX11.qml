// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Iconos del escritorio de NekoWall en MATE (X11): ventana transparente (Marco compone) de tipo escritorio
// que cubre toda la pantalla, ENCIMA del vídeo de fondo (xwinwrap) y debajo de las ventanas. Imita al
// escritorio de Caja (DesktopIconsContent.qml). main.cpp le da el tipo y la mantiene encima del vídeo.
import QtQuick

Window {
    id: win
    color: "transparent"
    title: "NekoWall · escritorio"
    flags: Qt.FramelessWindowHint
    required property var dm

    DesktopIconsContent { anchors.fill: parent; dm: win.dm }
}
