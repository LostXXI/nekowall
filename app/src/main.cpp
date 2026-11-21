// nekowall-gui            → ventana principal (biblioteca y ajustes)
// nekowall-gui --hud      → selector rápido a pantalla completa (--hud --imagenes: empieza en imágenes)
// nekowall-gui --import <ruta>...  → ventana principal e importa esas rutas (p. ej. «Abrir con» desde Dolphin)
// nekowall-gui --link=<enlace>     → Workshop con ese enlace analizado en «Pegar enlace»
// NEKOWALL_TIMING=1 imprime en stderr cuánto tarda en aparecer el primer fotograma.

#include "Bridge.h"
#include "Workshop.h"

#include <QDateTime>
#include <QElapsedTimer>
#include <QApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QIcon>
#include <QPalette>
#include <QStyleHints>
#include <QLocalServer>
#include <QLockFile>
#include <QTimer>
#include <utility>
#include <QDir>
#include <QStandardPaths>
#include <QLocalSocket>
#include <unistd.h>
#include <QFileInfo>
#ifdef HAVE_KWINDOWEFFECTS
#include <KWindowEffects>
#endif
#include <cstdio>
#include <memory>

int main(int argc, char *argv[])
{
    QElapsedTimer clock;
    clock.start();

    QApplication app(argc, argv);  // QApplication: el estilo Breeze (org.kde.desktop) usa QStyle
    app.setApplicationName(QStringLiteral("nekowall"));
    app.setOrganizationName(QStringLiteral("nekowall"));   // sin esto QSettings no lee ni guarda nada
    app.setApplicationDisplayName(QStringLiteral("NekoWall"));
    app.setDesktopFileName(QStringLiteral("nekowall"));
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("nekowall"), QIcon::fromTheme(QStringLiteral("preferences-desktop-wallpaper"))));

    const bool hud = app.arguments().contains(QStringLiteral("--hud"));
    bool darkFallback = false;   // paleta oscura propia (fuera de KDE, si el sistema no da una oscura)

    // Selector rápido de una sola instancia: si ya hay uno abierto, se le pide que se cierre y este
    // proceso termina sin abrir otra ventana. Así, pulsar Meta+Alt+W muchas veces seguidas lo abre y lo
    // cierra en vez de amontonar selectores hasta llenar la memoria.
    QLocalServer *hudServer = nullptr;
    const bool hudDebug = qEnvironmentVariableIsSet("NEKOWALL_HUD_DEBUG");
    auto hudLog = [hudDebug](const char *msg) {
        if (hudDebug) std::fprintf(stderr, "[hud %lld %d] %s\n", (long long)QDateTime::currentMSecsSinceEpoch() % 100000, (int)getpid(), msg);
    };
    if (hud) {
        const QString name = QStringLiteral("nekowall-hud-%1").arg(getuid());
        auto toggleExisting = [&name] {
            QLocalSocket s;
            s.connectToServer(name);
            if (!s.waitForConnected(200))
                return false;
            s.write("cerrar\n");
            s.waitForBytesWritten(200);
            return true;
        };
        // Candado: las pulsaciones muy seguidas deciden de una en una (si no, dos procesos que arrancan a la
        // vez podían no verse y el estado final no coincidía con el número de pulsaciones).
        QString lockDir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
        if (lockDir.isEmpty())
            lockDir = QDir::tempPath();
        QLockFile turn(lockDir + QStringLiteral("/nekowall-hud.lock"));
        turn.setStaleLockTime(3000);
        turn.tryLock(3000);
        if (toggleExisting()) {
            hudLog("había uno abierto: le pido que se cierre");
            return 0;
        }
        hudServer = new QLocalServer(&app);
        hudServer->setSocketOptions(QLocalServer::UserAccessOption);
        if (!hudServer->listen(name)) {
            // Otro selector acaba de abrirse a la vez (pulsaciones muy seguidas) o quedó un socket viejo.
            if (toggleExisting()) {
                hudLog("carrera: otro acababa de abrirse, le pido que se cierre");
                return 0;
            }
            QLocalServer::removeServer(name);
            hudServer->listen(name);
        }
        hudLog("abro el selector");
        turn.unlock();   // ya escucha: la siguiente pulsación lo encontrará
    }
    // Estilo Breeze de KDE para los controles, salvo que el usuario elija otro.
    // Estilo de los controles: Breeze en KDE si su módulo QML está instalado (kf6-qqc2-desktop-style);
    // en cualquier otro caso Fusion, que viene con Qt. Sin esta comprobación, fuera de KDE la ventana no
    // abre («module "org.kde.desktop" is not installed»).
    if (!hud && qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        const bool kde = qEnvironmentVariable("XDG_CURRENT_DESKTOP").contains(QStringLiteral("KDE"), Qt::CaseInsensitive);
        bool breeze = false;
        if (kde) {
            QQmlEngine probe;
            for (const QString &dir : probe.importPathList())
                if (QFileInfo::exists(dir + QStringLiteral("/org/kde/desktop/qmldir")))
                    breeze = true;
        }
        QQuickStyle::setStyle(breeze ? QStringLiteral("org.kde.desktop") : QStringLiteral("Fusion"));
        // Fuera de KDE (Fusion): modo oscuro salvo que el sistema pida claro. En niri, por ejemplo, Qt no
        // sabe el modo del sistema y caía en la paleta clara (todo blanco). Si el escritorio ya da una
        // paleta oscura (MATE con tema oscuro), se respeta.
        if (!breeze && qEnvironmentVariableIsEmpty("NEKOWALL_LIGHT")) {
            const bool lightAsked = QGuiApplication::styleHints()->colorScheme() == Qt::ColorScheme::Light;
            const bool alreadyDark = QGuiApplication::palette().color(QPalette::Window).lightness() < 128;
            if (!lightAsked && !alreadyDark) {
                darkFallback = true;
                const QColor win(0x1c, 0x20, 0x26), base(0x14, 0x17, 0x1c), btn(0x26, 0x2b, 0x33);
                const QColor text(0xe6, 0xe9, 0xed), dim(0x6b, 0x72, 0x7c), accent(0x39, 0xc5, 0xbb);
                QPalette p;
                p.setColor(QPalette::Window, win);
                p.setColor(QPalette::WindowText, text);
                p.setColor(QPalette::Base, base);
                p.setColor(QPalette::AlternateBase, win);
                p.setColor(QPalette::ToolTipBase, btn);
                p.setColor(QPalette::ToolTipText, text);
                p.setColor(QPalette::PlaceholderText, QColor(0x8a, 0x91, 0x9a));
                p.setColor(QPalette::Text, text);
                p.setColor(QPalette::Button, btn);
                p.setColor(QPalette::ButtonText, text);
                p.setColor(QPalette::BrightText, QColor(0xff, 0x8a, 0x80));
                p.setColor(QPalette::Link, accent);
                p.setColor(QPalette::Highlight, accent);
                p.setColor(QPalette::HighlightedText, QColor(0x0b, 0x0f, 0x14));
                p.setColor(QPalette::Light, QColor(0x34, 0x3a, 0x44));
                p.setColor(QPalette::Midlight, QColor(0x2c, 0x32, 0x3b));
                p.setColor(QPalette::Mid, QColor(0x22, 0x27, 0x2e));
                p.setColor(QPalette::Dark, QColor(0x10, 0x12, 0x16));
                p.setColor(QPalette::Shadow, Qt::black);
                for (auto role : {QPalette::WindowText, QPalette::Text, QPalette::ButtonText})
                    p.setColor(QPalette::Disabled, role, dim);
                p.setColor(QPalette::Disabled, QPalette::Highlight, QColor(0x3a, 0x40, 0x48));
                QApplication::setPalette(p);   // QApplication: si no, su estilo la pisa al cargarse
            }
        }
    }

    Bridge bridge;
    Workshop workshop;   // no se conecta a nada hasta que se abre la página Workshop
    CachedNetworkFactory netFactory;
    QQmlApplicationEngine engine;
    engine.setNetworkAccessManagerFactory(&netFactory);
    engine.rootContext()->setContextProperty(QStringLiteral("Bridge"), &bridge);
    engine.rootContext()->setContextProperty(QStringLiteral("Workshop"), &workshop);
    // Los controles de Qt Quick toman la paleta de la ventana, no la de la aplicación: Main.qml la aplica.
    engine.rootContext()->setContextProperty(QStringLiteral("nekoDark"), darkFallback);

    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    bridge.refresh();
    // HUD: con layer-shell en Wayland si el módulo existe; si no (X11, p. ej. MATE), el HUD normal.
    QString root = QStringLiteral("Main");
    if (hud) {
        bool layerShell = false;
        if (QGuiApplication::platformName().startsWith(QStringLiteral("wayland"))) {
            for (const QString &dir : engine.importPathList())
                if (QFileInfo::exists(dir + QStringLiteral("/org/kde/layershell/qmldir")))
                    layerShell = true;
        }
        root = layerShell ? QStringLiteral("HudWayland") : QStringLiteral("Hud");
    }
    engine.loadFromModule("NekoWall", root);

    // Otra pulsación del atajo mientras el selector está abierto: se cierra (con su animación).
    if (hudServer) {
        // Mientras se cierra (animación de 0,15 s) ya no responde: la pulsación siguiente abre otro.
        QObject::connect(&bridge, &Bridge::hudClosingStarted, hudServer, [hudServer, hudLog] {
            hudLog("me cierro: dejo de atender el atajo");
            hudServer->close();   // también borra su socket
        });
        // Cada pulsación es «abrir/cerrar». Las que llegan muy juntas se cuentan todas antes de decidir
        // (se espera 80 ms por si llega otra): un número impar cierra y uno par deja el selector como está.
        auto pending = std::make_shared<int>(0);
        auto settle = new QTimer(&app);
        settle->setSingleShot(true);
        settle->setInterval(80);
        QObject::connect(hudServer, &QLocalServer::newConnection, &app, [hudServer, pending, settle] {
            while (QLocalSocket *c = hudServer->nextPendingConnection()) {
                ++*pending;
                QObject::connect(c, &QLocalSocket::disconnected, c, &QObject::deleteLater);
            }
            settle->start();
        });
        QObject::connect(settle, &QTimer::timeout, &app, [pending, &engine, hudLog] {
            const int n = std::exchange(*pending, 0);
            hudLog(n % 2 ? "me piden cerrar" : "pulsaciones pares juntas: sigo abierto");
            if (n % 2 == 0)
                return;
            if (QObject *w = engine.rootObjects().value(0))
                QMetaObject::invokeMethod(w, "close");
        });
    }

#ifdef HAVE_KWINDOWEFFECTS
    // HUD de cristal: KWin desenfoca lo que hay detrás de la ventana transparente.
    // La ventana ya se mostró en Component.onCompleted: pedirlo ahora y tras el primer fotograma.
    if (hud) {
        if (auto *w = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0))) {
            KWindowEffects::enableBlurBehind(w, true);
            auto once = std::make_shared<bool>(false);
            QObject::connect(w, &QQuickWindow::frameSwapped, w, [w, once] {
                if (!*once) { *once = true; KWindowEffects::enableBlurBehind(w, true); }
            });
        }
    }
#endif

    const QStringList args = app.arguments();
    const qsizetype imp = args.indexOf(QStringLiteral("--import"));
    if (!hud && imp >= 0) {
        QVariantList paths;
        for (qsizetype i = imp + 1; i < args.size() && !args[i].startsWith(QStringLiteral("--")); ++i)
            paths << args[i];
        QMetaObject::invokeMethod(&bridge, [&bridge, paths] { bridge.importPaths(paths); }, Qt::QueuedConnection);
    }

    if (qEnvironmentVariableIntValue("NEKOWALL_TIMING")) {
        if (auto *win = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0))) {
            QObject::connect(win, &QQuickWindow::frameSwapped, &app, [&clock, win] {
                std::fprintf(stderr, "[timing] primer fotograma: %lld ms desde main (epoch %lld)\n",
                             static_cast<long long>(clock.elapsed()),
                             static_cast<long long>(QDateTime::currentMSecsSinceEpoch()));
                QObject::disconnect(win, &QQuickWindow::frameSwapped, nullptr, nullptr);
            });
        }
    }
    const int r = app.exec();
    hudLog("terminé");
    return r;
}
