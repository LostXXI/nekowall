// Tarjeta de la rejilla: miniatura 16:9, nombre, tipo/tamaño, «Aplicar» y favorito.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Item {
    id: tile

    required property var modelData
    required property int index
    property bool selected: false
    property bool isCurrentWallpaper: false
    property bool selecting: false   // modo «Seleccionar»: clic = marcar/desmarcar
    property bool picked: false

    signal applyRequested()
    signal favoriteToggled()
    signal clicked()
    signal contextMenuRequested(real x, real y)

    readonly property string thumbUrl: modelData.thumb
        ? "file://" + modelData.thumb + "?v=" + Bridge.thumbVersion : ""

    Rectangle {                              // fondo de selección / hover
        anchors.fill: parent
        anchors.margins: 4
        radius: 10
        color: tile.picked ? Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.28)
             : tile.selected ? Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.18)
             : hover.hovered ? Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.06)
             : "transparent"
        border.width: tile.selected || tile.picked ? 2 : 0
        border.color: palette.highlight
    }

    HoverHandler { id: hover }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: (m) => {
            tile.clicked()
            if (m.button === Qt.RightButton) tile.contextMenuRequested(m.x, m.y)
        }
        onDoubleClicked: (m) => { if (m.button === Qt.LeftButton && !tile.selecting) tile.applyRequested() }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 6

        Item {                               // miniatura con esquinas redondeadas
            Layout.fillWidth: true
            Layout.preferredHeight: width * 9 / 16

            Rectangle {
                anchors.fill: parent
                radius: 8
                color: Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.08)
                visible: img.status !== Image.Ready
                Label {
                    anchors.centerIn: parent
                    text: tile.modelData.missing ? "Archivo no encontrado"
                        : tile.thumbUrl === "" ? "Generando miniatura…" : ""
                    opacity: 0.7
                }
            }
            Image {
                id: img
                anchors.fill: parent
                source: tile.thumbUrl
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize.width: 480
                visible: false
            }
            Rectangle { id: mask; anchors.fill: parent; radius: 8; visible: false; layer.enabled: true }
            MultiEffect {
                anchors.fill: parent
                source: img
                visible: img.status === Image.Ready
                opacity: tile.modelData.missing ? 0.4 : 1
                maskEnabled: true
                maskSource: mask
                maskThresholdMin: 0.5
                maskSpreadAtMin: 1.0
            }
            Row {                            // etiquetas sobre la miniatura
                anchors { left: parent.left; top: parent.top; margins: 8 }
                spacing: 6
                Rectangle {
                    visible: tile.modelData.kind === "video"
                    radius: 4; height: 22; width: vlabel.implicitWidth + 12
                    color: Qt.rgba(0, 0, 0, 0.7)
                    Label { id: vlabel; anchors.centerIn: parent; text: "▶ Vídeo"; color: "white"; font.pixelSize: 12 }
                }
                Rectangle {
                    visible: tile.isCurrentWallpaper
                    radius: 4; height: 22; width: alabel.implicitWidth + 12
                    color: Qt.darker(palette.highlight, 1.8)
                    Label { id: alabel; anchors.centerIn: parent; text: "✓ Actual"; color: "white"; font.pixelSize: 12 }
                }
            }
            Rectangle {                      // casilla del modo «Seleccionar»
                visible: tile.selecting
                anchors { right: parent.right; top: parent.top; margins: 8 }
                width: 28; height: 28; radius: 6
                color: tile.picked ? palette.highlight : Qt.rgba(0, 0, 0, 0.55)
                border.width: 2
                border.color: tile.picked ? palette.highlight : "white"
                Label { anchors.centerIn: parent; text: "✓"; color: "white"; font.pixelSize: 17; font.bold: true; visible: tile.picked }
            }
        }

        Label {
            Layout.fillWidth: true
            text: tile.modelData.name
            elide: Text.ElideRight        // una sola línea: todas las tarjetas miden lo mismo
            font.pixelSize: 14
            ToolTip.visible: nameHover.hovered && truncated
            ToolTip.text: tile.modelData.name
            HoverHandler { id: nameHover }
        }
        Label {
            Layout.fillWidth: true
            text: (tile.modelData.kind === "video" ? "Vídeo" : "Imagen")
                  + (tile.modelData.size ? " · " + Bridge.formatSize(tile.modelData.size) : "")
            opacity: 0.7
            font.pixelSize: 12
        }
        // Acciones: «Aplicar» a la izquierda; «Ajustar» y favorito a la derecha, todos del mismo alto.
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 2
            spacing: 4
            Button {
                Layout.preferredWidth: Math.max(implicitWidth, 96)
                text: tile.isCurrentWallpaper ? "Aplicado" : "Aplicar"
                highlighted: !tile.isCurrentWallpaper && hover.hovered && enabled
                enabled: !tile.isCurrentWallpaper && !tile.modelData.missing && !Bridge.busy
                focusPolicy: Qt.NoFocus      // el teclado lo maneja la rejilla
                onClicked: tile.applyRequested()
            }
            Item { Layout.fillWidth: true }
            ToolButton {
                // Abre la ventana «Ajustar» (vista previa, mover, escala, relleno).
                visible: tile.modelData.kind === "static" && !tile.modelData.missing
                text: "Ajustar"
                Layout.preferredHeight: 34
                enabled: !Bridge.taskActive
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: "Encuadrar en tu pantalla (" + Bridge.fitSizeText() + ") con vista previa: mover, escala y relleno"
                Accessible.name: "Ajustar a mi pantalla"
                onClicked: Bridge.openAdjust(tile.modelData.path, tile.modelData.name)
            }
            ToolButton {
                text: tile.modelData.favorite ? "★" : "☆"
                font.pixelSize: 18
                Layout.preferredHeight: 34
                Layout.preferredWidth: 34
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: tile.modelData.favorite ? "Quitar de favoritos (F)" : "Marcar como favorito (F)"
                Accessible.name: ToolTip.text
                onClicked: tile.favoriteToggled()
            }
        }
        Item { Layout.fillHeight: true }
    }
}
