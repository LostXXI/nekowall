// Placa «esqueumórfica» (inspirada en el tema SkeuoCord de Marda33): pieza de plástico brillante
// con degradado de dos tonos, bisel y sombra. Se usa para teclas, indicadores y etiquetas del HUD.
//   lit: true  → encendida (color de acento, p. ej. carrusel activo o «Actual»)
import QtQuick
import QtQuick.Effects

Item {
    id: plate

    property bool lit: false
    property color base: "#3a3a3a"          // cuerpo apagado (≈ toolbar oscuro de SkeuoCord: 88 → 29)
    property color litColor: "#39C5BB"      // cuerpo encendido
    property real radius: 6
    default property alias content: inner.data

    readonly property color body: lit ? litColor : base
    // Color de texto y sombra adecuados al cuerpo (texto claro sobre gris, oscuro sobre turquesa).
    readonly property color textColor: lit ? "#04201d" : "#f2f4f6"
    readonly property color textShadow: lit ? Qt.rgba(1, 1, 1, 0.45) : Qt.rgba(0, 0, 0, 0.7)

    RectangularShadow {                      // sombra exterior: la pieza «flota» 1 px
        anchors.fill: shell
        radius: plate.radius
        offset.y: 1
        blur: 3
        color: Qt.rgba(0, 0, 0, 0.65)
    }
    Rectangle {
        id: shell
        anchors.fill: parent
        radius: plate.radius
        border.width: 1
        border.color: plate.lit ? Qt.darker(plate.litColor, 2.2) : "#727272"
        gradient: Gradient {                 // brillo de dos tonos: mitad superior clara, inferior oscura
            GradientStop { position: 0.0; color: Qt.lighter(plate.body, plate.lit ? 1.35 : 1.55) }
            GradientStop { position: 0.49; color: plate.body }
            GradientStop { position: 0.5; color: Qt.darker(plate.body, 1.3) }
            GradientStop { position: 1.0; color: Qt.darker(plate.body, plate.lit ? 1.7 : 2.6) }
        }
        Behavior on border.color { ColorAnimation { duration: 150 } }
    }
    Rectangle {                              // bisel interior (inset 1px negro al 60 %)
        anchors.fill: shell
        anchors.margins: 1
        radius: Math.max(0, plate.radius - 1)
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(0, 0, 0, 0.6)
    }
    Rectangle {                              // filo de luz en el borde superior
        anchors { left: shell.left; right: shell.right; top: shell.top
                  leftMargin: Math.max(3, plate.radius * 0.6); rightMargin: Math.max(3, plate.radius * 0.6); topMargin: 2 }
        height: 1
        color: Qt.rgba(1, 1, 1, plate.lit ? 0.6 : 0.3)
    }
    Item { id: inner; anchors.fill: parent }
}
