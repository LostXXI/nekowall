// Workshop: buscar y descargar fondos de Wallhaven. Las descargas van a «nekowallpapers» y aparecen
// solas en la biblioteca. Solo contenido apto (SFW). No se conecta a internet hasta abrir esta página.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Item {
    id: page

    property string sorting: "popular"     // popular | recent | toplist | random
    property bool catGeneral: true
    property bool catAnime: true
    property bool catPeople: true
    property bool forMyScreen: false
    property bool started: false

    signal notify(string text, bool isError)

    function categories() {
        return (catGeneral ? "1" : "0") + (catAnime ? "1" : "0") + (catPeople ? "1" : "0")
    }
    function doSearch() {
        started = true
        Workshop.search(search.text, sorting, categories(),
                        forMyScreen ? Bridge.screenWidth + "x" + Bridge.screenHeight : "")
        grid.positionViewAtBeginning()
    }
    function searchTag(name) {
        details.close()
        search.text = name
        doSearch()
    }
    function shortSize(bytes) { return Bridge.formatSize(bytes) }

    // La primera búsqueda se hace al abrir la página, no al abrir NekoWall.
    onVisibleChanged: if (visible && !started) doSearch()
    Component.onCompleted: if (visible && !started) doSearch()

    Binding { target: Workshop; property: "folder"; value: Bridge.defaultFolder }

    Connections {
        target: Workshop
        function onDownloadFinished(id, path, applyAfter) {
            if (applyAfter) Bridge.applyFile(path)
            else page.notify("Descargado en nekowallpapers: " + path.split("/").pop(), false)
        }
        function onDownloadFailed(id, message) { page.notify("No se pudo descargar: " + message, true) }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        // Aviso fijo: Workshop necesita internet (la biblioteca y el selector no).
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: notice.implicitHeight + 20
            radius: 8
            color: "#1e88ff"
            RowLayout {
                id: notice
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                spacing: 10
                Label { text: "🌐"; font.pixelSize: 18; color: "white" }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: "white"
                    font.weight: Font.DemiBold
                    text: "Workshop solo funciona si estás conectado a internet. Tus fondos descargados se quedan en tu equipo y funcionan sin conexión."
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            TextField {
                id: search
                Layout.fillWidth: true
                placeholderText: "Buscar en Wallhaven: miku, cyberpunk, paisaje…  (Enter)"
                Accessible.name: "Buscar en Wallhaven"
                onAccepted: page.doSearch()
                Keys.onEscapePressed: { text = ""; page.doSearch() }
            }
            Button {
                text: "Buscar"
                icon.name: "search"
                highlighted: true
                onClicked: page.doSearch()
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: 8
            FilterChip { text: "Populares"; checked: page.sorting === "popular"; onClicked: { page.sorting = "popular"; page.doSearch() } }
            FilterChip { text: "Recientes"; checked: page.sorting === "recent"; onClicked: { page.sorting = "recent"; page.doSearch() } }
            FilterChip { text: "Top del mes"; checked: page.sorting === "toplist"; onClicked: { page.sorting = "toplist"; page.doSearch() } }
            FilterChip { text: "Aleatorio"; checked: page.sorting === "random"; onClicked: { page.sorting = "random"; page.doSearch() } }
            Item { width: 16; height: 1 }
            FilterChip {
                text: "Anime"; autoExclusive: false; checked: page.catAnime
                onClicked: { page.catAnime = checked; page.doSearch() }
            }
            FilterChip {
                text: "General"; autoExclusive: false; checked: page.catGeneral
                onClicked: { page.catGeneral = checked; page.doSearch() }
            }
            FilterChip {
                text: "Personas"; autoExclusive: false; checked: page.catPeople
                onClicked: { page.catPeople = checked; page.doSearch() }
            }
            Item { width: 16; height: 1 }
            FilterChip {
                text: "Para mi pantalla (" + Bridge.screenWidth + "×" + Bridge.screenHeight + " o más)"
                autoExclusive: false; checked: page.forMyScreen
                onClicked: { page.forMyScreen = checked; page.doSearch() }
            }
        }

        Label {
            Layout.fillWidth: true
            opacity: 0.7
            font.pixelSize: 12
            text: Workshop.searched
                  ? Workshop.total.toLocaleString(Qt.locale("es_ES"), "f", 0) + " fondos en Wallhaven · solo contenido apto · se guardan en nekowallpapers"
                  : "Fondos de wallhaven.cc · solo contenido apto · se guardan en nekowallpapers"
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            readonly property int columns: Math.max(1, Math.floor(width / 280))
            cellWidth: Math.floor(width / columns)
            cellHeight: cellWidth * 10 / 16 + 56
            model: Workshop.model
            cacheBuffer: cellHeight * 3
            ScrollBar.vertical: ScrollBar { }
            // Al acercarse al final se pide la página siguiente.
            onContentYChanged: if (contentHeight > 0 && contentY + height > contentHeight - cellHeight * 2) Workshop.loadMore()

            delegate: Item {
                id: tile
                required property var wp
                required property int index
                width: grid.cellWidth
                height: grid.cellHeight
                readonly property bool have: Workshop.downloaded.indexOf(wp.id) >= 0
                readonly property var pct: Workshop.progress[wp.id]

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 4
                    radius: 10
                    color: tileHover.hovered ? Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.06) : "transparent"
                }
                HoverHandler { id: tileHover }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: details.openFor(tile.wp)
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 6

                    Item {
                        Layout.fillWidth: true
                        Layout.preferredHeight: width * 10 / 16
                        Rectangle {
                            anchors.fill: parent
                            radius: 8
                            color: tile.wp.colors.length > 0 ? tile.wp.colors[0]
                                 : Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.08)
                            opacity: 0.5
                            visible: thumb.status !== Image.Ready
                        }
                        Image {
                            id: thumb
                            anchors.fill: parent
                            source: tile.wp.thumb
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            visible: false
                        }
                        Rectangle { id: tmask; anchors.fill: parent; radius: 8; visible: false; layer.enabled: true }
                        MultiEffect {
                            anchors.fill: parent
                            source: thumb
                            visible: thumb.status === Image.Ready
                            maskEnabled: true
                            maskSource: tmask
                            maskThresholdMin: 0.5
                            maskSpreadAtMin: 1.0
                        }
                        Rectangle {
                            visible: tile.have
                            anchors { left: parent.left; top: parent.top; margins: 8 }
                            radius: 4; height: 22; width: haveLabel.implicitWidth + 12
                            color: Qt.darker(palette.highlight, 1.8)
                            Label { id: haveLabel; anchors.centerIn: parent; text: "✓ Descargado"; color: "white"; font.pixelSize: 12 }
                        }
                        ProgressBar {
                            visible: tile.pct !== undefined
                            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 8 }
                            from: 0; to: 100; value: tile.pct || 0
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 6
                        Label {
                            Layout.fillWidth: true
                            text: tile.wp.resolution + "  ·  " + page.shortSize(tile.wp.size)
                            font.pixelSize: 13
                            elide: Text.ElideRight
                        }
                        ToolButton {
                            icon.name: tile.have ? "dialog-ok-apply" : "download"
                            text: tile.have ? "Aplicar" : ""
                            display: tile.have ? AbstractButton.TextBesideIcon : AbstractButton.IconOnly
                            enabled: tile.pct === undefined && !Bridge.busy
                            focusPolicy: Qt.NoFocus
                            ToolTip.visible: hovered
                            ToolTip.text: tile.have ? "Aplicar este fondo" : "Descargar a nekowallpapers"
                            Accessible.name: ToolTip.text
                            onClicked: tile.have ? Bridge.applyFile(Workshop.fileFor(tile.wp.id))
                                                 : Workshop.download(tile.wp, false)
                        }
                    }
                }
            }

            footer: Item {
                width: grid.width
                height: Workshop.loading && Workshop.results.length > 0 ? 56 : 0
                BusyIndicator { anchors.centerIn: parent; running: parent.height > 0; visible: running }
            }

            // Estados: cargando, error, sin resultados
            Column {
                anchors.centerIn: parent
                spacing: 10
                visible: Workshop.results.length === 0
                width: Math.min(parent.width - 40, 480)
                BusyIndicator {
                    anchors.horizontalCenter: parent.horizontalCenter
                    running: Workshop.loading
                    visible: running
                }
                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: 16
                    visible: !Workshop.loading
                    text: Workshop.error !== "" ? Workshop.error
                        : Workshop.searched ? (search.text !== "" ? "Nada en Wallhaven para «" + search.text + "»"
                                                                  : "No hay fondos con estos filtros")
                        : ""
                }
                Button {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: Workshop.error !== "" && !Workshop.loading
                    text: "Reintentar"
                    icon.name: "view-refresh"
                    onClicked: page.doSearch()
                }
            }
        }

        // Error al cargar más páginas (con resultados ya en pantalla)
        RowLayout {
            Layout.fillWidth: true
            visible: Workshop.error !== "" && Workshop.results.length > 0
            Label { Layout.fillWidth: true; text: Workshop.error; color: "#e0575e"; wrapMode: Text.Wrap }
            Button { text: "Reintentar"; onClicked: Workshop.loadMore() }
        }
    }

    // Panel de detalle: imagen grande, datos, etiquetas y descarga.
    Popup {
        id: details
        property var wp: null
        property var tags: []
        property string uploader: ""
        readonly property bool have: wp !== null && Workshop.downloaded.indexOf(wp.id) >= 0
        readonly property var pct: wp ? Workshop.progress[wp.id] : undefined

        function openFor(w) {
            wp = w
            tags = []
            uploader = ""
            Workshop.fetchTags(w.id)
            open()
        }

        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - 60, 1200)
        height: Math.min(parent.height - 60, 860)
        modal: true
        padding: 16
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.6) }

        Connections {
            target: Workshop
            function onTagsReady(id, tags, uploader) {
                if (details.wp && details.wp.id === id) { details.tags = tags; details.uploader = uploader }
            }
        }

        contentItem: ColumnLayout {
            spacing: 12
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true
                // Miniatura de fondo mientras llega la imagen completa.
                Image {
                    anchors.fill: parent
                    source: details.wp ? details.wp.thumb : ""
                    fillMode: Image.PreserveAspectFit
                    visible: full.status !== Image.Ready
                }
                Image {
                    id: full
                    anchors.fill: parent
                    source: details.visible && details.wp ? details.wp.full : ""
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    sourceSize.width: 1600
                }
                BusyIndicator {
                    anchors.centerIn: parent
                    running: full.status === Image.Loading
                    visible: running
                }
            }
            Label {
                Layout.fillWidth: true
                text: details.wp
                      ? details.wp.resolution + "  ·  " + page.shortSize(details.wp.size)
                        + "  ·  " + ({ anime: "Anime", general: "General", people: "Personas" }[details.wp.category] || details.wp.category)
                        + "  ·  ♥ " + details.wp.favorites
                        + (details.uploader ? "  ·  subido por " + details.uploader : "")
                      : ""
                font.pixelSize: 14
                elide: Text.ElideRight
            }
            Flow {
                Layout.fillWidth: true
                spacing: 6
                visible: details.tags.length > 0
                Repeater {
                    model: details.tags.slice(0, 16)
                    FilterChip {
                        text: "# " + modelData
                        checkable: false
                        autoExclusive: false
                        implicitHeight: 26
                        ToolTip.visible: hovered
                        ToolTip.text: "Buscar «" + modelData + "» en Workshop"
                        onClicked: page.searchTag(modelData)
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Button {
                    text: "Ver en Wallhaven"
                    icon.name: "internet-services"
                    onClicked: Qt.openUrlExternally(details.wp.url)
                    ToolTip.visible: hovered
                    ToolTip.text: "Abre la página del fondo (autor, fuente original y comentarios)"
                }
                Item { Layout.fillWidth: true }
                ProgressBar {
                    visible: details.pct !== undefined
                    from: 0; to: 100; value: details.pct || 0
                    Layout.preferredWidth: 180
                }
                Button {
                    text: "Cerrar"
                    onClicked: details.close()
                }
                Button {
                    text: details.have ? "Ya descargado" : "Descargar"
                    icon.name: "download"
                    enabled: !details.have && details.pct === undefined
                    onClicked: Workshop.download(details.wp, false)
                }
                Button {
                    text: details.have ? "Aplicar" : "Descargar y aplicar"
                    icon.name: "dialog-ok-apply"
                    highlighted: true
                    enabled: details.pct === undefined && !Bridge.busy
                    onClicked: {
                        if (details.have) Bridge.applyFile(Workshop.fileFor(details.wp.id))
                        else Workshop.download(details.wp, true)
                        details.close()
                    }
                }
            }
        }
    }
}
