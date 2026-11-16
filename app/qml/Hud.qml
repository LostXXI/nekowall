// Selector rápido (HUD cinemático) con dos carruseles en cruz que comparten el centro:
//   · vídeos en horizontal (←/→)   · imágenes estáticas en vertical (↑/↓)
// La flecha que pulses activa su carrusel; Tab alterna. El activo va delante, nítido y con halo
// turquesa; el otro queda detrás y atenuado. Enter aplica, F favorito, Esc cierra.
// Estilo Dark Aero: ventana transparente (KWin desenfoca detrás), paneles de cristal con brillo,
// acento turquesa Miku e ilustración opcional a la derecha.
import QtQuick
import QtQuick.Window
import QtQuick.Effects

Window {
    id: root

    title: "NekoWall"
    // Cristal si el compositor desenfoca (KWin); si no (p. ej. MATE/X11), fondo oscuro opaco.
    color: Bridge.glass ? "transparent" : "#0b0f14"
    visible: false
    width: Screen.width
    height: Screen.height
    // En X11 no hay layer-shell: ventana sin bordes, siempre encima y a pantalla completa.
    flags: Bridge.wayland ? Qt.FramelessWindowHint : (Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint)


    Component.onCompleted: {
        if (Bridge.wayland) {
            show()
        } else {
            showFullScreen()
            raise()
            requestActivate()          // el teclado debe llegar al HUD en X11
        }
    }

    readonly property bool reduced: Bridge.reducedMotion
    readonly property color accent: "#39C5BB"   // turquesa Miku
    readonly property var videos: Bridge.wallpapers.filter(w => w.kind === "video")
    readonly property var images: Bridge.wallpapers.filter(w => w.kind !== "video")

    // "video" | "image": qué carrusel responde al teclado y a Enter.
    property string active: "video"
    readonly property bool videoActive: active === "video"
    readonly property var activeView: videoActive ? hView : vView
    readonly property var activeList: videoActive ? videos : images
    readonly property var selected: activeView.currentIndex >= 0 && activeView.currentIndex < activeList.length
                                    ? activeList[activeView.currentIndex] : null

    property string videoId: ""      // selección de cada carrusel, para conservarla al recargar
    property string imageId: ""
    property bool userMoved: false   // si el usuario ya navegó, no saltar al fondo actual
    // true mientras se recarga la lista: los carruseles vuelven a la tarjeta 1 al recibir la lista
    // nueva y eso no debe sobrescribir la selección guardada (fallo «F lleva a la imagen 1»).
    property bool restoring: false
    property string message: ""
    property bool messageIsError: false
    property bool closing: false

    SystemPalette { id: sysPal; colorGroup: SystemPalette.Active }
    component Glass: Rectangle {
        color: Qt.rgba(0.04, 0.07, 0.10, 0.62)
        border.width: 1
        border.color: Qt.rgba(0.22, 0.77, 0.73, 0.35)
        Rectangle {                          // brillo glossy en la mitad superior
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 1 }
            height: parent.height * 0.5
            radius: parent.radius
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.rgba(1, 1, 1, 0.16) }
                GradientStop { position: 1.0; color: Qt.rgba(1, 1, 1, 0.0) }
            }
        }
    }


    function indexIn(list, id) {
        for (let i = 0; i < list.length; ++i)
            if (list[i].id === id) return i
        return -1
    }
    // Coloca un carrusel en una tarjeta sin animación (al cargar o tras recargar la lista).
    function place(view, list, id) {
        const i = indexIn(list, id)
        if (i < 0) return false
        const d = view.highlightMoveDuration
        view.highlightMoveDuration = 0
        view.currentIndex = i
        view.highlightMoveDuration = d
        return true
    }
    // Al abrir: activa el carrusel del fondo actual y céntralo en él.
    readonly property bool startWithImages: Qt.application.arguments.indexOf("--imagenes") >= 0
    function focusCurrent() {
        const id = Bridge.currentId
        const inVideos = place(hView, videos, id)
        const inImages = place(vView, images, id)
        if (startWithImages && images.length > 0) active = "image"
        else if (inVideos) active = "video"
        else if (inImages) active = "image"
        else active = videos.length > 0 ? "video" : "image"
    }
    function setActive(which) {
        if (which === "video" && videos.length === 0) return
        if (which === "image" && images.length === 0) return
        active = which
    }
    function move(which, step) {
        setActive(which)
        if (active !== which) return
        userMoved = true
        const v = which === "video" ? hView : vView
        step > 0 ? v.incrementCurrentIndex() : v.decrementCurrentIndex()
    }
    function applySelected() {
        if (!selected || Bridge.busy || closing) return
        if (selected.missing) { showMessage("El archivo ya no existe", true); return }
        showMessage(selected.kind === "video" ? "Aplicando vídeo…" : "Aplicando imagen…", false)
        Bridge.apply(selected.id)
    }
    function showMessage(text, isError) {
        message = text
        messageIsError = isError
        messageTimer.restart()
    }
    function close() {
        if (closing) return
        closing = true
        if (reduced) Qt.quit()
        else fadeOut.start()
    }

    Connections {
        target: Bridge
        function onWallpapersAboutToChange() { root.restoring = true }
        function onWallpapersChanged() {
            Qt.callLater(() => {
                if (root.videoId !== "" || root.imageId !== "") {
                    root.place(hView, root.videos, root.videoId)
                    root.place(vView, root.images, root.imageId)
                } else {
                    root.focusCurrent()
                }
                root.restoring = false
            })
        }
        function onStatusChanged() { if (!root.userMoved) root.focusCurrent() }
        function onApplied(ok, id, msg) {
            if (ok) {
                root.showMessage("Fondo aplicado ✓", false)
                closeTimer.start()
            } else {
                root.showMessage("No se pudo aplicar: " + msg, true)
            }
        }
        function onError(msg) { root.showMessage(msg, true) }
    }

    Timer { id: messageTimer; interval: root.messageIsError ? 6000 : 2500; onTriggered: if (!Bridge.busy) root.message = "" }
    Timer { id: closeTimer; interval: root.reduced ? 150 : 450; onTriggered: root.close() }

    Item {
        id: content
        anchors.fill: parent
        opacity: 0
        scale: root.reduced ? 1 : 0.98
        focus: true
        Component.onCompleted: { if (root.reduced) opacity = 1; else fadeIn.start() }

        ParallelAnimation {
            id: fadeIn
            NumberAnimation { target: content; property: "opacity"; to: 1; duration: 160; easing.type: Easing.OutCubic }
            NumberAnimation { target: content; property: "scale"; to: 1; duration: 200; easing.type: Easing.OutCubic }
        }
        NumberAnimation {
            id: fadeOut
            target: content; property: "opacity"; to: 0; duration: 120
            onFinished: Qt.quit()
        }

        // Teclado: la dirección de la flecha elige el carrusel.
        Keys.onLeftPressed: root.move("video", -1)
        Keys.onRightPressed: root.move("video", 1)
        Keys.onUpPressed: root.move("image", -1)
        Keys.onDownPressed: root.move("image", 1)
        Keys.onTabPressed: root.setActive(root.videoActive ? "image" : "video")
        Keys.onBacktabPressed: root.setActive(root.videoActive ? "image" : "video")
        Keys.onReturnPressed: root.applySelected()
        Keys.onEnterPressed: root.applySelected()
        Keys.onEscapePressed: root.close()
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_F && root.selected) {
                Bridge.setFavorite(root.selected.id, !root.selected.favorite)
                e.accepted = true
            } else if (e.key === Qt.Key_Home) {
                root.userMoved = true; root.activeView.currentIndex = 0; e.accepted = true
            } else if (e.key === Qt.Key_End) {
                root.userMoved = true; root.activeView.currentIndex = root.activeView.count - 1; e.accepted = true
            }
        }

        // Fondo: la miniatura seleccionada del carrusel activo, ampliada y desenfocada.
        // Tono del fondo seleccionado: la miniatura a 48 px ampliada ya se ve desenfocada,
        // sin MultiEffect (KWin desenfoca el escritorio de detrás).
        Image {
            anchors.fill: parent
            source: root.selected && root.selected.thumb ? "file://" + root.selected.thumb : ""
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: 48
            smooth: true
            asynchronous: true
            opacity: Bridge.glass ? 0.35 : 0.85   // sin cristal hace de fondo principal
        }
        // Ilustración (Miku) a la derecha: llega ya recortada, fundida y con su opacidad
        // (Bridge::bakedMascot), así que es una sola imagen sin efectos ni capas intermedias.
        Image {
            anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
            width: Math.round(parent.width * 0.62)
            source: Bridge.mascotUrl
            visible: source !== ""
            asynchronous: true
            smooth: true
        }
        Rectangle {                         // velo oscuro para que el texto tenga contraste
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.rgba(0.01, 0.02, 0.04, 0.70) }
                GradientStop { position: 0.5; color: Qt.rgba(0.01, 0.02, 0.04, 0.45) }
                GradientStop { position: 1.0; color: Qt.rgba(0.01, 0.02, 0.04, 0.80) }
            }
        }

        MouseArea {                         // clic fuera de las tarjetas = cerrar; rueda = carrusel activo
            anchors.fill: parent
            onClicked: root.close()
            onWheel: (w) => {
                const d = Math.abs(w.angleDelta.x) > Math.abs(w.angleDelta.y) ? w.angleDelta.x : w.angleDelta.y
                root.move(root.active, d > 0 ? -1 : 1)
            }
        }

        // ── Carrusel vertical: imágenes estáticas ──────────────────────────────
        PathView {
            id: vView
            readonly property bool isActive: !root.videoActive
            readonly property real cardH: Math.round(root.height * 0.26)
            readonly property real cardW: Math.round(cardH * 1.5)
            readonly property real span: root.height * 0.40

            anchors.horizontalCenter: parent.horizontalCenter
            width: cardW * 1.6
            height: parent.height
            z: isActive ? 2 : 1
            model: root.images
            interactive: true
            pathItemCount: Math.max(1, Math.min(5, count % 2 === 1 ? count : count - 1))
            preferredHighlightBegin: 0.5
            preferredHighlightEnd: 0.5
            highlightRangeMode: PathView.StrictlyEnforceRange
            snapMode: PathView.SnapToItem
            highlightMoveDuration: root.reduced ? 0 : 220
            flickDeceleration: 2500
            onMovementStarted: { root.userMoved = true; root.setActive("image") }
            onCurrentIndexChanged: if (!root.restoring && currentIndex >= 0 && currentIndex < root.images.length) root.imageId = root.images[currentIndex].id

            // El inactivo se aplana en una capa para atenuarlo entero sin que sus tarjetas se transparenten.
            opacity: isActive ? 1 : 0.28
            layer.enabled: !isActive
            Behavior on opacity { enabled: !root.reduced; NumberAnimation { duration: 180 } }

            path: Path {
                startX: vView.width / 2; startY: vView.height / 2 - vView.span
                PathAttribute { name: "itemScale"; value: 0.55 }
                PathAttribute { name: "itemZ"; value: 0 }
                PathAttribute { name: "itemOpacity"; value: 0.45 }
                PathLine { x: vView.width / 2; y: vView.height / 2 }
                PathAttribute { name: "itemScale"; value: 1.0 }
                PathAttribute { name: "itemZ"; value: 100 }
                PathAttribute { name: "itemOpacity"; value: 1.0 }
                PathLine { x: vView.width / 2; y: vView.height / 2 + vView.span }
                PathAttribute { name: "itemScale"; value: 0.55 }
                PathAttribute { name: "itemZ"; value: 0 }
                PathAttribute { name: "itemOpacity"; value: 0.45 }
            }

            delegate: HudCard {
                cardW: vView.cardW
                cardH: vView.cardH
                skew: root.reduced ? 0 : 0.22
                accent: root.accent
                carouselActive: vView.isActive
                isCurrentWallpaper: modelData.id === Bridge.currentId
                onPicked: { root.userMoved = true; root.setActive("image"); vView.currentIndex = index }
                onActivated: { if (vView.isActive) root.applySelected(); else root.setActive("image") }
            }
        }

        // ── Carrusel horizontal: vídeos ────────────────────────────────────────
        PathView {
            id: hView
            readonly property bool isActive: root.videoActive
            readonly property real cardH: Math.round(root.height * 0.34)
            readonly property real cardW: Math.round(cardH * 1.08)
            readonly property real span: Math.min(root.width * 0.42, cardW * 2.4)

            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            height: cardH + 40
            z: isActive ? 2 : 1
            model: root.videos
            interactive: true
            pathItemCount: Math.max(1, Math.min(7, count % 2 === 1 ? count : count - 1))
            preferredHighlightBegin: 0.5
            preferredHighlightEnd: 0.5
            highlightRangeMode: PathView.StrictlyEnforceRange
            snapMode: PathView.SnapToItem
            highlightMoveDuration: root.reduced ? 0 : 220
            flickDeceleration: 2500
            onMovementStarted: { root.userMoved = true; root.setActive("video") }
            onCurrentIndexChanged: if (!root.restoring && currentIndex >= 0 && currentIndex < root.videos.length) root.videoId = root.videos[currentIndex].id

            opacity: isActive ? 1 : 0.28
            layer.enabled: !isActive
            Behavior on opacity { enabled: !root.reduced; NumberAnimation { duration: 180 } }

            path: Path {
                startX: hView.width / 2 - hView.span; startY: hView.height / 2
                PathAttribute { name: "itemScale"; value: 0.55 }
                PathAttribute { name: "itemZ"; value: 0 }
                PathAttribute { name: "itemOpacity"; value: 0.45 }
                PathLine { x: hView.width / 2; y: hView.height / 2 }
                PathAttribute { name: "itemScale"; value: 1.0 }
                PathAttribute { name: "itemZ"; value: 100 }
                PathAttribute { name: "itemOpacity"; value: 1.0 }
                PathLine { x: hView.width / 2 + hView.span; y: hView.height / 2 }
                PathAttribute { name: "itemScale"; value: 0.55 }
                PathAttribute { name: "itemZ"; value: 0 }
                PathAttribute { name: "itemOpacity"; value: 0.45 }
            }

            delegate: HudCard {
                cardW: hView.cardW
                cardH: hView.cardH
                skew: root.reduced ? 0 : 0.22
                accent: root.accent
                carouselActive: hView.isActive
                isCurrentWallpaper: modelData.id === Bridge.currentId
                onPicked: { root.userMoved = true; root.setActive("video"); hView.currentIndex = index }
                onActivated: { if (hView.isActive) root.applySelected(); else root.setActive("video") }
            }
        }

        // ── Indicador de carrusel activo (arriba a la izquierda) ──────────────
        Row {
            anchors { left: parent.left; top: parent.top; margins: parent.height * 0.05 }
            spacing: 10
            z: 5
            Repeater {
                model: [
                    { key: "video", label: "▶  Vídeos", hint: "← →", count: root.videos.length },
                    { key: "image", label: "▣  Imágenes", hint: "↑ ↓", count: root.images.length }
                ]
                Skeuo {
                    id: pillPlate
                    readonly property bool on: root.active === modelData.key
                    height: 40
                    width: pill.implicitWidth + 32
                    radius: height / 2
                    visible: modelData.count > 0
                    lit: on
                    litColor: root.accent
                    Text {
                        id: pill
                        anchors.centerIn: parent
                        color: pillPlate.textColor
                        style: Text.Raised
                        styleColor: pillPlate.textShadow
                        font.pixelSize: 16
                        font.weight: pillPlate.on ? Font.Bold : Font.DemiBold
                        text: modelData.label + "  " + modelData.count + "   " + modelData.hint
                    }
                    MouseArea { anchors.fill: parent; onClicked: root.setActive(modelData.key) }
                    Accessible.role: Accessible.Button
                    Accessible.name: modelData.label + (on ? ", activo" : "")
                }
            }
        }

        // ── Datos del fondo seleccionado (abajo, sobre la ayuda de teclado) ───
        Glass {
            id: info
            z: 5
            visible: root.selected !== null
            // Abajo a la izquierda: así no tapa ninguno de los dos carruseles del centro.
            anchors { left: parent.left; leftMargin: parent.height * 0.05; bottom: hints.top; bottomMargin: 18 }
            width: Math.min(infoCol.implicitWidth + 48, parent.width * 0.36)
            height: infoCol.implicitHeight + 20
            radius: 12
            Column {
                id: infoCol
                anchors.centerIn: parent
                width: Math.min(implicitWidth, root.width * 0.36 - 48)
                spacing: 4
                Text {
                    width: Math.min(implicitWidth, root.width * 0.36 - 48)
                    elide: Text.ElideMiddle
                    color: "white"
                    font.pixelSize: 22
                    font.weight: Font.DemiBold
                    text: root.selected ? root.selected.name : ""
                }
                Text {
                    color: "#d6dde4"
                    font.pixelSize: 15
                    text: !root.selected ? "" :
                          (root.videoActive ? "Vídeo " : "Imagen ") + (root.activeView.currentIndex + 1) + " de " + root.activeList.length
                          + (root.selected.size ? "  ·  " + Bridge.formatSize(root.selected.size) : "")
                          + (root.selected.favorite ? "  ·  ★ Favorito" : "")
                          + (root.selected.id === Bridge.currentId ? "  ·  ✓ Actual" : "")
                          + (root.selected.kind === "video" && !Bridge.videoSupported ? "  ·  vídeo no disponible en este escritorio" : "")
                }
            }
        }

        // Estados vacíos
        Text {
            anchors.centerIn: parent
            z: 5
            width: parent.width * 0.6
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: "white"
            font.pixelSize: 18
            visible: Bridge.wallpapers.length === 0
            text: Bridge.loaded
                  ? "La biblioteca está vacía.\nImporta fondos desde la ventana de NekoWall."
                  : "Cargando biblioteca…"
        }

        // Mensaje de estado (aplicando / aplicado / error)
        Rectangle {
            id: msgBox
            z: 5
            anchors { horizontalCenter: parent.horizontalCenter; bottom: hints.top; bottomMargin: 18 }
            visible: root.message !== ""
            radius: height / 2
            height: msg.implicitHeight + 16
            width: Math.min(msg.implicitWidth + 40, parent.width - 64)
            color: root.messageIsError ? "#7a1f24" : Qt.rgba(0, 0, 0, 0.7)
            border.color: root.messageIsError ? "#e0575e" : Qt.rgba(1, 1, 1, 0.15)
            Row {
                anchors.centerIn: parent
                spacing: 8
                Rectangle {                 // indicador de actividad
                    width: 10; height: 10; radius: 5
                    anchors.verticalCenter: parent.verticalCenter
                    color: root.accent
                    visible: Bridge.busy
                    SequentialAnimation on opacity {
                        running: Bridge.busy && !root.reduced
                        loops: Animation.Infinite
                        NumberAnimation { to: 0.2; duration: 400 }
                        NumberAnimation { to: 1.0; duration: 400 }
                    }
                }
                Text { id: msg; color: "white"; font.pixelSize: 15; text: root.message; elide: Text.ElideRight }
            }
        }

        // Ayuda de teclado
        Row {
            id: hints
            z: 5
            anchors { bottom: parent.bottom; bottomMargin: parent.height * 0.04; horizontalCenter: parent.horizontalCenter }
            spacing: 22
            Repeater {
                model: [["← →", "Vídeos"], ["↑ ↓", "Imágenes"], ["Tab", "Cambiar"], ["Enter", "Aplicar"],
                        ["F", "Favorito"], ["Esc", "Cerrar"]]
                Row {
                    spacing: 8
                    Skeuo {                  // tecla física (como las teclas de atajo de SkeuoCord)
                        id: keyPlate
                        height: 26
                        width: Math.max(32, keyLabel.implicitWidth + 18)
                        radius: 5
                        Text { id: keyLabel; anchors.centerIn: parent; anchors.verticalCenterOffset: -1; text: modelData[0]
                               color: keyPlate.textColor; style: Text.Raised; styleColor: keyPlate.textShadow
                               font.pixelSize: 13; font.weight: Font.Bold }
                    }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: modelData[1]; color: "#e6ebf0"; font.pixelSize: 14
                           style: Text.Raised; styleColor: Qt.rgba(0, 0, 0, 0.7) }
                }
            }
        }
    }
}
