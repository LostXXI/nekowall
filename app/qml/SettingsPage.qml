// Ajustes: solo opciones que funcionan hoy. Lo pendiente se nombra como pendiente.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

ScrollView {
    id: page
    contentWidth: availableWidth
    readonly property bool videoDesktop: Bridge.backend === "plasma" || Bridge.backend === "mate" || Bridge.backend === "niri"
    clip: true

    component SectionTitle: Label {
        Layout.topMargin: 18
        font.pixelSize: 20
        font.weight: Font.DemiBold
    }
    component Hint: Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        opacity: 0.75
    }
    component KeyCap: Rectangle {
        property alias text: keyText.text
        radius: 6
        implicitWidth: keyText.implicitWidth + 16
        implicitHeight: keyText.implicitHeight + 8
        color: Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.1)
        border.color: Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.25)
        Label { id: keyText; anchors.centerIn: parent; font.weight: Font.DemiBold }
    }
    // Aviso destacado (borde y fondo ámbar): para limitaciones que el usuario debe conocer.
    component Warning: Rectangle {
        property alias text: warnText.text
        Layout.fillWidth: true
        implicitHeight: warnRow.implicitHeight + 20
        radius: 8
        color: Qt.rgba(0.95, 0.65, 0.15, 0.12)
        border.color: Qt.rgba(0.95, 0.65, 0.15, 0.7)
        RowLayout {
            id: warnRow
            anchors { fill: parent; margins: 10 }
            spacing: 10
            Label { text: "⚠"; font.pixelSize: 20; color: "#f0a020"; Layout.alignment: Qt.AlignTop }
            Label { id: warnText; Layout.fillWidth: true; wrapMode: Text.Wrap }
        }
    }
    component PathLabel: TextField {
        Layout.fillWidth: true
        readOnly: true
        selectByMouse: true
    }

    ColumnLayout {
        width: Math.min(page.availableWidth - 8, 860)
        spacing: 8

        Label { text: "Ajustes"; font.pixelSize: 26; font.weight: Font.DemiBold }

        SectionTitle { text: "Selector rápido (HUD)" }
        Hint { text: "Carrusel a pantalla completa para cambiar de fondo al instante. ←/→ para moverte, Enter para aplicar, Esc para cerrar." }
        RowLayout {
            spacing: 10
            Label { text: "Atajo de teclado:" }
            KeyCap { text: Bridge.shortcut }
            Button {
                text: "Registrar atajo"
                icon.name: "input-keyboard"
                visible: Bridge.backend === "mate" || Bridge.backend === "niri"
                onClicked: Bridge.backend === "niri" ? Bridge.registerNiriShortcut() : Bridge.registerMateShortcut()
            }
            Button {
                text: Bridge.backend === "mate" ? "Cambiar en Atajos de teclado…"
                    : Bridge.backend === "niri" ? "Abrir nekowall.kdl…" : "Cambiar en Ajustes del sistema…"
                icon.name: "configure-shortcuts"
                onClicked: Bridge.openShortcutSettings()
            }
            Button { text: "Abrir ahora"; icon.name: "view-preview"; onClicked: Bridge.openHud() }
        }
        RowLayout {
            spacing: 10
            visible: Bridge.backend === "mate"
            Label { text: "Mostrar los iconos del escritorio (con vídeo de fondo):" }
            KeyCap { text: "Meta+Alt+D" }
            Label { text: "(se registra con «Registrar atajo»)"; opacity: 0.7 }
        }

        RowLayout {
            spacing: 10
            CheckBox {
                text: "Mostrar ilustración en el selector"
                checked: Bridge.showMascot
                onToggled: Bridge.showMascot = checked
            }
            Button { text: "Elegir imagen…"; icon.name: "insert-image"; onClicked: mascotDialog.open() }
        }
        Hint { text: "Se muestra a la derecha del selector, fundida con el fondo. Se guarda una copia en ~/.local/share/nekowall/ (no forma parte del programa): úsala solo para uso personal si es arte de otra persona." }
        FileDialog {
            id: mascotDialog
            title: "Elegir ilustración para el selector"
            nameFilters: ["Imágenes (*.png *.jpg *.jpeg *.webp)"]
            onAccepted: Bridge.setMascotImage(selectedFile)
        }

        SectionTitle { text: "Vídeo: pausa y ahorro"; visible: page.videoDesktop }
        Hint {
            visible: !page.videoDesktop && Bridge.backend !== "" && Bridge.backend !== "niri"
            text: "Los fondos de vídeo todavía no están disponibles en este escritorio (" + Bridge.backend + "): por ahora NekoWall aplica imágenes."
        }
        Hint {
            visible: Bridge.backend === "niri"
            text: Bridge.videoSupported
                  ? "En niri los vídeos de fondo los reproduce mpvpaper (uno por pantalla, sin sonido) y NekoWall los pausa según estas opciones (cada pantalla por separado). "
                    + "Debajo queda un fotograma del vídeo, así los colores del sistema (Noctalia) siguen al fondo."
                  : "Para fondos de vídeo en niri instala mpvpaper:  sudo xbps-install -S mpvpaper"
        }
        Hint {
            visible: Bridge.backend === "mate" && !Bridge.videoPluginInstalled
            text: "Para fondos de vídeo en MATE instala mpv y xwinwrap:  doas xbps-install mpv xwinwrap"
        }
        Warning {
            visible: Bridge.backend === "mate" && Bridge.videoPluginInstalled
            text: "<b>En MATE, los iconos del escritorio y un fondo animado no pueden verse a la vez.</b> "
                  + "El escritorio de MATE (Caja) no permite iconos con fondo transparente, así que el vídeo los tapa. "
                  + "Siguen funcionando: el clic derecho del escritorio y el doble clic sobre un icono responden igual. "
                  + "Pulsa <b>Meta+Alt+D</b> para pausar el vídeo y ver los iconos; vuelve a pulsarlo para seguir. "
                  + "Con un fondo de imagen, los iconos se ven siempre."
        }
        Hint {
            visible: Bridge.backend === "plasma" && !Bridge.videoPluginInstalled
            text: "El plugin de vídeo de Plasma (Smart Video Wallpaper Reborn) no está instalado: estas opciones no tendrán efecto."
        }
        GridLayout {
            columns: 2
            columnSpacing: 12
            rowSpacing: 8
            visible: page.videoDesktop
            enabled: Bridge.videoPluginInstalled
            Label { text: "Pausar el vídeo cuando haya:" }
            ComboBox {
                id: pauseBox
                Layout.preferredWidth: 340
                model: ["Una ventana maximizada o a pantalla completa", "Una ventana activa",
                        "Cualquier ventana visible (máximo ahorro)", "Nunca"]
                currentIndex: Bridge.pauseMode
                onActivated: (i) => Bridge.setVideoSettings(i, batteryBox.checked, levelBox.value)
                Accessible.name: "Cuándo pausar el vídeo"
            }
            CheckBox {
                id: batteryBox
                text: "Pausar con batería por debajo del"
                checked: Bridge.batteryPauses
                onToggled: Bridge.setVideoSettings(pauseBox.currentIndex, checked, levelBox.value)
            }
            RowLayout {
                SpinBox {
                    id: levelBox
                    from: 5; to: 100; stepSize: 5
                    value: Bridge.batteryLevel
                    enabled: batteryBox.checked
                    editable: true
                    onValueModified: levelTimer.restart()
                    Accessible.name: "Nivel de batería"
                }
                Label { text: "%" }
            }
        }
        Timer {                                  // no escribir en Plasma a cada clic del SpinBox
            id: levelTimer
            interval: 600
            onTriggered: Bridge.setVideoSettings(pauseBox.currentIndex, batteryBox.checked, levelBox.value)
        }
        Hint { visible: Bridge.backend === "plasma"; text: "Medido en este equipo: con el vídeo en pausa, Plasma pasa de ~17 % a ~3,5 % de CPU." }

        Label { text: "Al importar vídeos más grandes que tu pantalla (" + Bridge.screenWidth + "×" + Bridge.screenHeight + "):"; Layout.topMargin: 8 }
        ComboBox {
            Layout.preferredWidth: 420
            readonly property var values: ["ask", "30", "60", "keep"]
            model: ["Preguntar cada vez",
                    "Convertir a " + Math.min(Bridge.screenHeight, Bridge.screenWidth) + "p · 30 fps",
                    "Convertir a " + Math.min(Bridge.screenHeight, Bridge.screenWidth) + "p · 60 fps (si el original los tiene)",
                    "Importar tal cual"]
            currentIndex: Math.max(0, values.indexOf(Bridge.largeVideoPolicy))
            onActivated: (i) => Bridge.largeVideoPolicy = values[i]
            Accessible.name: "Qué hacer con vídeos grandes al importar"
        }
        Hint { text: "La conversión crea una copia junto al original («nombre (1080p30).mp4») y la biblioteca usa la copia. El original no se toca. También puedes optimizar un vídeo que ya está en la biblioteca: clic derecho → «Optimizar para mi pantalla…»." }

        SectionTitle { text: "Animaciones" }
        Hint {
            text: (Bridge.backend === "plasma"
                   ? "NekoWall sigue la preferencia de KDE «Velocidad de animación» (Ajustes del sistema → Accesibilidad / Comportamiento general). "
                   : "Para quitar las animaciones del selector, inicia NekoWall con NEKOWALL_REDUCED_MOTION=1. ")
                  + "Estado actual: " + (Bridge.reducedMotion
                        ? "animaciones reducidas: el HUD se muestra sin inclinación ni transiciones."
                        : "animaciones activadas.")
        }

        SectionTitle { text: "Escritorio" }
        Hint {
            text: "Backend: " + (Bridge.backend || "detectando…")
                  + (Bridge.backend === "plasma" ? " (KDE Plasma)" : Bridge.backend === "mate" ? " (MATE, X11)"
                     : Bridge.backend === "niri" ? " (niri, Wayland · imágenes con Noctalia o awww)" : "")
                  + " · Fondos de vídeo: " + (Bridge.videoSupported
                        ? (Bridge.backend === "mate" ? "disponibles (mpv + xwinwrap)"
                           : Bridge.backend === "niri" ? "disponibles (mpvpaper)"
                                                     : "disponibles (plugin Smart Video Wallpaper Reborn)")
                        : Bridge.backend === "plasma" ? "no disponibles; instala un plugin de vídeo para Plasma 6"
                                                      : "aún no implementados en este escritorio")
        }

        SectionTitle { text: "Miniaturas" }
        Hint { text: "Se guardan en caché para que la biblioteca y el HUD abran rápido. Regenéralas si alguna se ve mal o desactualizada." }
        PathLabel { text: Bridge.thumbsDir }
        Button {
            text: "Regenerar todas las miniaturas"
            icon.name: "view-refresh"
            enabled: !Bridge.busy
            onClicked: Bridge.regenerateThumbs()
        }

        SectionTitle { text: "Biblioteca" }
        Hint { text: "NekoWall indexa los archivos en su ubicación original: no los copia ni los modifica. Quitar un fondo de la biblioteca no borra el archivo." }
        PathLabel { text: Bridge.dbPath }

        SectionTitle { text: "Pendiente (aún no implementado)" }
        Hint {
            text: "• Rotación automática de fondos\n"
                  + "• Fondos distintos por monitor\n"
                  + "• Carpetas vigiladas e importación/exportación de colecciones\n"
                  + "• Quedarse en la bandeja del sistema para abrir el HUD más rápido"
        }
        Item { Layout.preferredHeight: 20 }
    }
}
