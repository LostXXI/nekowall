// Filtro en forma de píldora: «Animados (14)». Marcado = color de acento.
import QtQuick
import QtQuick.Controls

AbstractButton {
    id: chip
    property int count: -1
    checkable: true
    autoExclusive: true
    focusPolicy: Qt.StrongFocus
    implicitHeight: 32
    implicitWidth: label.implicitWidth + 28

    Accessible.name: text + (count >= 0 ? ", " + count : "")

    background: Rectangle {
        radius: height / 2
        color: chip.checked ? chip.palette.highlight
             : chip.hovered ? Qt.rgba(chip.palette.text.r, chip.palette.text.g, chip.palette.text.b, 0.14)
             : Qt.rgba(chip.palette.text.r, chip.palette.text.g, chip.palette.text.b, 0.08)
        border.width: chip.visualFocus ? 2 : 0
        border.color: chip.palette.highlight
    }
    contentItem: Text {
        id: label
        text: chip.text + (chip.count >= 0 ? "  (" + chip.count + ")" : "")
        color: chip.checked ? chip.palette.highlightedText : chip.palette.text
        font.pixelSize: 14
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}
