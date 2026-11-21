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
#include <QLocalServer>
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

    // Selector rápido de una sola instancia: si ya hay uno abierto, se le pide que se cierre y este
    // proceso termina sin abrir otra ventana. Así, pulsar Meta+Alt+W muchas veces seguidas lo abre y lo
    // cierra en vez de amontonar selectores hasta llenar la memoria.
    QLocalServer *hudServer = nullptr;
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
        if (toggleExisting())
            return 0;
        hudServer = new QLocalServer(&app);
        hudServer->setSocketOptions(QLocalServer::UserAccessOption);
        if (!hudServer->listen(name)) {
            // Otro selector acaba de abrirse a la vez (pulsaciones muy seguidas) o quedó un socket viejo.
            if (toggleExisting())
                return 0;
            QLocalServer::removeServer(name);
            hudServer->listen(name);
        }
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
    }

    Bridge bridge;
    Workshop workshop;   // no se conecta a nada hasta que se abre la página Workshop
    CachedNetworkFactory netFactory;
    QQmlApplicationEngine engine;
    engine.setNetworkAccessManagerFactory(&netFactory);
    engine.rootContext()->setContextProperty(QStringLiteral("Bridge"), &bridge);
    engine.rootContext()->setContextProperty(QStringLiteral("Workshop"), &workshop);

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
        QObject::connect(hudServer, &QLocalServer::newConnection, &app, [hudServer, &engine] {
            while (QLocalSocket *c = hudServer->nextPendingConnection())
                QObject::connect(c, &QLocalSocket::disconnected, c, &QObject::deleteLater);
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
    return app.exec();
}
