// «Pegar enlace»: descargar un fondo desde un enlace (YouTube, Reddit, una página con vídeo, un archivo
// directo…). Vista previa FIJA a la izquierda; los formatos, de menor a mayor, en un carrusel a la
// derecha que se desplaza por su cuenta. Se descarga sin audio a nekowallpapers.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: dlg

    property string analyzed: ""      // enlace analizado
    property var info: null           // respuesta de link-info
    property bool analyzing: false
    property string error: ""
    property int choice: -1           // índice en info.formats
    property bool playing: false
    property string previewError: ""
    property string previewSource: ""   // lo que reproduce la vista previa (enlace o archivo generado)
    property bool makingPreview: false
    property bool triedGenerated: false
    // Códecs que el reproductor de Qt no abre sin decodificación por hardware.
    function heavyCodec(c) { return c === "AV1" || c === "H.265" }

    readonly property var formats: info ? info.formats : []
    readonly property int screenH: Math.max(1, Bridge.fitHeight())

    function openWith(text) {
        urlField.text = text || Bridge.clipboardUrl()
        info = null
        error = ""
        playing = false
        open()
        if (urlField.text !== "") analyze()
        else urlField.forceActiveFocus()
    }
    function analyze() {
        const u = urlField.text.trim()
        if (u === "") return
        analyzed = u
        info = null
        error = ""
        playing = false
        previewError = ""
        previewSource = ""
        makingPreview = false
        triedGenerated = false
        analyzing = true
        Bridge.linkInfo(u)
    }
    // Recomendado: el más grande que no supera tu pantalla, evitando AV1/H.265 si hay otro
    // (si no hay ninguno que quepa, el más pequeño).
    function recommended() {
        let best = -1
        for (let i = 0; i < formats.length; ++i)
            if (formats[i].height <= screenH + 16 && !heavyCodec(formats[i].codec)) best = i
        if (best < 0)
            for (let i = 0; i < formats.length; ++i)
                if (formats[i].height <= screenH + 16) best = i
        return best >= 0 ? best : 0
    }
    // ▶: el enlace tal cual, o una vista previa en H.264 hecha por NekoWall si el códec no se puede
    // reproducir aquí (o si el reproductor falló).
    function startPreview() {
        previewError = ""
        if (!info || !info.preview) return
        if (heavyCodec(info.preview_codec) && !triedGenerated) { generatePreview(); return }
        previewSource = info.preview
        playing = true
    }
    function generatePreview() {
        triedGenerated = true
        makingPreview = true
        playing = false
        Bridge.linkPreview(info.preview)
    }
    function sizeText(f) {
        if (!f.size) return ""
        return (f.approx ? "~" : "") + Bridge.formatSize(f.size)
    }
    function download(applyAfter) {
        const f = formats[choice]
        const url = info.direct_url ? info.direct_url : analyzed
        Bridge.linkDownload(url, f.id, f.convert_to, f.reencode === true, info.title || "fondo", applyAfter)
        close()
    }
    function durationText(s) {
        if (!s) return ""
        s = Math.round(s)
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0")
    }

    Connections {
        target: Bridge
        function onLinkInfoReady(url, data) {
            if (url !== dlg.analyzed) return
            dlg.analyzing = false
            dlg.info = data
            dlg.choice = dlg.recommended()
        }
        function onLinkPreviewReady(url, path) {
            if (!dlg.info || url !== dlg.info.preview) return
            dlg.makingPreview = false
            dlg.previewSource = "file://" + path
            dlg.playing = true
        }
        function onLinkPreviewFailed(url, message) {
            if (!dlg.info || url !== dlg.info.preview) return
            dlg.makingPreview = false
            dlg.previewError = "No se pudo preparar la vista previa: " + message
        }
        function onLinkInfoFailed(url, message) {
            if (url !== dlg.analyzed) return
            dlg.analyzing = false
            dlg.error = message
        }
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: parent ? Math.min(parent.width - 40, 1200) : 900
    height: parent ? Math.min(parent.height - 40, 760) : 600
    modal: true
    padding: 16
    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.6) }
    onClosed: playing = false

    contentItem: ColumnLayout {
        spacing: 12

        Label { text: "Descargar desde un enlace"; font.pixelSize: 18; font.weight: Font.DemiBold }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            TextField {
                id: urlField
                Layout.fillWidth: true
                placeholderText: "Pega aquí el enlace: YouTube, Reddit, una página con vídeo o el enlace directo a un vídeo…"
                selectByMouse: true
                onAccepted: dlg.analyze()
            }
            Button {
                text: "Pegar"
                icon.name: "edit-paste"
                onClicked: { const u = Bridge.clipboardUrl(); if (u !== "") { urlField.text = u; dlg.analyze() } else urlField.paste() }
            }
            Button {
                text: "Analizar"
                icon.name: "search"
                highlighted: true
                enabled: !dlg.analyzing && urlField.text.trim() !== ""
                onClicked: dlg.analyze()
            }
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
                    width: Math.min(area.width, area.height * 16 / 9)
                    height: width * 9 / 16
                    anchors.centerIn: parent
                    color: "black"
                    radius: 6
                    clip: true

                    Image {
                        anchors.fill: parent
                        source: dlg.info && dlg.info.thumbnail ? dlg.info.thumbnail : ""
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        visible: !videoLoader.item || !videoLoader.item.ready
                    }
                    Loader {
                        id: videoLoader
                        anchors.fill: parent
                        active: dlg.playing && dlg.previewSource !== ""
                        source: "LinkPreview.qml"
                        onStatusChanged: if (status === Loader.Error) {
                            dlg.playing = false
                            dlg.previewError = "La vista previa de vídeo necesita qt6-multimedia; se muestra la miniatura."
                        }
                        onLoaded: {
                            item.source = dlg.previewSource
                            item.failed.connect(msg => {
                                dlg.playing = false
                                // Si falló el enlace tal cual, se intenta una vez con la versión H.264.
                                if (!dlg.triedGenerated) dlg.generatePreview()
                                else dlg.previewError = "No se pudo reproducir la vista previa."
                            })
                        }
                    }
                    BusyIndicator {
                        anchors.centerIn: parent
                        running: dlg.analyzing || dlg.makingPreview || (dlg.playing && (!videoLoader.item || !videoLoader.item.ready))
                        visible: running
                    }
                    // Mensajes cuando no hay nada que ver
                    Label {
                        anchors.centerIn: parent
                        width: parent.width - 60
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        color: dlg.error !== "" ? "#ff8a80" : "white"
                        visible: !dlg.analyzing && !dlg.info
                        text: dlg.error !== "" ? dlg.error
                              : "Pega un enlace y pulsa «Analizar» para ver la vista previa y los formatos."
                    }
                    Button {
                        anchors.left: parent.left
                        anchors.bottom: parent.bottom
                        anchors.margins: 12
                        visible: !!(dlg.info && dlg.info.preview)
                        enabled: !dlg.makingPreview
                        text: dlg.makingPreview ? "Preparando la vista previa…" : dlg.playing ? "⏸ Detener" : "▶ Reproducir vista previa"
                        onClicked: dlg.playing ? (dlg.playing = false) : dlg.startPreview()
                    }
                }
                Label {
                    anchors.top: frame.bottom
                    anchors.topMargin: 6
                    anchors.horizontalCenter: frame.horizontalCenter
                    width: frame.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: 12
                    opacity: 0.7
                    text: dlg.previewError !== "" ? dlg.previewError
                          : dlg.info && dlg.heavyCodec(dlg.info.preview_codec)
                            ? "Este vídeo es " + dlg.info.preview_codec + ": la vista previa es una copia corta en H.264 que prepara NekoWall."
                          : dlg.info && dlg.info.preview ? "La vista previa se reproduce en baja calidad y sin descargar nada." : ""
                }
            }

            // ── Formatos (carrusel que se desplaza) ─────────────────────────────
            ScrollView {
                id: rail
                Layout.preferredWidth: 340
                Layout.fillHeight: true
                clip: true
                contentWidth: availableWidth
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                ColumnLayout {
                    width: rail.availableWidth
                    spacing: 8

                    Label {
                        Layout.fillWidth: true
                        visible: !!dlg.info
                        text: dlg.info ? dlg.info.title : ""
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                    }
                    Label {
                        visible: !!(dlg.info && dlg.info.duration)
                        text: dlg.info ? "Duración " + dlg.durationText(dlg.info.duration) : ""
                        opacity: 0.75
                        font.pixelSize: 12
                    }
                    Label {
                        visible: dlg.formats.length > 0
                        text: "Formato (de menor a mayor)"
                        font.weight: Font.DemiBold
                        Layout.topMargin: 6
                    }

                    Repeater {
                        model: dlg.formats
                        delegate: ItemDelegate {
                            id: card
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            highlighted: dlg.choice === index
                            onClicked: dlg.choice = index
                            contentItem: RowLayout {
                                spacing: 10
                                RadioButton {
                                    checked: dlg.choice === card.index
                                    onClicked: dlg.choice = card.index
                                    focusPolicy: Qt.NoFocus
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Label {
                                        Layout.fillWidth: true
                                        font.weight: Font.DemiBold
                                        text: card.modelData.codec === "imagen"
                                              ? "Imagen " + card.modelData.width + "×" + card.modelData.height
                                              : card.modelData.height + "p"
                                                + (card.modelData.fps ? " · " + card.modelData.fps + " fps" : "")
                                                + " · " + card.modelData.codec + (card.modelData.reencode ? " (convertido)" : "")
                                                + (dlg.sizeText(card.modelData) ? " · " + dlg.sizeText(card.modelData) : "")
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        wrapMode: Text.Wrap
                                        font.pixelSize: 12
                                        opacity: 0.75
                                        visible: text !== ""
                                        text: {
                                            const f = card.modelData
                                            let t = []
                                            if (index === dlg.recommended()) t.push("✓ el adecuado para tu pantalla")
                                            else if (f.height > dlg.screenH + 16) t.push("más grande que tu pantalla: gasta más batería")
                                            if (f.reencode) t.push("pasado a H.264 por NekoWall: se reproduce bien en casi cualquier equipo")
                                            else if (f.convert_to > 0) t.push("lo crea NekoWall convirtiendo el vídeo")
                                            if (dlg.heavyCodec(f.codec)) t.push(f.codec + ": sin decodificación por hardware gasta mucha CPU y puede no reproducirse como fondo en KDE")
                                            return t.join(" · ")
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.topMargin: 10
                        visible: dlg.formats.length > 0
                        wrapMode: Text.Wrap
                        font.pixelSize: 12
                        opacity: 0.7
                        text: "Se descarga sin audio (un fondo no lo necesita y pesa menos) a la carpeta nekowallpapers, "
                              + "y aparece sola en tu biblioteca."
                              + "\n\nMoeWalls: su botón «Download» no se puede usar desde aquí. Descarga el vídeo en su web "
                              + "y arrástralo a NekoWall, o pega aquí el enlace del archivo."
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: "Solo descarga contenido que tengas permiso para usar."
                opacity: 0.6
                font.pixelSize: 12
                elide: Text.ElideRight
            }
            Button { text: "Cancelar"; onClicked: dlg.close() }
            Button {
                text: "Descargar"
                icon.name: "download"
                enabled: dlg.choice >= 0 && dlg.formats.length > 0 && !Bridge.taskActive
                onClicked: dlg.download(false)
            }
            Button {
                text: "Descargar y aplicar"
                icon.name: "dialog-ok-apply"
                highlighted: true
                enabled: dlg.choice >= 0 && dlg.formats.length > 0 && !Bridge.taskActive
                onClicked: dlg.download(true)
            }
        }
    }
}
