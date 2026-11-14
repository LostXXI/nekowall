// nekowall-gui            → ventana principal (biblioteca y ajustes)
// nekowall-gui --hud      → selector rápido a pantalla completa (--hud --imagenes: empieza en imágenes)
// nekowall-gui --import <ruta>...  → ventana principal e importa esas rutas (p. ej. «Abrir con» desde Dolphin)
// NEKOWALL_TIMING=1 imprime en stderr cuánto tarda en aparecer el primer fotograma.

#include "Bridge.h"

#include <QDateTime>
#include <QElapsedTimer>
#include <QApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QIcon>
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
    // Estilo Breeze de KDE para los controles, salvo que el usuario elija otro.
    if (!hud && qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE"))
        QQuickStyle::setStyle(QStringLiteral("org.kde.desktop"));

    Bridge bridge;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("Bridge"), &bridge);

    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    bridge.refresh();
    engine.loadFromModule("NekoWall", hud ? "Hud" : "Main");

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
