// Ventana «Ajustar»: encuadrar una imagen en la pantalla con vista previa.
// La vista previa (izquierda) queda fija; las opciones van a la derecha en un panel con su propia barra.
// Arrastrar = mover · rueda = zoom (sobre el punto del ratón) · flechas = mover · +/− = zoom.
// Todo se calcula en píxeles de la pantalla real: zoomX/zoomY = píxeles de pantalla por píxel de la
// imagen en horizontal y en vertical (distintos = estirada); (ox, oy) = esquina de la imagen en la pantalla.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Popup {
    id: dlg

    property string target: ""     // archivo de la biblioteca (puede ser una copia ya ajustada)
    property string source: ""     // original del que se parte siempre
    property string name: ""
    property int iw: 1
    property int ih: 1
    readonly property int sw: Math.max(1, Bridge.fitWidth())
    readonly property int sh: Math.max(1, Bridge.fitHeight())

    property real zoomX: 1
    property real zoomY: 1
    property bool keepRatio: true
    property real ox: 0
    property real oy: 0
    property string fill: "blur"   // blur | black

    readonly property real coverZoom: Math.max(sw / iw, sh / ih)
    readonly property real containZoom: Math.min(sw / iw, sh / ih)
    readonly property real minZoom: Math.min(containZoom * 0.5, 1)
    readonly property real maxZoom: Math.max(coverZoom * 4, 2)
    readonly property real shownW: iw * zoomX
    readonly property real shownH: ih * zoomY
    readonly property bool stretched: Math.abs(zoomX / zoomY - 1) > 0.005
    readonly property bool covers: ox <= 0.5 && oy <= 0.5 && ox + shownW >= sw - 0.5 && oy + shownH >= sh - 0.5

    function openFor(info) {
        target = info.target
        source = info.source
        name = info.name
        iw = Math.max(1, info.width)
        ih = Math.max(1, info.height)
        fill = "blur"
        keepRatio = true
        setCover()
        open()
        frameArea.forceActiveFocus()
    }
    function clampZoom(z) { return Math.max(minZoom, Math.min(maxZoom, z)) }
    // Agranda o achica ancho y alto a la vez (misma proporción), dejando fijo el punto (cx, cy) de la
    // pantalla (por defecto, el centro).
    function scaleBy(f, cx, cy) {
        f = Math.min(f, maxZoom / Math.max(zoomX, zoomY))
        f = Math.max(f, minZoom / Math.min(zoomX, zoomY))
        if (cx === undefined) { cx = sw / 2; cy = sh / 2 }
        ox = cx - (cx - ox) * f
        oy = cy - (cy - oy) * f
        zoomX *= f
        zoomY *= f
    }
    // Ancho (o alto) a un valor; con «Mantener proporción» el otro le sigue.
    function setZoomX(z) {
        z = clampZoom(z)
        if (keepRatio) { scaleBy(z / zoomX); return }
        ox = sw / 2 - (sw / 2 - ox) * z / zoomX
        zoomX = z
    }
    function setZoomY(z) {
        z = clampZoom(z)
        if (keepRatio) { scaleBy(z / zoomY); return }
        oy = sh / 2 - (sh / 2 - oy) * z / zoomY
        zoomY = z
    }
    function setBoth(z) { zoomX = z; zoomY = z; center() }
    function center() { ox = (sw - shownW) / 2; oy = (sh - shownH) / 2 }
    function setCover() { setBoth(coverZoom) }
    function setContain() { setBoth(containZoom) }
    function setOriginal() { setBoth(1) }
    // Estirar: la imagen entera ocupa justo la pantalla (se deforma si las proporciones no coinciden).
    function setStretch() { keepRatio = false; zoomX = sw / iw; zoomY = sh / ih; ox = 0; oy = 0 }
    function save(applyNow) {
        Bridge.adjustImage(target, name, zoomX, zoomY, ox, oy, fill, applyNow)
        close()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: parent ? parent.width - 40 : 900
    height: parent ? parent.height - 40 : 600
    modal: true
    padding: 16
    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.6) }

    contentItem: ColumnLayout {
        spacing: 12

        Label {
            Layout.fillWidth: true
            text: "Ajustar fondo · " + dlg.name
            font.pixelSize: 18
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 16

            // ── Vista previa (fija) ─────────────────────────────────────────────
            Item {
                id: area
                Layout.fillWidth: true
                Layout.fillHeight: true

                Rectangle {
                    id: frame
                    // Misma forma que la pantalla, lo más grande posible.
                    width: Math.min(area.width, area.height * dlg.sw / dlg.sh)
                    height: width * dlg.sh / dlg.sw
                    anchors.centerIn: parent
                    color: "black"
                    clip: true
                    readonly property real k: width / dlg.sw   // píxeles de la vista por píxel de pantalla

                    // Relleno desenfocado (solo se ve donde la imagen no llega).
                    Image {
                        id: bgImg
                        anchors.fill: parent
                        source: dlg.source ? "file://" + dlg.source : ""
                        fillMode: Image.PreserveAspectCrop
                        sourceSize.width: 480
                        asynchronous: true
                        visible: false
                    }
                    MultiEffect {
                        anchors.fill: parent
                        source: bgImg
                        visible: dlg.fill === "blur" && bgImg.status === Image.Ready
                        blurEnabled: true
                        blur: 1.0
                        blurMax: 64
                        brightness: -0.06
                    }

                    Image {
                        id: img
                        x: dlg.ox * frame.k
                        y: dlg.oy * frame.k
                        width: dlg.shownW * frame.k
                        height: dlg.shownH * frame.k
                        source: dlg.source ? "file://" + dlg.source : ""
                        sourceSize.width: Math.min(dlg.iw, 2560)
                        asynchronous: true
                        smooth: true
                        mipmap: true
                    }
                    BusyIndicator {
                        anchors.centerIn: parent
                        running: img.status === Image.Loading
                        visible: running
                    }

                    MouseArea {
                        id: frameArea
                        anchors.fill: parent
                        focus: true
                        cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                        property real px: 0
                        property real py: 0
                        property real sx: 0
                        property real sy: 0
                        onPressed: (m) => { px = m.x; py = m.y; sx = dlg.ox; sy = dlg.oy }
                        onPositionChanged: (m) => {
                            if (!pressed) return
                            dlg.ox = sx + (m.x - px) / frame.k
                            dlg.oy = sy + (m.y - py) / frame.k
                        }
                        onWheel: (w) => dlg.scaleBy(Math.pow(1.0015, w.angleDelta.y), w.x / frame.k, w.y / frame.k)
                        Keys.onLeftPressed: dlg.ox -= 10
                        Keys.onRightPressed: dlg.ox += 10
                        Keys.onUpPressed: dlg.oy -= 10
                        Keys.onDownPressed: dlg.oy += 10
                        Keys.onPressed: (e) => {
                            if (e.key === Qt.Key_Plus || e.key === Qt.Key_Equal) { dlg.scaleBy(1.1); e.accepted = true }
                            else if (e.key === Qt.Key_Minus) { dlg.scaleBy(1 / 1.1); e.accepted = true }
                        }
                    }
                }
                // Borde de la pantalla
                Rectangle {
                    anchors.fill: frame
                    color: "transparent"
                    border.width: 2
                    border.color: dlg.palette.highlight
                }
                Label {
                    anchors.top: frame.bottom
                    anchors.topMargin: 6
                    anchors.horizontalCenter: frame.horizontalCenter
                    text: "Tu pantalla · " + dlg.sw + "×" + dlg.sh + "   (arrastra para mover · rueda para acercar)"
                    opacity: 0.7
                    font.pixelSize: 12
                }
            }

            // ── Opciones (panel con su propia barra) ────────────────────────────
            ScrollView {
                id: rail
                Layout.preferredWidth: 300
                Layout.fillHeight: true
                clip: true
                contentWidth: availableWidth
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                ColumnLayout {
                    width: rail.availableWidth
                    spacing: 10

                    Label { text: "Encajar"; font.weight: Font.DemiBold }
                    GridLayout {
                        Layout.fillWidth: true
                        columns: 2
                        columnSpacing: 6
                        rowSpacing: 6
                        Button {
                            Layout.fillWidth: true; text: "Rellenar"; onClicked: dlg.setCover()
                            ToolTip.visible: hovered; ToolTip.text: "Cubre toda la pantalla (lo que hace el escritorio)"
                        }
                        Button {
                            Layout.fillWidth: true; text: "Entera"; onClicked: dlg.setContain()
                            ToolTip.visible: hovered; ToolTip.text: "Se ve la imagen completa; el resto se rellena"
                        }
                        Button {
                            Layout.fillWidth: true; text: "Original"; onClicked: dlg.setOriginal()
                            ToolTip.visible: hovered; ToolTip.text: "Tamaño real: un píxel de la imagen = un píxel de la pantalla"
                        }
                        Button { Layout.fillWidth: true; text: "Centrar"; onClicked: dlg.center() }
                        Button {
                            Layout.fillWidth: true; text: "Estirar"; onClicked: dlg.setStretch()
                            ToolTip.visible: hovered; ToolTip.text: "La imagen entera ocupa justo la pantalla (puede deformarse)"
                        }
                    }

                    Label { text: "Tamaño"; font.weight: Font.DemiBold; Layout.topMargin: 6 }
                    CheckBox {
                        text: "Mantener proporción"
                        checked: dlg.keepRatio
                        onToggled: {
                            dlg.keepRatio = checked
                            // Al volver a activarla, el alto se iguala al ancho (sin deformar).
                            if (checked && dlg.stretched) dlg.setZoomY(dlg.zoomX)
                        }
                        ToolTip.visible: hovered
                        ToolTip.text: "Desmárcala para cambiar el ancho y el alto por separado (la imagen se estira)"
                    }
                    Label { text: "Ancho · " + Math.round(dlg.zoomX * 100) + " %"; font.pixelSize: 12; opacity: 0.75 }
                    RowLayout {
                        Layout.fillWidth: true
                        ToolButton { text: "−"; onClicked: dlg.setZoomX(dlg.zoomX / 1.05) }
                        Slider {
                            Layout.fillWidth: true
                            from: 0; to: 1
                            // Escala logarítmica: el mismo recorrido para agrandar que para achicar.
                            value: Math.log(dlg.zoomX / dlg.minZoom) / Math.log(dlg.maxZoom / dlg.minZoom)
                            onMoved: dlg.setZoomX(dlg.minZoom * Math.pow(dlg.maxZoom / dlg.minZoom, value))
                        }
                        ToolButton { text: "+"; onClicked: dlg.setZoomX(dlg.zoomX * 1.05) }
                    }
                    Label { text: "Alto · " + Math.round(dlg.zoomY * 100) + " %"; font.pixelSize: 12; opacity: 0.75 }
                    RowLayout {
                        Layout.fillWidth: true
                        ToolButton { text: "−"; onClicked: dlg.setZoomY(dlg.zoomY / 1.05) }
                        Slider {
                            Layout.fillWidth: true
                            from: 0; to: 1
                            value: Math.log(dlg.zoomY / dlg.minZoom) / Math.log(dlg.maxZoom / dlg.minZoom)
                            onMoved: dlg.setZoomY(dlg.minZoom * Math.pow(dlg.maxZoom / dlg.minZoom, value))
                        }
                        ToolButton { text: "+"; onClicked: dlg.setZoomY(dlg.zoomY * 1.05) }
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        font.pixelSize: 12
                        opacity: 0.75
                        text: "En pantalla ocupa " + Math.round(dlg.shownW) + "×" + Math.round(dlg.shownH) + " píxeles"
                              + (dlg.stretched ? "\nLa imagen está estirada (ancho y alto distintos)." : "")
                              + (Math.max(dlg.zoomX, dlg.zoomY) > 1.01 ? "\nAgrandada: puede verse menos nítida." : "")
                    }

                    Label { text: "Posición"; font.weight: Font.DemiBold; Layout.topMargin: 6 }
                    Label { text: "Horizontal"; font.pixelSize: 12; opacity: 0.75 }
                    Slider {
                        Layout.fillWidth: true
                        // Siempre se puede mover: hasta media pantalla más allá de cada borde.
                        readonly property real a: Math.min(0, dlg.sw - dlg.shownW) - dlg.sw / 2
                        readonly property real b: Math.max(0, dlg.sw - dlg.shownW) + dlg.sw / 2
                        from: a; to: b
                        value: Math.max(a, Math.min(b, dlg.ox))
                        onMoved: dlg.ox = value
                    }
                    Label { text: "Vertical"; font.pixelSize: 12; opacity: 0.75 }
                    Slider {
                        Layout.fillWidth: true
                        readonly property real a: Math.min(0, dlg.sh - dlg.shownH) - dlg.sh / 2
                        readonly property real b: Math.max(0, dlg.sh - dlg.shownH) + dlg.sh / 2
                        from: a; to: b
                        value: Math.max(a, Math.min(b, dlg.oy))
                        onMoved: dlg.oy = value
                    }

                    Label { text: "Relleno"; font.weight: Font.DemiBold; Layout.topMargin: 6 }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        font.pixelSize: 12
                        opacity: 0.75
                        text: "Lo que se ve donde la imagen no llega"
                              + (dlg.covers ? " (ahora la imagen cubre toda la pantalla, así que no se nota)." : ":")
                    }
                    RadioButton {
                        text: "La misma imagen desenfocada"
                        checked: dlg.fill === "blur"
                        onClicked: dlg.fill = "blur"
                    }
                    RadioButton {
                        text: "Negro"
                        checked: dlg.fill === "black"
                        onClicked: dlg.fill = "black"
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 10
                        wrapMode: Text.Wrap
                        font.pixelSize: 12
                        opacity: 0.7
                        text: "Imagen: " + dlg.iw + "×" + dlg.ih + " · Pantalla: " + dlg.sw + "×" + dlg.sh
                              + "\n\nSe guarda una copia en JPG de máxima calidad junto al original, que no se modifica. "
                              + "Si vuelves a ajustarla, se parte otra vez del original."
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: "Flechas: mover · +/−: zoom · Esc: cancelar"
                opacity: 0.6
                font.pixelSize: 12
            }
            Button { text: "Cancelar"; onClicked: dlg.close() }
            Button {
                text: "Guardar"
                icon.name: "document-save"
                onClicked: dlg.save(false)
                ToolTip.visible: hovered
                ToolTip.text: "Guarda la copia ajustada en la biblioteca (si ya era tu fondo, se actualiza)"
            }
            Button {
                text: "Guardar y aplicar"
                icon.name: "dialog-ok-apply"
                highlighted: true
                enabled: !Bridge.busy
                onClicked: dlg.save(true)
            }
        }
    }
}
