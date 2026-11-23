// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Ventana principal: cabecera con acciones, barra lateral, páginas y barra de estado.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

ApplicationWindow {
    id: win
    width: 1280
    height: 820
    minimumWidth: 720
    minimumHeight: 480
    visible: true
    title: "NekoWall"
    // Modo oscuro fuera de KDE cuando el sistema no da una paleta oscura (p. ej. niri): ver main.cpp.
    palette.window: nekoDark ? "#1c2026" : undefined
    palette.windowText: nekoDark ? "#e6e9ed" : undefined
    palette.base: nekoDark ? "#14171c" : undefined
    palette.alternateBase: nekoDark ? "#1c2026" : undefined
    palette.text: nekoDark ? "#e6e9ed" : undefined
    palette.placeholderText: nekoDark ? "#8a919a" : undefined
    palette.button: nekoDark ? "#262b33" : undefined
    palette.buttonText: nekoDark ? "#e6e9ed" : undefined
    palette.toolTipBase: nekoDark ? "#262b33" : undefined
    palette.toolTipText: nekoDark ? "#e6e9ed" : undefined
    palette.highlight: nekoDark ? "#39c5bb" : undefined
    palette.highlightedText: nekoDark ? "#0b0f14" : undefined
    palette.link: nekoDark ? "#39c5bb" : undefined
    palette.light: nekoDark ? "#343a44" : undefined
    palette.midlight: nekoDark ? "#2c323b" : undefined
    palette.mid: nekoDark ? "#22272e" : undefined
    palette.dark: nekoDark ? "#101216" : undefined
    palette.shadow: nekoDark ? "#000000" : undefined
    palette.disabled.windowText: nekoDark ? "#6b727c" : undefined
    palette.disabled.text: nekoDark ? "#6b727c" : undefined
    palette.disabled.buttonText: nekoDark ? "#6b727c" : undefined
    // La pantalla de la ventana es el destino por defecto de «Aplicar» («donde está el ratón»).
    onScreenChanged: Bridge.hostScreen = screen ? screen.name : ""
    Component.onCompleted: Bridge.hostScreen = screen ? screen.name : ""

    // --page=N abre la ventana en esa página (0 Biblioteca · 1 Workshop · 2 Ajustes · 3 Acerca de).
    // --link=<enlace>: abre «Descargar con enlace» con ese enlace ya analizado (p. ej. desde el navegador).
    readonly property string startLink: {
        for (const a of Qt.application.arguments)
            if (a.startsWith("--link=")) return a.slice(7)
        return ""
    }
    property int pageIndex: {
        for (const a of Qt.application.arguments)
            if (a.startsWith("--page=")) return Math.max(0, Math.min(3, parseInt(a.slice(7)) || 0))
        return 0
    }
    property string statusText: ""
    property bool statusIsError: false
    property var undoRemoved: null      // { path, name } del último fondo quitado

    function notify(text, isError) {
        statusText = text
        statusIsError = isError
        statusTimer.restart()
    }
    function nameOf(id) {
        for (const w of Bridge.wallpapers) if (w.id === id) return w.name
        return ""
    }
    function undoRemove() {
        if (!undoRemoved) return
        Bridge.importPaths([undoRemoved.path])
        undoRemoved = null
    }

    Timer { id: statusTimer; interval: win.statusIsError ? 8000 : 5000; onTriggered: { if (!Bridge.busy) win.statusText = ""; win.undoRemoved = null } }

    Connections {
        target: Bridge
        function onNotice(text, isError) { win.notify(text, isError) }
        function onError(msg) { win.notify(msg, true) }
        function onApplied(ok, id, msg) {
            win.notify(ok ? "Fondo aplicado: " + win.nameOf(id) : "No se pudo aplicar: " + msg, !ok)
        }
        function onLargeVideosFound(videos, paths) { videoDialog.openFor(videos, paths) }
        function onAdjustReady(info) { adjustDialog.openFor(info) }
        function onOptimizeOffer(videos) { videoDialog.openFor(videos, null) }
        function onRemoved(id, path) {
            const name = path.split("/").pop()
            win.undoRemoved = { path: path, name: name }
            win.notify("Quitado de la biblioteca: " + name + " (el archivo sigue en su carpeta)", false)
        }
    }

    // Atajos globales de la ventana
    Shortcut { sequences: [StandardKey.Find]; onActivated: { win.pageIndex = 0; library.focusSearch() } }
    Shortcut { sequences: [StandardKey.Open]; onActivated: filesDialog.open() }
    Shortcut { sequence: "Ctrl+Shift+O"; onActivated: folderDialog.open() }
    Shortcut { sequences: [StandardKey.Refresh]; onActivated: Bridge.refresh() }
    Shortcut { sequences: [StandardKey.Undo]; enabled: win.undoRemoved !== null; onActivated: win.undoRemove() }
    Shortcut { sequences: [StandardKey.Quit]; onActivated: Qt.quit() }
    Shortcut { sequence: "Ctrl+H"; onActivated: Bridge.openHud() }

    FileDialog {
        id: filesDialog
        title: "Añadir imágenes y vídeos"
        fileMode: FileDialog.OpenFiles
        nameFilters: ["Imágenes y vídeos (*.png *.jpg *.jpeg *.webp *.bmp *.gif *.avif *.tiff *.mp4 *.mkv *.webm *.mov *.avi *.m4v)",
                      "Todos los archivos (*)"]
        onAccepted: Bridge.importPaths(selectedFiles)
    }
    FolderDialog {
        id: folderDialog
        title: "Añadir una carpeta de fondos"
        onAccepted: Bridge.importPaths([selectedFolder])
    }

    AdjustDialog { id: adjustDialog }
    LinkDialog {
        id: linkDialog
        Component.onCompleted: if (win.startLink !== "") Qt.callLater(() => linkDialog.openWith(win.startLink))
    }

    // Vídeos más grandes que la pantalla (al importar) o vídeo a optimizar (menú contextual).
    Dialog {
        id: videoDialog
        property var videos: []
        property var paths: null           // null = vídeos que ya están en la biblioteca
        readonly property bool importing: paths !== null
        readonly property int targetH: Math.min(Bridge.screenHeight, Bridge.screenWidth)
        readonly property bool anyLarge: videos.some(v => v.width > Bridge.screenWidth || v.height > Bridge.screenHeight)
        readonly property bool any60: videos.some(v => v.fps >= 50)
        function openFor(v, p) {
            videos = v; paths = p
            opt30.checked = true
            remember.checked = false
            open()
        }
        anchors.centerIn: parent
        modal: true
        width: Math.min(560, win.width - 40)
        title: anyLarge
               ? (videos.length === 1 ? "Este vídeo es más grande que tu pantalla"
                                      : videos.length + " vídeos son más grandes que tu pantalla")
               : "Este vídeo tiene más fotogramas de los necesarios"

        ColumnLayout {
            width: videoDialog.availableWidth
            spacing: 10
            Label {
                Layout.preferredWidth: videoDialog.availableWidth
                wrapMode: Text.Wrap
                text: "Tu pantalla es de " + Bridge.screenWidth + "×" + Bridge.screenHeight + ". "
                      + (videoDialog.anyLarge
                         ? "No verás la diferencia, pero un vídeo más grande gasta más batería y más memoria."
                         : "A 30 fps el fondo se ve casi igual y Plasma trabaja la mitad.")
            }
            Column {
                Layout.preferredWidth: videoDialog.availableWidth
                spacing: 2
                Repeater {
                    model: videoDialog.videos.slice(0, 5)
                    Label {
                        width: videoDialog.availableWidth
                        elide: Text.ElideMiddle
                        text: "• " + modelData.name + "  —  " + modelData.width + "×" + modelData.height
                              + " · " + Math.round(modelData.fps) + " fps"
                    }
                }
                Label {
                    visible: videoDialog.videos.length > 5
                    opacity: 0.75
                    text: "…y " + (videoDialog.videos.length - 5) + " más"
                }
            }
            ButtonGroup { id: choiceGroup }
            RadioButton {
                id: opt30
                ButtonGroup.group: choiceGroup
                text: "Convertir a " + videoDialog.targetH + "p · 30 fps (recomendado)"
            }
            RadioButton {
                id: opt60
                ButtonGroup.group: choiceGroup
                visible: videoDialog.any60 && videoDialog.anyLarge
                text: "Convertir a " + videoDialog.targetH + "p · 60 fps (más fluido, más consumo)"
            }
            RadioButton {
                id: optKeep
                ButtonGroup.group: choiceGroup
                text: videoDialog.importing ? "Importar tal cual" : "Dejarlo como está"
            }
            Label {
                Layout.preferredWidth: videoDialog.availableWidth
                wrapMode: Text.Wrap
                opacity: 0.75
                visible: !optKeep.checked
                text: "Se crea una copia junto al original, p. ej. «nombre (" + videoDialog.targetH
                      + "p30).mp4», y la biblioteca usa la copia. El original no se modifica ni se borra."
            }
            CheckBox {
                id: remember
                visible: videoDialog.importing
                text: "Recordar mi elección (se puede cambiar en Ajustes)"
            }
        }
        footer: DialogButtonBox {
            Button { text: "Cancelar"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            Button {
                text: videoDialog.importing ? "Importar" : (optKeep.checked ? "Cerrar" : "Convertir")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onAccepted: {
            const choice = optKeep.checked ? "keep" : (opt60.checked ? "60" : "30")
            if (importing) Bridge.importWithChoice(paths, videos, choice, remember.checked)
            else if (choice !== "keep") Bridge.optimizeVideos(videos, choice)
        }
    }

    Dialog {
        id: removeDialog
        property var target: null
        anchors.centerIn: parent
        modal: true
        title: "¿Quitar de la biblioteca?"
        standardButtons: Dialog.Cancel
        Label {
            width: 420
            wrapMode: Text.Wrap
            text: removeDialog.target
                  ? "«" + removeDialog.target.name + "» dejará de aparecer en NekoWall.\n\nEl archivo no se borra: sigue en su carpeta y puedes deshacerlo con Ctrl+Z."
                  : ""
        }
        footer: DialogButtonBox {
            Button { text: "Cancelar"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            Button { text: "Quitar"; icon.name: "list-remove"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
        }
        onAccepted: if (target) Bridge.removeWallpaper(target.id)
        onClosed: library.focusGrid()
    }

    // Arrastrar archivos o carpetas sobre la ventana = importar
    DropArea {
        id: drop
        anchors.fill: parent
        onDropped: (d) => { if (d.hasUrls) Bridge.importPaths(d.urls) }
        Rectangle {
            anchors.fill: parent
            anchors.margins: 8
            z: 100
            visible: drop.containsDrag
            radius: 12
            color: Qt.rgba(win.palette.highlight.r, win.palette.highlight.g, win.palette.highlight.b, 0.15)
            border.width: 2
            border.color: win.palette.highlight
            Label { anchors.centerIn: parent; text: "Suelta para añadir a la biblioteca"; font.pixelSize: 20 }
        }
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 12
            spacing: 8
            Label { text: "NekoWall"; font.pixelSize: 22; font.weight: Font.DemiBold }
            Label {
                text: Bridge.loaded ? "(" + Bridge.wallpapers.length + " fondos)" : ""
                opacity: 0.7
                Layout.alignment: Qt.AlignBaseline
            }
            Item { Layout.fillWidth: true }
            Button {
                text: "Añadir"
                icon.name: "list-add"
                visible: win.pageIndex === 0         // solo en la Biblioteca
                onClicked: addMenu.popup(this, 0, height)
                ToolTip.visible: hovered && !addMenu.visible
                ToolTip.text: "Añadir fondos a la biblioteca"
                Menu {
                    id: addMenu
                    MenuItem { text: "Archivos…   (Ctrl+O)"; icon.name: "document-open"; onTriggered: filesDialog.open() }
                    MenuItem { text: "Carpeta…   (Ctrl+Mayús+O)"; icon.name: "folder-add"; onTriggered: folderDialog.open() }
                    MenuSeparator { visible: Bridge.defaultFolder !== "" }
                    MenuItem {
                        text: "Abrir «Mis fondos» (nekowallpapers)"
                        icon.name: "folder-pictures"
                        visible: Bridge.defaultFolder !== ""
                        height: visible ? implicitHeight : 0
                        onTriggered: Bridge.openDefaultFolder()
                    }
                }
            }
            Button {
                // Descargar un fondo desde un enlace (YouTube, Reddit, un vídeo directo…): botón azul.
                id: linkButton
                text: "Descargar con enlace"
                visible: win.pageIndex === 0         // solo en la Biblioteca, junto a «Añadir»
                onClicked: linkDialog.openWith("")
                ToolTip.visible: hovered
                ToolTip.text: "Descarga un fondo (también animado) desde un enlace: YouTube, Reddit, una página con vídeo o un archivo directo. Eliges el formato."
                contentItem: Label {
                    text: linkButton.text
                    color: "white"
                    font.weight: Font.DemiBold
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    implicitHeight: 32
                    radius: 5
                    color: linkButton.down ? "#1565c0" : linkButton.hovered ? "#3d9bff" : "#1e88ff"
                    border.width: linkButton.visualFocus ? 2 : 0
                    border.color: "white"
                }
                leftPadding: 14
                rightPadding: 14
            }
            Button {
                text: "Selector rápido"
                icon.name: "view-preview"
                visible: win.pageIndex === 0         // solo en la Biblioteca
                highlighted: true                    // la acción principal
                onClicked: Bridge.openHud()
                ToolTip.visible: hovered
                ToolTip.text: "Abrir el HUD (" + Bridge.shortcut + " desde cualquier lugar)"
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Barra lateral
        Pane {
            Layout.fillHeight: true
            Layout.preferredWidth: 220
            padding: 8
            background: Rectangle { color: Qt.darker(win.palette.window, 1.12) }
            ColumnLayout {
                anchors.fill: parent
                spacing: 2
                Repeater {
                    model: [
                        { text: "Biblioteca", icon: "folder-pictures" },
                        { text: "Workshop", icon: "download" },
                        { text: "Ajustes", icon: "configure" },
                        { text: "Acerca de", icon: "help-about" }
                    ]
                    ItemDelegate {
                        Layout.fillWidth: true
                        text: modelData.text
                        icon.name: modelData.icon
                        highlighted: win.pageIndex === index
                        onClicked: win.pageIndex = index
                    }
                }
                Item { Layout.fillHeight: true }
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 16
            currentIndex: win.pageIndex

            LibraryPage {
                id: library
                onRemoveRequested: (w) => { removeDialog.target = w; removeDialog.open() }
                Component.onCompleted: focusGrid()
            }
            WorkshopPage {
                onNotify: (text, isError) => win.notify(text, isError)
            }
            SettingsPage { }
            ColumnLayout {                           // Acerca de
                spacing: 8
                Label { text: "NekoWall"; font.pixelSize: 26; font.weight: Font.DemiBold }
                Label { text: "Gestor de fondos de pantalla estáticos y animados para Void Linux y KDE Plasma (Wayland)."; wrapMode: Text.Wrap; Layout.fillWidth: true }
                Label { text: "Versión 0.1 (prototipo)"; opacity: 0.7 }
                Label { text: "© 2026 Lost (LostXXI) · github.com/LostXXI/nekowall"; font.weight: Font.DemiBold }
                Label {
                    text: "Licencia PolyForm Noncommercial 1.0.0: puedes usarlo, estudiarlo y modificarlo libremente para "
                          + "uso personal y sin ánimo de lucro. Venderlo o usarlo para ganar dinero requiere permiso del autor."
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                    opacity: 0.8
                }
                Label { text: "Inspirado visualmente en Aura (github.com/antwny/aura); no reutiliza su código."; opacity: 0.7; wrapMode: Text.Wrap; Layout.fillWidth: true }
                Item { Layout.fillHeight: true }
            }
        }
    }

    // Barra de estado: fondo actual, backend y mensajes (visibilidad del estado)
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 16
            spacing: 12
            ColumnLayout {
                spacing: 0
                Label {
                    text: (Bridge.currentId ? "Fondo actual: " + (win.nameOf(Bridge.currentId) || "fuera de la biblioteca")
                                            : "Fondo actual: desconocido")
                          + (Bridge.screenPicker ? "  (" + Bridge.targetLabel + ")" : "")
                    elide: Text.ElideRight
                    Layout.maximumWidth: win.width * 0.4
                }
                Label {
                    text: (Bridge.backend === "plasma" ? "KDE Plasma" : Bridge.backend)
                          + (Bridge.backend ? (Bridge.videoSupported ? " · imágenes y vídeo" : " · solo imágenes") : "")
                    opacity: 0.7
                    font.pixelSize: 12
                }
            }
            Item { Layout.fillWidth: true }
            ColumnLayout {
                visible: Bridge.taskActive
                spacing: 2
                Label { text: Bridge.taskText + "  " + Bridge.taskProgress + " %"; font.pixelSize: 12; elide: Text.ElideMiddle; Layout.maximumWidth: 360 }
                ProgressBar { from: 0; to: 100; value: Bridge.taskProgress; Layout.preferredWidth: 260 }
            }
            BusyIndicator {
                running: Bridge.busy
                visible: running
                Layout.preferredHeight: 24
                Layout.preferredWidth: 24
            }
            Label {
                text: win.statusText
                color: win.statusIsError ? "#e0575e" : win.palette.windowText
                elide: Text.ElideRight
                Layout.maximumWidth: win.width * 0.45
            }
            Button {
                text: "Deshacer"
                icon.name: "edit-undo"
                visible: win.undoRemoved !== null
                onClicked: win.undoRemove()
            }
        }
    }
}
