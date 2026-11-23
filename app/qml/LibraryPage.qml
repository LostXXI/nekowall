// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Biblioteca: búsqueda, orden, filtros con contador y rejilla de miniaturas.
// Teclado en la rejilla: flechas = mover · Enter = aplicar · F = favorito · Supr = quitar.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page

    property string filter: "all"          // all | video | static | fav | missing
    property alias query: search.text
    property int sortMode: 0               // 0 nombre · 1 recientes · 2 tamaño

    signal removeRequested(var wallpaper)

    // Modo «Seleccionar»: varios fondos a la vez (para eliminarlos).
    property bool selecting: false
    property var picked: ({})            // id → true
    readonly property int pickedCount: Object.keys(picked).length
    function togglePick(id) {
        let p = Object.assign({}, picked)
        if (p[id]) delete p[id]; else p[id] = true
        picked = p
    }
    function pickAll() { let p = {}; for (const w of shown) p[w.id] = true; picked = p }
    function pickNone() { picked = ({}) }
    function stopSelecting() { selecting = false; picked = ({}) }
    onSelectingChanged: if (!selecting) picked = ({})

    function focusSearch() { search.forceActiveFocus(); search.selectAll() }
    function focusGrid() { grid.forceActiveFocus() }

    readonly property var all: Bridge.wallpapers
    readonly property var counts: {
        let c = { all: all.length, video: 0, static: 0, fav: 0, missing: 0 }
        for (const w of all) {
            if (w.kind === "video") c.video++; else c.static++
            if (w.favorite) c.fav++
            if (w.missing) c.missing++
        }
        return c
    }
    readonly property var shown: {
        const q = search.text.trim().toLowerCase()
        let list = all.filter(w =>
            (filter === "all" || (filter === "video" && w.kind === "video")
             || (filter === "static" && w.kind === "static")
             || (filter === "fav" && w.favorite) || (filter === "missing" && w.missing))
            && (q === "" || w.name.toLowerCase().includes(q)))
        if (sortMode === 1) list.sort((a, b) => b.mtime - a.mtime)
        else if (sortMode === 2) list.sort((a, b) => b.size - a.size)
        else list.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }))
        return list
    }
    readonly property var selected: grid.currentIndex >= 0 && grid.currentIndex < shown.length
                                    ? shown[grid.currentIndex] : null

    // Conserva la selección por id cuando la lista se recalcula.
    // Mientras la lista se recalcula (favorito, filtro, búsqueda, orden, recarga) la rejilla vuelve a la
    // primera tarjeta; `restoring` evita que eso sobrescriba la selección guardada.
    property string selectedId: ""
    property bool restoring: false
    function beginRestore() { restoring = true }
    onFilterChanged: beginRestore()
    onSortModeChanged: beginRestore()
    Connections {
        target: Bridge
        function onWallpapersAboutToChange() { page.beginRestore() }
    }
    onShownChanged: Qt.callLater(() => {
        let idx = shown.length > 0 ? 0 : -1
        for (let i = 0; i < shown.length; ++i)
            if (shown[i].id === selectedId) { idx = i; break }
        grid.currentIndex = idx
        if (idx >= 0) grid.positionViewAtIndex(idx, GridView.Contain)
        restoring = false
        if (idx >= 0) selectedId = shown[idx].id
    })

    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            TextField {
                id: search
                Layout.fillWidth: true
                placeholderText: "Buscar fondos por nombre…  (Ctrl+F)"
                onTextChanged: page.beginRestore()
                Accessible.name: "Buscar fondos"
                Keys.onDownPressed: grid.forceActiveFocus()
                Keys.onReturnPressed: grid.forceActiveFocus()
                Keys.onEscapePressed: { text = ""; grid.forceActiveFocus() }
            }
            // Varias pantallas: dónde se aplica el fondo. Por defecto, la pantalla de esta ventana.
            ComboBox {
                id: screenBox
                visible: Bridge.screenPicker
                // [{text, value}]: value "" = esta pantalla, nombre = esa pantalla, "*" = todas
                readonly property var options: [{ text: "Esta pantalla (" + Bridge.hostScreen + ")", value: "" }]
                    .concat(Bridge.screens.map(s => ({ text: s.label + (s.primary ? " (principal)" : ""), value: s.name })))
                    .concat([{ text: "Todas las pantallas", value: "*" }])
                model: options.map(o => o.text)
                currentIndex: Math.max(0, options.findIndex(o => o.value === Bridge.target))
                onActivated: (i) => Bridge.target = options[i].value
                Layout.preferredWidth: Math.max(implicitWidth, 260)
                ToolTip.visible: hovered
                ToolTip.text: "Pantalla donde se aplica el fondo"
                Accessible.name: "Aplicar en"
            }
            Button {
                text: "Seleccionar"
                icon.name: "checkbox"
                checkable: true
                checked: page.selecting
                onToggled: page.selecting = checked
                ToolTip.visible: hovered
                ToolTip.text: "Elegir varios fondos para eliminarlos"
            }
            ComboBox {
                id: sortBox
                model: ["Nombre (A-Z)", "Más recientes", "Más grandes"]
                currentIndex: page.sortMode
                onActivated: (i) => page.sortMode = i
                Accessible.name: "Ordenar por"
            }
        }

        // Barra del modo «Seleccionar»
        Rectangle {
            Layout.fillWidth: true
            visible: page.selecting
            implicitHeight: selBar.implicitHeight + 16
            radius: 8
            color: Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.16)
            border.width: 1
            border.color: palette.highlight
            RowLayout {
                id: selBar
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8
                Label {
                    Layout.fillWidth: true
                    text: page.pickedCount === 0 ? "Haz clic en los fondos que quieras elegir"
                        : page.pickedCount === 1 ? "1 fondo seleccionado" : page.pickedCount + " fondos seleccionados"
                    font.weight: Font.DemiBold
                }
                Button { text: "Seleccionar todo"; onClicked: page.pickAll(); ToolTip.visible: hovered; ToolTip.text: "Todos los que se ven ahora (Ctrl+A)" }
                Button { text: "Ninguno"; enabled: page.pickedCount > 0; onClicked: page.pickNone() }
                Button {
                    text: "Eliminar…"
                    icon.name: "edit-delete"
                    enabled: page.pickedCount > 0
                    onClicked: deleteDialog.open()
                }
                Button { text: "Cancelar"; onClicked: page.stopSelecting() }
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: 8
            FilterChip { text: "Todos"; count: page.counts.all; checked: page.filter === "all"; onClicked: page.filter = "all" }
            FilterChip { text: "Animados"; count: page.counts.video; checked: page.filter === "video"; onClicked: page.filter = "video" }
            FilterChip { text: "Estáticos"; count: page.counts.static; checked: page.filter === "static"; onClicked: page.filter = "static" }
            FilterChip { text: "Favoritos"; count: page.counts.fav; checked: page.filter === "fav"; onClicked: page.filter = "fav" }
            FilterChip {
                text: "Sin archivo"; count: page.counts.missing
                visible: page.counts.missing > 0
                checked: page.filter === "missing"; onClicked: page.filter = "missing"
            }
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            focus: true
            keyNavigationEnabled: true
            boundsBehavior: Flickable.StopAtBounds
            readonly property int columns: Math.max(1, Math.floor(width / 260))
            cellWidth: Math.floor(width / columns)
            // miniatura 16:9 + nombre + tipo + botones + márgenes (sin hueco vacío debajo)
            cellHeight: Math.round((cellWidth - 28) * 9 / 16) + 124
            model: page.shown
            cacheBuffer: cellHeight * 2
            ScrollBar.vertical: ScrollBar { }
            FastWheel { flick: grid }
            onCurrentIndexChanged: if (!page.restoring && page.selected) page.selectedId = page.selected.id

            delegate: WallpaperTile {
                width: grid.cellWidth
                height: grid.cellHeight
                selected: GridView.isCurrentItem && grid.activeFocus
                isCurrentWallpaper: modelData.id === Bridge.currentId
                selecting: page.selecting
                picked: page.picked[modelData.id] === true
                onClicked: {
                    grid.currentIndex = index
                    grid.forceActiveFocus()
                    if (page.selecting) page.togglePick(modelData.id)
                }
                onApplyRequested: Bridge.apply(modelData.id)
                onFavoriteToggled: Bridge.setFavorite(modelData.id, !modelData.favorite)
                onContextMenuRequested: (x, y) => { menu.target = modelData; menu.popup() }
            }

            Keys.onReturnPressed: if (page.selected && !page.selected.missing) Bridge.apply(page.selected.id)
            Keys.onEnterPressed: if (page.selected && !page.selected.missing) Bridge.apply(page.selected.id)
            Keys.onDeletePressed: {
                if (page.selecting) { if (page.pickedCount > 0) deleteDialog.open() }
                else if (page.selected) page.removeRequested(page.selected)
            }
            Keys.onSpacePressed: if (page.selecting && page.selected) page.togglePick(page.selected.id)
            Keys.onEscapePressed: if (page.selecting) page.stopSelecting()
            Keys.onPressed: (e) => {
                if (e.key === Qt.Key_A && (e.modifiers & Qt.ControlModifier) && page.selecting) {
                    page.pickAll()
                    e.accepted = true
                } else if (e.key === Qt.Key_F && e.modifiers === Qt.NoModifier && page.selected) {
                    Bridge.setFavorite(page.selected.id, !page.selected.favorite)
                    e.accepted = true
                }
            }

            // Estados vacíos
            Column {
                anchors.centerIn: parent
                spacing: 10
                visible: page.shown.length === 0
                width: Math.min(parent.width - 40, 480)
                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: 16
                    text: !Bridge.loaded ? "Cargando biblioteca…"
                        : page.all.length === 0 ? "Tu biblioteca está vacía"
                        : search.text !== "" ? "Ningún fondo coincide con «" + search.text + "»"
                        : "No hay fondos en este filtro"
                }
                Label {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    visible: Bridge.loaded && page.all.length === 0
                    text: (Bridge.defaultFolder !== ""
                           ? "Copia tus imágenes y vídeos en la carpeta «nekowallpapers» (dentro de tu carpeta de Imágenes) y aparecerán aquí solos. "
                           : "")
                        + "También puedes añadir otra carpeta o arrastrar archivos a esta ventana. Los archivos no se copian ni se modifican."
                }
                Button {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: Bridge.loaded && page.all.length === 0 && Bridge.defaultFolder !== ""
                    text: "Abrir la carpeta nekowallpapers"
                    icon.name: "folder-pictures"
                    highlighted: true
                    onClicked: Bridge.openDefaultFolder()
                }
            }
        }
    }

    // Confirmación de «Eliminar…» (modo «Seleccionar»)
    Dialog {
        id: deleteDialog
        anchors.centerIn: parent
        modal: true
        width: Math.min(520, page.width - 40)
        title: page.pickedCount === 1 ? "¿Eliminar 1 fondo?" : "¿Eliminar " + page.pickedCount + " fondos?"
        onAboutToShow: trashBox.checked = true
        ColumnLayout {
            width: deleteDialog.availableWidth
            spacing: 10
            CheckBox {
                id: trashBox
                text: "Mover también los archivos a la papelera"
            }
            Label {
                Layout.preferredWidth: deleteDialog.availableWidth
                wrapMode: Text.Wrap
                opacity: 0.8
                text: trashBox.checked
                      ? "Los archivos irán a la papelera: si te arrepientes, los recuperas desde ahí y vuelven a aparecer en NekoWall. Los fondos del sistema (los de neko-void) no se pueden borrar: solo se quitan de la biblioteca."
                      : "Solo se quitan de NekoWall. Los archivos se quedan en su carpeta y no vuelven a aparecer en la biblioteca."
            }
        }
        footer: DialogButtonBox {
            Button { text: "Cancelar"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
            Button {
                text: trashBox.checked ? "Eliminar" : "Quitar de la biblioteca"
                icon.name: trashBox.checked ? "edit-delete" : "list-remove"
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onAccepted: {
            Bridge.removeMany(Object.keys(page.picked), trashBox.checked)
            page.stopSelecting()
        }
        onClosed: grid.forceActiveFocus()
    }

    Menu {
        id: menu
        property var target: null
        MenuItem {
            text: "Aplicar"
            icon.name: "dialog-ok-apply"
            enabled: menu.target && !menu.target.missing && menu.target.id !== Bridge.currentId
            onTriggered: Bridge.apply(menu.target.id)
        }
        MenuItem {
            text: menu.target && menu.target.favorite ? "Quitar de favoritos" : "Marcar como favorito"
            icon.name: menu.target && menu.target.favorite ? "starred-symbolic" : "non-starred-symbolic"
            onTriggered: Bridge.setFavorite(menu.target.id, !menu.target.favorite)
        }
        MenuItem {
            text: "Optimizar para mi pantalla…"
            icon.name: "transform-scale"
            visible: menu.target && menu.target.kind === "video" && !menu.target.missing
            height: visible ? implicitHeight : 0
            enabled: !Bridge.taskActive
            onTriggered: Bridge.requestOptimize(menu.target.path)
        }
        MenuItem {
            text: "Ajustar a mi pantalla…"
            icon.name: "transform-crop"
            visible: menu.target && menu.target.kind === "static" && !menu.target.missing
            height: visible ? implicitHeight : 0
            onTriggered: Bridge.openAdjust(menu.target.path, menu.target.name)
        }
        MenuItem {
            text: "Abrir carpeta contenedora"
            icon.name: "folder-open"
            onTriggered: Bridge.revealInFolder(menu.target.path)
        }
        MenuSeparator { }
        MenuItem {
            text: "Quitar de la biblioteca…"
            icon.name: "list-remove"
            onTriggered: page.removeRequested(menu.target)
        }
    }
}
