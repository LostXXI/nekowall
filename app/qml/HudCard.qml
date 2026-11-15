// Tarjeta del carrusel: paralelogramo con esquinas redondeadas.
// La escala, la profundidad y la opacidad las da la posición en el PathView.
import QtQuick
import QtQuick.Effects

Item {
    id: card

    required property var modelData
    required property int index

    property real cardW: 380
    property real cardH: 340
    property real skew: 0.22          // inclinación horizontal (0 = rectángulo)
    property color accent: "#3daee9"
    property bool isCurrentWallpaper: false
    property bool carouselActive: true  // el carrusel al que pertenece es el que responde al teclado

    signal activated()                // clic en la tarjeta ya seleccionada → aplicar
    signal picked()                   // clic en una tarjeta lateral → seleccionarla

    readonly property bool selected: PathView.isCurrentItem
    readonly property bool highlighted: selected && carouselActive
    // Las tarjetas laterales se oscurecen con un velo opaco en vez de hacerse transparentes,
    // así no se ven unas a través de otras.
    readonly property real shade: PathView.itemOpacity !== undefined ? 1 - PathView.itemOpacity : 0
    readonly property string thumb: modelData.thumb || ""

    width: cardW
    height: cardH
    z: PathView.itemZ !== undefined ? PathView.itemZ : 0
    scale: PathView.itemScale !== undefined ? PathView.itemScale : 1
    opacity: modelData.missing ? 0.35 : 1

    Accessible.role: Accessible.Button
    Accessible.name: modelData.name + (modelData.kind === "video" ? ", vídeo" : ", imagen")

    Item {
        id: body
        anchors.fill: parent
        // Cizalla horizontal alrededor del centro: x' = x + skew * (h/2 - y)
        transform: Matrix4x4 {
            matrix: Qt.matrix4x4(1, -card.skew, 0, card.skew * card.height / 2,
                                 0, 1, 0, 0,
                                 0, 0, 1, 0,
                                 0, 0, 0, 1)
        }

        RectangularShadow {              // halo de neón en la tarjeta activa
            anchors.fill: parent
            radius: 10
            blur: 38
            spread: 4
            color: card.accent
            opacity: card.highlighted ? 0.85 : 0
            Behavior on opacity { NumberAnimation { duration: 180 } }
        }
        Rectangle {                      // relleno mientras no hay miniatura
            anchors.fill: parent
            radius: 10
            color: "#1d2329"
            visible: img.status !== Image.Ready
            Text {
                anchors.centerIn: parent
                width: parent.width - 32
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: "#c8d0d8"
                font.pixelSize: 15
                text: card.modelData.missing ? "Archivo no encontrado"
                      : card.thumb === "" ? "Generando miniatura…" : "Cargando…"
            }
        }

        Image {
            id: img
            anchors.fill: parent
            source: card.thumb !== "" ? "file://" + card.thumb : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize.width: 420   // las tarjetas miden ≤ ~420 px: no decodificar más
            visible: false
        }
        Rectangle {
            id: mask
            anchors.fill: parent
            radius: 10
            visible: false
            layer.enabled: true
        }
        MultiEffect {
            anchors.fill: parent
            source: img
            visible: img.status === Image.Ready
            maskEnabled: true
            maskSource: mask
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }

        // Etiquetas: tipo, favorito y «actual»
        Row {
            anchors { left: parent.left; top: parent.top; margins: 12 }
            spacing: 6
            visible: card.selected
            Badge { text: card.modelData.kind === "video" ? "▶ Vídeo" : "Imagen" }
            Badge { text: "★ Favorito"; visible: card.modelData.favorite }
            Badge { text: "✓ Actual"; visible: card.isCurrentWallpaper; lit: true; litColor: card.accent }
        }

        Rectangle {                      // velo de profundidad (tarjetas laterales)
            anchors.fill: parent
            radius: 10
            color: "black"
            opacity: card.shade * 0.85
            visible: opacity > 0.01
        }
        Rectangle {                      // borde; el seleccionado del carrusel activo usa el acento
            anchors.fill: parent
            radius: 10
            color: "transparent"
            border.width: card.highlighted ? 3 : 1
            border.color: card.highlighted ? card.accent : Qt.rgba(1, 1, 1, 0.18)
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: card.selected ? card.activated() : card.picked()
        }
    }

    component Badge: Skeuo {                 // etiqueta esqueumórfica (ver Skeuo.qml)
        id: badge
        property alias text: label.text
        base: "#2c2c2c"
        height: label.implicitHeight + 8
        width: label.implicitWidth + 18
        radius: height / 2
        Text {
            id: label
            anchors.centerIn: parent
            color: badge.textColor
            style: Text.Raised
            styleColor: badge.textShadow
            font.pixelSize: 13
            font.weight: Font.Bold
        }
    }
}
