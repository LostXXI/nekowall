// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Rueda del ratón más rápida (y suave) para una rejilla o lista. Se pone dentro del Flickable:
//     FastWheel { flick: grid }
// No toca los clics: solo atiende la rueda. Con touchpad usa el desplazamiento en píxeles.
import QtQuick

MouseArea {
    id: wheel
    required property Flickable flick
    property real speed: 2.5          // ×2,5 lo que Qt mueve por defecto

    anchors.fill: parent
    acceptedButtons: Qt.NoButton
    z: 1000

    property real targetY: 0

    function clampY(y) {
        const top = flick.originY
        const bottom = flick.originY + Math.max(0, flick.contentHeight - flick.height)
        return Math.max(top, Math.min(bottom, y))
    }

    onWheel: (w) => {
        // Si ya se estaba animando, se suma al destino (girar rápido la rueda acelera).
        const from = anim.running ? targetY : flick.contentY
        const d = w.pixelDelta.y !== 0 ? w.pixelDelta.y * 1.5 : w.angleDelta.y * speed
        targetY = clampY(from - d)
        anim.stop()
        anim.from = flick.contentY
        anim.to = targetY
        anim.start()
        w.accepted = true
    }

    NumberAnimation {
        id: anim
        target: wheel.flick
        property: "contentY"
        duration: 140
        easing.type: Easing.OutCubic
    }
}
