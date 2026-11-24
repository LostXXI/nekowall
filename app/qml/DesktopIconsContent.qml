// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Contenido de los iconos del escritorio de NekoWall (lo usan DesktopIcons.qml —Wayland, layer-shell— y
// DesktopIconsX11.qml —MATE—), ENCIMA del vídeo de fondo y debajo de las ventanas.
// Modo «pcmanfm» (dm.flavor) imita a pcmanfm-qt; modo «caja» imita al escritorio de Caja (MATE): cada icono
// trae su propia geometría (calculada como Caja, con Pango) y los menús usan los textos de Caja traducidos.
// Imita al escritorio de pcmanfm-qt: mismos ajustes y posiciones (sus archivos), misma rejilla, clic /
// doble clic, selección con recuadro, Ctrl/Mayús, arrastrar para recolocar (queda fijo), soltar sobre una
// carpeta o la papelera, soltar archivos desde otro programa, teclado (flechas, Intro, Supr, Mayús+Supr,
// F2, Ctrl+A/C/X/V, Ctrl+Mayús+N, Ctrl+H) y sus menús.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Item {
    id: root

    required property var dm             // un DesktopModel por pantalla (lo crea main.cpp)
    readonly property color hl: palette.highlight
    readonly property bool caja: dm.flavor === "caja"
    // Texto de un menú de Caja, traducido por Caja, sin el «_» de la tecla de acceso.
    function ct(id, dominio) { return dm.cajaText(id, dominio || "caja").replace(/_(?!_)/g, "") }

    // Estado de arrastre (dentro del escritorio)
    property bool dragging: false
    property real dragDx: 0
    property real dragDy: 0
    property int dragRow: -1
    property int dropTarget: -1
    property point dropCell: Qt.point(0, 0)
    property int extTarget: -1          // destino al soltar archivos de otro programa
    property int renaming: -1
    property int hoverRow: -1
    property bool tipHidden: false
    onHoverRowChanged: tipHidden = false
    property int menuRow: -1
    property var apps: []

    onWidthChanged: dm.setViewSize(width, height)
    onHeightChanged: dm.setViewSize(width, height)
    Component.onCompleted: dm.setViewSize(width, height)

    property string toast: ""
    Timer { id: toastTimer; interval: 3500; onTriggered: root.toast = "" }
    function say(t) { root.toast = t; toastTimer.restart() }
    Connections {
        target: root.dm
        function onMessage(text) { root.say(text) }
    }

    // ── Acciones ──
    function askTrash() {
        if (dm.selectedCount === 0) return
        if (!dm.useTrash) { askDelete(); return }
        if (dm.confirmTrash)
            confirm("¿Mover a la papelera " + countText() + "?", function () { dm.trashSelected() })
        else
            dm.trashSelected()
    }
    function askDelete() {
        if (dm.selectedCount === 0) return
        if (dm.confirmDelete)
            confirm("¿Eliminar definitivamente " + countText() + "?\nNo se puede deshacer.", function () { dm.deleteSelected() })
        else
            dm.deleteSelected()
    }
    function countText() {
        return dm.selectedCount === 1 ? "el elemento seleccionado" : "los " + dm.selectedCount + " elementos seleccionados"
    }
    function startRename(row) {
        if (row < 0 || row >= dm.count) return
        dm.select(row, 0)
        renaming = row
    }
    function openItemMenu(row) {
        menuRow = row
        apps = dm.appsFor(row)
        const it = repeaterItem(row)
        if (caja) {
            if (it && it.special) cajaSpecialMenu.popup()
            else cajaItemMenu.popup()
        } else if (it && it.isTrash) trashMenu.popup()
        else itemMenu.popup()
    }
    function openDesktopMenu() { if (caja) cajaDesktopMenu.popup(); else desktopMenu.popup() }
    function repeaterItem(row) { return icons.itemAt(row) }
    function newFolder() { const r = dm.createFolder(); if (r >= 0) startRename(r) }
    function newFile() { const r = dm.createFile(); if (r >= 0) startRename(r) }
    function tipFor(row) {
        const p = dm.properties(row)
        if (!p.name) return ""
        let t = p.name + "\nTipo: " + p.type + "\nModificado: " + p.modified
        if (p.mime !== "inode/directory") t += "\nTamaño: " + p.size
        return t
    }

    // ── Escritorio ──
    Item {
        id: desk
        anchors.fill: parent
        focus: true

        Keys.onPressed: (e) => {
            root.tipHidden = true              // como pcmanfm-qt: una tecla quita la descripción emergente
            const ctrl = e.modifiers & Qt.ControlModifier
            const shift = e.modifiers & Qt.ShiftModifier
            switch (e.key) {
            case Qt.Key_Left: case Qt.Key_Right: case Qt.Key_Up: case Qt.Key_Down:
            case Qt.Key_Home: case Qt.Key_End: case Qt.Key_PageUp: case Qt.Key_PageDown:
                dm.navigate(e.key, shift); break
            case Qt.Key_Return: case Qt.Key_Enter: dm.openSelected(); break
            case Qt.Key_Delete: if (shift) root.askDelete(); else root.askTrash(); break
            case Qt.Key_F2: root.startRename(dm.current); break
            case Qt.Key_Escape: dm.clearSelection(); break
            case Qt.Key_Menu:
                if (dm.selectedCount > 0) root.openItemMenu(dm.current); else root.openDesktopMenu()
                break
            case Qt.Key_A: if (ctrl) dm.selectAll(); else return; break
            case Qt.Key_C: if (ctrl) dm.copySelected(false); else return; break
            case Qt.Key_X: if (ctrl) dm.copySelected(true); else return; break
            case Qt.Key_V: if (ctrl) dm.paste(-1); else return; break
            case Qt.Key_H: if (ctrl) dm.setShowHidden(!dm.showHidden); else return; break
            case Qt.Key_N: if (ctrl && shift) root.newFolder(); else return; break
            default: return
            }
            e.accepted = true
        }

        // Fondo: quita la selección, recuadro de selección y menú del escritorio.
        MouseArea {
            id: bg
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            property point start
            property bool banding: false
            onPressed: (m) => {
                desk.forceActiveFocus()
                root.renaming = -1
                const ctrl = m.modifiers & Qt.ControlModifier
                if (m.button === Qt.RightButton) {
                    if (!ctrl) dm.clearSelection()
                    root.openDesktopMenu()
                    return
                }
                start = Qt.point(m.x, m.y)
                dm.beginRubberBand(ctrl)
                if (!ctrl) dm.clearSelection()
                rubber.x = m.x; rubber.y = m.y; rubber.width = 0; rubber.height = 0
                banding = true
            }
            onPositionChanged: (m) => {
                if (!banding) return
                rubber.x = Math.min(start.x, m.x); rubber.y = Math.min(start.y, m.y)
                rubber.width = Math.abs(m.x - start.x); rubber.height = Math.abs(m.y - start.y)
                dm.rubberBand(rubber.x, rubber.y, rubber.width, rubber.height)
            }
            onReleased: banding = false
            onCanceled: banding = false
            onWheel: (w) => { w.accepted = true }
        }

        Rectangle {                                   // recuadro de selección
            id: rubber
            visible: bg.banding && width > 2 && height > 2
            color: Qt.rgba(root.hl.r, root.hl.g, root.hl.b, 0.25)
            border.color: root.hl
            border.width: 1
        }

        Rectangle {                                   // celda donde caerán los iconos arrastrados
            visible: root.dragging && root.dropTarget < 0 && !root.caja
            x: root.dropCell.x; y: root.dropCell.y
            width: dm.cellWidth; height: dm.cellHeight
            color: "transparent"
            radius: 3
            border.width: 1
            border.color: Qt.rgba(1, 1, 1, 0.7)
        }

        Repeater {
            id: icons
            model: root.dm
            delegate: Item {
                id: cell
                required property int index
                required property string name
                required property string fileName
                required property string icon
                required property string thumb
                required property int posX
                required property int posY
                required property bool selected
                required property bool cut
                required property bool hiddenFile
                required property bool isDir
                required property bool isTrash
                required property bool sticky
                required property int boxX
                required property int boxY
                required property int boxW
                required property int boxH
                required property int iconX
                required property int iconY
                required property int iconPx
                required property int textX
                required property int textY
                required property int textW
                required property int textH
                required property int editableH
                required property string additional
                required property bool special
                readonly property bool moving: root.dragging && selected
                readonly property bool dropHere: (root.dragging && root.dropTarget === index) || root.extTarget === index

                visible: !dm.hideItems
                x: (root.caja ? boxX : posX) + (moving ? root.dragDx : 0)
                y: (root.caja ? boxY : posY) + (moving ? root.dragDy : 0)
                z: moving ? 10 : 0
                width: root.caja ? boxW : dm.cellWidth
                height: root.caja ? boxH : dm.cellHeight
                opacity: moving ? 0.7 : 1

                Image {
                    id: img
                    x: root.caja ? cell.iconX : (cell.width - dm.iconSize) / 2
                    y: root.caja ? cell.iconY : dm.marginHeight
                    width: root.caja ? cell.iconPx : dm.iconSize
                    height: width
                    sourceSize: Qt.size(width, height)
                    fillMode: Image.PreserveAspectFit
                    source: cell.thumb !== "" ? cell.thumb : "image://icono/" + cell.icon
                    asynchronous: true
                    cache: true
                    opacity: cell.cut || cell.hiddenFile ? 0.5 : 1
                    onStatusChanged: if (status === Image.Error && cell.thumb !== "") source = "image://icono/" + cell.icon
                    layer.enabled: cell.selected || cell.dropHere
                    layer.effect: MultiEffect {
                        colorization: 0.45
                        colorizationColor: root.hl
                    }
                }

                Item {
                    id: textArea
                    // Caja: el recuadro del nombre lleva 4 px de margen a cada lado y 1 arriba (TEXT_BACK_PADDING).
                    x: root.caja ? cell.textX + 4 : dm.marginWidth + 2
                    y: root.caja ? cell.textY + 1 : dm.marginHeight + dm.iconSize + 2
                    width: root.caja ? cell.textW - 8 : dm.cellWidth - 2 * dm.marginWidth - 4
                    height: root.caja ? cell.textH - 2 : dm.cellHeight - 2 * dm.marginHeight - dm.iconSize - 4

                    Rectangle {                         // Caja: fondo del nombre seleccionado (todo el recuadro)
                        visible: root.caja && cell.selected && root.renaming !== cell.index
                        color: root.hl
                        radius: 3
                        x: -4; y: -1
                        width: cell.textW; height: cell.textH
                    }
                    Image {                             // Caja: el nombre dibujado con Pango, igual que Caja
                        visible: root.caja && root.renaming !== cell.index
                        x: -4; y: -1
                        width: cell.textW; height: cell.textH + 1
                        cache: false
                        source: root.caja ? "image://etiqueta/" + cell.index + "/" + (cell.selected ? 1 : 0) + "/"
                                            + encodeURIComponent(cell.name + "|" + cell.additional + "|" + cell.textW) : ""
                    }
                    Rectangle {                         // fondo del nombre seleccionado
                        visible: !root.caja && cell.selected && root.renaming !== cell.index
                        color: root.hl
                        x: (textArea.width - label.contentWidth) / 2 - 2
                        y: -2
                        width: label.contentWidth + 4
                        height: label.contentHeight + 4
                    }
                    Text {                              // sombra (como pcmanfm-qt: 1 px abajo a la derecha)
                        visible: !root.caja && !cell.selected && dm.prefs.shadowOn && root.renaming !== cell.index
                        x: 1; y: 1
                        width: textArea.width
                        text: label.text
                        font: dm.font
                        color: dm.shadowColor
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        horizontalAlignment: Text.AlignHCenter
                        maximumLineCount: 3
                        elide: Text.ElideRight
                    }
                    Text {
                        id: label
                        visible: !root.caja && root.renaming !== cell.index
                        width: textArea.width
                        text: cell.name
                        font: dm.font
                        color: cell.selected ? root.palette.highlightedText : dm.fgColor
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        horizontalAlignment: Text.AlignHCenter
                        maximumLineCount: root.caja ? Math.max(1, Math.round((cell.editableH - 2) / Math.max(1, fontInfo.lineHeight))) : 3
                        elide: Text.ElideRight
                        opacity: cell.cut ? 0.6 : 1
                        FontMetrics { id: fontInfo; font: dm.font; readonly property real lineHeight: height }
                    }
                    Text {                              // (los pies de icono de Caja van en la imagen de Pango)
                        visible: false
                        y: cell.editableH - 2
                        width: textArea.width
                        text: cell.additional
                        font: dm.font
                        color: cell.selected ? root.palette.highlightedText : Qt.rgba(1, 1, 1, 0.8)
                        style: Text.Outline
                        styleColor: Qt.rgba(0, 0, 0, 0.35)
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        elide: Text.ElideRight
                    }
                    Loader {                            // renombrar en el sitio (F2)
                        active: root.renaming === cell.index
                        width: Math.max(textArea.width + 30, 170)
                        // Centrado bajo el icono, pero sin salirse de la pantalla.
                        x: Math.min(Math.max((textArea.width - width) / 2, 4 - cell.x - textArea.x),
                                    root.width - 4 - width - cell.x - textArea.x)
                        sourceComponent: TextField {
                            id: editor
                            text: cell.name
                            font: dm.font
                            property bool done: false
                            function commit() {
                                if (done) return
                                done = true
                                if (cell.index < 0 || root.renaming !== cell.index) return   // el icono ya no está
                                const err = dm.renameItem(cell.index, text)
                                root.renaming = -1
                                desk.forceActiveFocus()
                                if (err !== "") root.say(err)
                            }
                            Component.onCompleted: {
                                forceActiveFocus()
                                const dot = text.lastIndexOf(".")
                                if (!cell.isDir && dot > 0) select(0, dot); else selectAll()
                            }
                            Keys.onReturnPressed: commit()
                            Keys.onEnterPressed: commit()
                            Keys.onEscapePressed: { done = true; root.renaming = -1; desk.forceActiveFocus() }
                            onActiveFocusChanged: if (!activeFocus) commit()
                        }
                    }
                }

                HoverHandler {
                    onHoveredChanged: {
                        if (hovered) root.hoverRow = cell.index
                        else if (root.hoverRow === cell.index) root.hoverRow = -1
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    anchors.leftMargin: root.caja ? 0 : dm.marginWidth; anchors.rightMargin: root.caja ? 0 : dm.marginWidth
                    anchors.topMargin: root.caja ? 0 : dm.marginHeight; anchors.bottomMargin: root.caja ? 0 : dm.marginHeight
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    property point pressPos
                    // Solo el icono y el nombre cuentan (como indexAt de libfm-qt), no el hueco de la celda.
                    function inside(m) {
                        const p = mapToItem(cell, m.x, m.y)
                        const onIcon = p.x >= img.x && p.x <= img.x + img.width && p.y >= img.y && p.y <= img.y + img.height
                        const tw = Math.max(label.contentWidth, 8)
                        const onText = p.y >= textArea.y - 2 && p.y <= textArea.y + label.contentHeight + 2
                                       && p.x >= textArea.x + (textArea.width - tw) / 2 - 2
                                       && p.x <= textArea.x + (textArea.width + tw) / 2 + 2
                        return onIcon || onText
                    }
                    onPressed: (m) => {
                        if (!inside(m)) { m.accepted = false; return }
                        desk.forceActiveFocus()
                        if (root.renaming !== cell.index) root.renaming = -1
                        const ctrl = m.modifiers & Qt.ControlModifier
                        const shift = m.modifiers & Qt.ShiftModifier
                        if (m.button === Qt.RightButton) {
                            if (!cell.selected) dm.select(cell.index, 0)
                            root.openItemMenu(cell.index)
                            return
                        }
                        if (ctrl) dm.select(cell.index, 1)
                        else if (shift) dm.select(cell.index, 2)
                        else if (!cell.selected) dm.select(cell.index, 0)
                        pressPos = mapToItem(desk, m.x, m.y)
                    }
                    onPositionChanged: (m) => {
                        if (!(m.buttons & Qt.LeftButton) || !cell.selected) return
                        const p = mapToItem(desk, m.x, m.y)
                        const dx = p.x - pressPos.x, dy = p.y - pressPos.y
                        if (!root.dragging && Math.abs(dx) + Math.abs(dy) < 10) return
                        root.dragging = true
                        root.dragRow = cell.index
                        root.dragDx = dx; root.dragDy = dy
                        const t = dm.rowAt(p.x, p.y)
                        root.dropTarget = dm.isDropTarget(t) ? t : -1
                        if (!root.caja)
                            root.dropCell = dm.cellFor(cell.posX + dx + dm.cellWidth / 2, cell.posY + dy + dm.cellHeight / 2)
                    }
                    onReleased: (m) => {
                        if (root.dragging) {
                            const dx = root.dragDx, dy = root.dragDy, target = root.dropTarget
                            root.dragging = false; root.dropTarget = -1
                            if (target >= 0) dm.dropSelectionOn(target)
                            else dm.moveSelection(dx, dy)
                            return
                        }
                        if (m.button !== Qt.LeftButton) return
                        if (!(m.modifiers & (Qt.ControlModifier | Qt.ShiftModifier))) {
                            dm.select(cell.index, 0)
                            if (dm.singleClick) dm.open(cell.index)
                        }
                    }
                    onCanceled: { root.dragging = false; root.dropTarget = -1 }
                    onDoubleClicked: (m) => { if (m.button === Qt.LeftButton && !dm.singleClick) dm.open(cell.index) }
                }
            }
        }

        // Archivos soltados desde otro programa (gestor de archivos, navegador…).
        DropArea {
            anchors.fill: parent
            onPositionChanged: (d) => { const t = dm.rowAt(d.x, d.y); root.extTarget = dm.isDropTarget(t) ? t : -1 }
            onExited: root.extTarget = -1
            onDropped: (d) => {
                root.extTarget = -1
                // En Wayland, d.urls llega vacío si el otro programa ofrece los archivos por el «portal» y no
                // hay xdg-desktop-portal: se prueban los demás formatos (pcmanfm-qt: libfm/files).
                let uris = []
                for (let i = 0; i < d.urls.length; ++i) uris.push(String(d.urls[i]))
                for (const f of ["text/uri-list", "libfm/files", "text/x-moz-url", "text/plain"]) {
                    if (uris.length > 0) break
                    if (d.formats.indexOf(f) < 0) continue
                    uris = d.getDataAsString(f).split(/\r?\n/).map(l => l.trim())
                            .filter(l => l.startsWith("file://") || l.startsWith("/"))
                            .map(l => l.startsWith("/") ? "file://" + l : l)
                }
                if (uris.length === 0) return
                dm.dropUrls(uris, d.x, d.y, d.proposedAction === Qt.CopyAction)
                d.acceptProposedAction()
            }
        }
    }

    // Descripción emergente: una sola, del icono bajo el ratón.
    ToolTip {
        id: tip
        parent: root.hoverRow >= 0 ? icons.itemAt(root.hoverRow) : desk
        visible: root.hoverRow >= 0 && !root.tipHidden && dm.tooltips && !root.dragging && root.renaming < 0 && !bg.banding
                 && !itemMenu.visible && !desktopMenu.visible && !trashMenu.visible && text !== ""
        delay: 700
        y: parent ? parent.height - 6 : 0
        text: root.hoverRow >= 0 ? root.tipFor(root.hoverRow) : ""
    }

    // ── Menús (los de pcmanfm-qt) ──
    component MItem: MenuItem { height: visible ? implicitHeight : 0 }
    component MSep: MenuSeparator { height: visible ? implicitHeight : 0 }

    Menu {
        id: itemMenu
        width: 300
        readonly property bool single: dm.selectedCount === 1
        readonly property bool dirItem: single && root.repeaterItem(root.menuRow) !== null && root.repeaterItem(root.menuRow).isDir
        MItem { text: "Abrir"; font.bold: true; onTriggered: dm.openSelected() }
        Menu {
            id: openWithMenu
            title: "Abrir con"
            Instantiator {
                model: root.apps
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.name + (modelData.isDefault ? "  (predeterminada)" : "")
                    icon.source: modelData.icon !== "" ? "image://icono/" + modelData.icon : ""
                    icon.width: 16; icon.height: 16
                    onTriggered: dm.openWith(root.menuRow, modelData.id)
                }
                onObjectAdded: (i, o) => openWithMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => openWithMenu.removeItem(o)
            }
            MItem { text: "(ninguna aplicación)"; enabled: false; visible: root.apps.length === 0 }
        }
        MenuSeparator { }
        MItem { text: "Cortar"; onTriggered: dm.copySelected(true) }
        MItem { text: "Copiar"; onTriggered: dm.copySelected(false) }
        MItem { text: "Copiar la ruta completa"; onTriggered: dm.copyPaths() }
        MItem { text: "Pegar dentro de la carpeta"; visible: itemMenu.dirItem; enabled: dm.canPaste(); onTriggered: dm.paste(root.menuRow) }
        MItem { text: dm.useTrash ? "Mover a la papelera" : "Eliminar"; onTriggered: root.askTrash() }
        MItem { text: "Eliminar definitivamente"; visible: dm.useTrash; onTriggered: root.askDelete() }
        MItem { text: "Renombrar"; visible: itemMenu.single; onTriggered: root.startRename(root.menuRow) }
        MSep { visible: !dm.allSticky }
        MItem {
            text: "Fijar en la posición actual"
            visible: !dm.allSticky
            checkable: true
            checked: itemMenu.visible && root.repeaterItem(root.menuRow) !== null && root.repeaterItem(root.menuRow).sticky
            onTriggered: dm.setStickySelected(checked)
        }
        MenuSeparator { }
        MItem { text: "Propiedades"; visible: itemMenu.single; onTriggered: { propsDialog.info = dm.properties(root.menuRow); propsDialog.open() } }
    }

    Menu {                                            // el acceso de la Papelera (como pcmanfm-qt)
        id: trashMenu
        width: 260
        MenuItem { text: "Abrir"; font.bold: true; onTriggered: dm.open(root.menuRow) }
        MItem {
            text: "Fijar en la posición actual"
            visible: !dm.allSticky
            checkable: true
            checked: trashMenu.visible && root.repeaterItem(root.menuRow) !== null && root.repeaterItem(root.menuRow).sticky
            onTriggered: dm.setStickySelected(checked)
        }
        MenuSeparator { }
        MenuItem {
            text: "Vaciar la papelera"
            enabled: trashMenu.visible && dm.trashCount() > 0
            onTriggered: root.confirm("¿Vaciar la papelera?\nSe eliminarán definitivamente todos sus elementos.", function () { dm.emptyTrash() })
        }
    }

    Menu {                                            // mismo orden que el de pcmanfm-qt 2.x
        id: desktopMenu
        width: 340
        MenuItem { text: "Abrir en una terminal"; icon.source: "image://icono/utilities-terminal"; onTriggered: dm.openTerminal() }
        Menu {
            title: "Crear nuevo"
            MenuItem { text: "Carpeta"; icon.source: "image://icono/folder-new"; onTriggered: root.newFolder() }
            MenuItem { text: "Archivo vacío"; icon.source: "image://icono/document-new"; onTriggered: root.newFile() }
        }
        MenuItem { text: "Pegar"; icon.source: "image://icono/edit-paste"; enabled: desktopMenu.visible && dm.canPaste(); onTriggered: dm.paste(-1) }
        MenuItem { text: "Seleccionar todo"; icon.source: "image://icono/edit-select-all"; onTriggered: dm.selectAll() }
        Menu {
            title: "Ordenar"
            MenuItem { text: "Por nombre"; checkable: true; checked: dm.sortColumn === "name"; onTriggered: dm.setSort("name") }
            MenuItem { text: "Por tipo"; checkable: true; checked: dm.sortColumn === "type"; onTriggered: dm.setSort("type") }
            MenuItem { text: "Por tamaño"; checkable: true; checked: dm.sortColumn === "size"; onTriggered: dm.setSort("size") }
            MenuItem { text: "Por fecha de modificación"; checkable: true; checked: dm.sortColumn === "mtime"; onTriggered: dm.setSort("mtime") }
            MenuSeparator { }
            MenuItem { text: "Ascendente"; checkable: true; checked: dm.sortAscending; onTriggered: dm.setSortAscending(true) }
            MenuItem { text: "Descendente"; checkable: true; checked: !dm.sortAscending; onTriggered: dm.setSortAscending(false) }
            MenuSeparator { }
            MenuItem { text: "Carpetas primero"; checkable: true; checked: dm.folderFirst; onTriggered: dm.setFolderFirst(checked) }
            MenuItem { text: "Ocultos al final"; checkable: true; checked: dm.hiddenLast; onTriggered: dm.setHiddenLast(checked) }
        }
        MenuItem { text: "Mostrar ocultos"; checkable: true; checked: dm.showHidden; onTriggered: dm.setShowHidden(checked) }
        MenuSeparator { }
        MenuItem { text: "Ocultar los elementos del escritorio"; checkable: true; checked: dm.hideItems; onTriggered: dm.setHideItems(checked) }
        MenuItem { text: "Crear lanzador"; enabled: !dm.hideItems; onTriggered: launcherDialog.open() }
        MenuItem { text: "Preferencias del escritorio"; icon.source: "image://icono/preferences-desktop"; onTriggered: prefsDialog.open() }
        MenuSeparator { }
        MenuItem { text: "Cambiar el fondo (NekoWall)…"; icon.source: "image://icono/preferences-desktop-wallpaper"; onTriggered: dm.openNekoWall() }
    }

    // ── Menús de Caja (MATE), con sus textos traducidos ──
    Menu {
        id: cajaDesktopMenu
        width: 300
        MenuItem { text: root.ct("Create _Folder"); icon.source: "image://icono/folder-new"; onTriggered: root.newFolder() }
        MenuItem { text: root.ct("Create L_auncher..."); onTriggered: launcherDialog.open() }
        Menu {
            title: root.ct("Create _Document")
            MenuItem { text: root.ct("_Empty File"); icon.source: "image://icono/text-x-generic"; onTriggered: root.newFile() }
        }
        MenuSeparator { }
        MenuItem { text: root.ct("Open in _Terminal", "caja-extensions"); icon.source: "image://icono/utilities-terminal"; onTriggered: dm.openTerminal() }
        MenuSeparator { }
        MenuItem { text: root.ct("_Organize by Name"); onTriggered: dm.organizeByName() }
        MenuItem { text: root.ct("_Keep Aligned"); checkable: true; checked: dm.keepAligned; onTriggered: dm.setKeepAligned(checked) }
        MenuSeparator { }
        MenuItem { text: root.ct("_Paste"); icon.source: "image://icono/edit-paste"; enabled: cajaDesktopMenu.visible && dm.canPaste(); onTriggered: dm.paste(-1) }
        MenuItem { text: root.ct("Select _All"); onTriggered: dm.selectAll() }
        MenuSeparator { }
        MenuItem { text: root.ct("Change Desktop _Background"); icon.source: "image://icono/preferences-desktop-wallpaper"; onTriggered: dm.openNekoWall() }
    }
    Menu {
        id: cajaItemMenu
        width: 300
        readonly property bool single: dm.selectedCount === 1
        readonly property bool dirItem: single && root.repeaterItem(root.menuRow) !== null && root.repeaterItem(root.menuRow).isDir
        MItem { text: root.ct("_Open"); font.bold: true; onTriggered: dm.openSelected() }
        Menu {
            id: cajaOpenWith
            title: root.ct("Open Wit_h")
            Instantiator {
                model: root.apps
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.name
                    icon.source: modelData.icon !== "" ? "image://icono/" + modelData.icon : ""
                    icon.width: 16; icon.height: 16
                    onTriggered: dm.openWith(root.menuRow, modelData.id)
                }
                onObjectAdded: (i, o) => cajaOpenWith.insertItem(i, o)
                onObjectRemoved: (i, o) => cajaOpenWith.removeItem(o)
            }
        }
        MenuSeparator { }
        MItem { text: root.ct("Cu_t"); onTriggered: dm.copySelected(true) }
        MItem { text: root.ct("_Copy"); onTriggered: dm.copySelected(false) }
        MItem { text: root.ct("_Paste Into Folder"); visible: cajaItemMenu.dirItem; enabled: dm.canPaste(); onTriggered: dm.paste(root.menuRow) }
        MItem { text: root.ct("_Rename..."); visible: cajaItemMenu.single; onTriggered: root.startRename(root.menuRow) }
        MenuSeparator { }
        MItem { text: root.ct("Mo_ve to Trash"); onTriggered: root.askTrash() }
        MenuSeparator { }
        MItem { text: root.ct("_Properties"); visible: cajaItemMenu.single; onTriggered: { propsDialog.info = dm.properties(root.menuRow); propsDialog.open() } }
    }
    Menu {                                            // Equipo, Carpeta personal, Papelera, Red
        id: cajaSpecialMenu
        width: 260
        readonly property bool trash: root.repeaterItem(root.menuRow) !== null && root.repeaterItem(root.menuRow).fileName === "trash"
        MItem { text: root.ct("_Open"); font.bold: true; onTriggered: dm.open(root.menuRow) }
        MSep { visible: cajaSpecialMenu.trash }
        MItem {
            text: root.ct("E_mpty Trash")
            visible: cajaSpecialMenu.trash
            enabled: cajaSpecialMenu.visible && dm.trashCount() > 0
            onTriggered: root.confirm(root.ct("Empty all items from Trash?"), function () { dm.emptyTrash() })
        }
    }

    // ── Ventanas ──
    property var confirmAction: null
    function confirm(text, action) {
        confirmDialog.text = text
        confirmAction = action
        confirmDialog.open()
    }

    Dialog {
        id: confirmDialog
        property string text
        anchors.centerIn: Overlay.overlay
        modal: true
        title: "Confirmar"
        width: 420
        Label { width: parent.width; text: confirmDialog.text; wrapMode: Text.Wrap }
        footer: DialogButtonBox {
            Button { text: "Sí"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
            Button { text: "No"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        onAccepted: { if (root.confirmAction) root.confirmAction(); root.confirmAction = null; desk.forceActiveFocus() }
        onRejected: { root.confirmAction = null; desk.forceActiveFocus() }
    }

    Dialog {
        id: propsDialog
        property var info: ({})
        anchors.centerIn: Overlay.overlay
        modal: true
        title: "Propiedades"
        width: 500
        ColumnLayout {
            width: parent.width
            spacing: 10
            RowLayout {
                spacing: 12
                Image { source: propsDialog.info.icon ? "image://icono/" + propsDialog.info.icon : ""; sourceSize: Qt.size(48, 48) }
                Label { text: propsDialog.info.name || ""; font.bold: true; font.pixelSize: 16; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
            GridLayout {
                columns: 2
                columnSpacing: 14
                rowSpacing: 6
                Layout.fillWidth: true
                Label { text: "Tipo:"; opacity: 0.7 }
                Label { text: propsDialog.info.type || ""; wrapMode: Text.Wrap; Layout.fillWidth: true }
                Label { text: "Tamaño:"; opacity: 0.7 }
                Label { text: propsDialog.info.size || ""; Layout.fillWidth: true }
                Label { text: "Ubicación:"; opacity: 0.7 }
                Label { text: propsDialog.info.location || ""; wrapMode: Text.WrapAnywhere; Layout.fillWidth: true }
                Label { text: "Destino / orden:"; opacity: 0.7; visible: !!propsDialog.info.target }
                Label { text: propsDialog.info.target || ""; visible: !!propsDialog.info.target; wrapMode: Text.WrapAnywhere; Layout.fillWidth: true }
                Label { text: "Modificado:"; opacity: 0.7 }
                Label { text: propsDialog.info.modified || ""; wrapMode: Text.Wrap; Layout.fillWidth: true }
                Label { text: "Último acceso:"; opacity: 0.7 }
                Label { text: propsDialog.info.accessed || ""; wrapMode: Text.Wrap; Layout.fillWidth: true }
                Label { text: "Propietario:"; opacity: 0.7 }
                Label { text: propsDialog.info.owner || "" }
                Label { text: "Permisos:"; opacity: 0.7 }
                Label { text: propsDialog.info.permissions || ""; font.family: "monospace" }
            }
        }
        footer: DialogButtonBox {
            Button { text: "Cerrar"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        onClosed: desk.forceActiveFocus()
    }

    Dialog {
        id: launcherDialog
        anchors.centerIn: Overlay.overlay
        modal: true
        title: "Crear lanzador"
        width: 460
        onOpened: { lName.text = ""; lCmd.text = ""; lIcon.text = ""; lTerm.checked = false; lName.forceActiveFocus() }
        GridLayout {
            width: parent.width
            columns: 2
            columnSpacing: 12
            rowSpacing: 8
            Label { text: "Nombre:" }
            TextField { id: lName; Layout.fillWidth: true; placeholderText: "Mi programa" }
            Label { text: "Orden:" }
            TextField { id: lCmd; Layout.fillWidth: true; placeholderText: "firefox" }
            Label { text: "Icono:" }
            RowLayout {
                Layout.fillWidth: true
                TextField { id: lIcon; Layout.fillWidth: true; placeholderText: "nombre del icono o ruta" }
                Image { source: "image://icono/" + (lIcon.text !== "" ? lIcon.text : "application-x-executable"); sourceSize: Qt.size(32, 32) }
            }
            Item { width: 1 }
            CheckBox { id: lTerm; text: "Ejecutar en una terminal" }
        }
        footer: DialogButtonBox {
            Button { text: "Crear"; enabled: lName.text.trim() !== "" && lCmd.text.trim() !== ""; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
            Button { text: "Cancelar"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        onAccepted: { if (!dm.createLauncher(lName.text, lCmd.text, lIcon.text, lTerm.checked)) root.say("No se pudo crear el lanzador") }
        onClosed: desk.forceActiveFocus()
    }

    // Preferencias del escritorio: las mismas pestañas y opciones (y el mismo archivo) que las de pcmanfm-qt.
    // «Fondo» y «Presentación» las lleva NekoWall.
    Dialog {
        id: prefsDialog
        anchors.centerIn: Overlay.overlay
        modal: true
        title: "Preferencias del escritorio"
        width: Math.min(720, root.width - 40)
        height: Math.min(680, root.height - 40)
        property var p: ({})
        readonly property var sizes: [16, 24, 32, 48, 64, 96, 128]
        onOpened: {
            p = dm.prefs
            tabs.currentIndex = 0
            pIcon.currentIndex = Math.max(0, sizes.indexOf(p.iconSize))
            pFont.model = dm.fontFamilies()
            pFont.currentIndex = Math.max(0, pFont.find(p.fontFamily))
            pFontSize.value = p.fontSize
            pFg.text = p.fgColor
            pShadowOn.checked = p.shadowOn
            pShadow.text = p.shadowColor
            pLock.checked = p.marginW === p.marginH
            pMw.value = p.marginW; pMh.value = p.marginH
            pWl.value = p.workLeft; pWt.value = p.workTop; pWr.value = p.workRight; pWb.value = p.workBottom
            pDefFm.checked = p.defaultFm
            pSticky.checked = p.allSticky
            pHome.checked = p.shortcuts.indexOf("Home") >= 0
            pTrash.checked = p.shortcuts.indexOf("Trash") >= 0
            pComputer.checked = p.shortcuts.indexOf("Computer") >= 0
            pNetwork.checked = p.shortcuts.indexOf("Network") >= 0
            pNoTips.checked = !p.tooltips
            pHidden.checked = p.showHidden
            pSingle.checked = p.singleClick
        }
        component Swatch: Rectangle {
            property string hex
            signal picked(string hex)
            width: 20; height: 20; radius: 3
            color: hex
            border.color: Qt.rgba(0.5, 0.5, 0.5, 0.8)
            MouseArea { anchors.fill: parent; onClicked: parent.picked(parent.hex) }
        }
        component Group: GroupBox { Layout.fillWidth: true }

        header: ColumnLayout {
            spacing: 0
            Label { text: prefsDialog.title; font.bold: true; padding: 10 }
            TabBar {
                id: tabs
                Layout.fillWidth: true
                TabButton { text: "General" }
                TabButton { text: "Fondo" }
                TabButton { text: "Avanzado" }
            }
        }

        StackLayout {
            anchors.fill: parent
            currentIndex: tabs.currentIndex

            // General
            ScrollView {
                contentWidth: availableWidth
                ColumnLayout {
                    width: parent.width
                    spacing: 10
                    Group {
                        title: "Iconos"
                        RowLayout {
                            anchors.fill: parent
                            Label { text: "Tamaño de los iconos:" }
                            ComboBox { id: pIcon; model: prefsDialog.sizes.map(s => s + " × " + s); Layout.fillWidth: true }
                        }
                    }
                    Group {
                        title: "Texto de los nombres"
                        GridLayout {
                            anchors.fill: parent
                            columns: 2
                            Label { text: "Letra:" }
                            RowLayout {
                                Layout.fillWidth: true
                                ComboBox { id: pFont; Layout.fillWidth: true }
                                SpinBox { id: pFontSize; from: 5; to: 72 }
                            }
                            Label { text: "Color del texto:" }
                            RowLayout {
                                TextField { id: pFg; Layout.preferredWidth: 90 }
                                Rectangle { width: 22; height: 22; radius: 3; color: pFg.text; border.color: "gray" }
                                Repeater {
                                    model: ["#ffffff", "#000000", "#e0e0e0", "#ffd54f", "#80deea", "#f48fb1"]
                                    Swatch { required property string modelData; hex: modelData; onPicked: (h) => pFg.text = h }
                                }
                            }
                            CheckBox { id: pShadowOn; text: "Color de la sombra:" }
                            RowLayout {
                                enabled: pShadowOn.checked
                                TextField { id: pShadow; Layout.preferredWidth: 90 }
                                Rectangle { width: 22; height: 22; radius: 3; color: pShadow.text; border.color: "gray" }
                                Repeater {
                                    model: ["#000000", "#333333", "#ffffff", "#1a237e"]
                                    Swatch { required property string modelData; hex: modelData; onPicked: (h) => pShadow.text = h }
                                }
                            }
                        }
                    }
                    Group {
                        title: "Espaciado"
                        ColumnLayout {
                            anchors.fill: parent
                            RowLayout {
                                Label { text: "Márgenes mínimos de los elementos:" }
                                SpinBox { id: pMw; from: 0; to: 48; onValueModified: if (pLock.checked) pMh.value = value }
                                Label { text: "×" }
                                SpinBox { id: pMh; from: 0; to: 48; onValueModified: if (pLock.checked) pMw.value = value }
                                CheckBox { id: pLock; text: "Bloquear"; onToggled: if (checked) pMh.value = pMw.value }
                            }
                            Label { text: "Márgenes del área de trabajo:" }
                            GridLayout {                    // en cruz, como pcmanfm-qt
                                columns: 3
                                Layout.alignment: Qt.AlignHCenter
                                Item { width: 1 }
                                SpinBox { id: pWt; from: 0; to: 200 }
                                Item { width: 1 }
                                SpinBox { id: pWl; from: 0; to: 200 }
                                Item { width: 1 }
                                SpinBox { id: pWr; from: 0; to: 200 }
                                Item { width: 1 }
                                SpinBox { id: pWb; from: 0; to: 200 }
                                Item { width: 1 }
                            }
                        }
                    }
                    CheckBox { id: pDefFm; text: "Abrir las carpetas con el gestor de archivos predeterminado" }
                    CheckBox { id: pSticky; text: "Fijar todos los elementos en su posición" }
                    CheckBox { id: pSingle; text: "Abrir con un solo clic" }
                }
            }

            // Fondo: lo pone NekoWall
            ColumnLayout {
                spacing: 12
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: "El fondo del escritorio (imágenes y vídeos animados) lo pone NekoWall."
                }
                Button { text: "Abrir NekoWall"; onClicked: { prefsDialog.close(); dm.openNekoWall() } }
                Item { Layout.fillHeight: true }
            }

            // Avanzado
            ColumnLayout {
                spacing: 10
                Group {
                    title: "Accesos visibles"
                    ColumnLayout {
                        anchors.fill: parent
                        CheckBox { id: pHome; text: "Carpeta personal"; icon.source: "image://icono/user-home" }
                        CheckBox { id: pTrash; text: "Papelera"; icon.source: "image://icono/user-trash" }
                        CheckBox { id: pComputer; text: "Equipo"; icon.source: "image://icono/computer" }
                        CheckBox { id: pNetwork; text: "Red"; icon.source: "image://icono/folder-network" }
                    }
                }
                Group {
                    title: "Escritorio"
                    RowLayout {
                        anchors.fill: parent
                        Label { text: "Carpeta del escritorio:" }
                        TextField { text: prefsDialog.p.folder || ""; readOnly: true; Layout.fillWidth: true }
                    }
                }
                CheckBox { id: pNoTips; text: "No mostrar descripciones de los archivos" }
                CheckBox { id: pHidden; text: "Mostrar archivos ocultos" }
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    opacity: 0.65
                    font.pixelSize: 11
                    text: "Se guardan en " + dm.settingsFile + " (el perfil «" + (prefsDialog.p.profile || "") + "» del escritorio de pcmanfm-qt)."
                }
                Item { Layout.fillHeight: true }
            }
        }
        footer: DialogButtonBox {
            Button { text: "Aceptar"; DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
            Button { text: "Cancelar"; DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
        }
        onAccepted: {
            const sc = []
            if (pHome.checked) sc.push("Home")
            if (pTrash.checked) sc.push("Trash")
            if (pComputer.checked) sc.push("Computer")
            if (pNetwork.checked) sc.push("Network")
            dm.savePrefs({
                iconSize: sizes[pIcon.currentIndex], fontFamily: pFont.currentText, fontSize: pFontSize.value,
                fgColor: pFg.text, shadowOn: pShadowOn.checked, shadowColor: pShadow.text,
                marginW: pMw.value, marginH: pMh.value,
                workLeft: pWl.value, workTop: pWt.value, workRight: pWr.value, workBottom: pWb.value,
                showHidden: pHidden.checked, singleClick: pSingle.checked, tooltips: !pNoTips.checked,
                allSticky: pSticky.checked, defaultFm: pDefFm.checked, shortcuts: sc
            })
        }
        onClosed: desk.forceActiveFocus()
    }

    // Aviso breve (abajo a la izquierda)
    Rectangle {
        visible: root.toast !== ""
        anchors { left: parent.left; bottom: parent.bottom; margins: 18 }
        radius: 8
        color: Qt.rgba(0.04, 0.06, 0.09, 0.85)
        width: toastText.implicitWidth + 24
        height: toastText.implicitHeight + 14
        Text { id: toastText; anchors.centerIn: parent; text: root.toast; color: "white"; font.pixelSize: 13 }
    }
}
