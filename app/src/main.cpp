// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// nekowall-gui            → ventana principal (biblioteca y ajustes)
// nekowall-gui --hud      → selector rápido a pantalla completa (--hud --imagenes: empieza en imágenes)
// nekowall-gui --import <ruta>...  → ventana principal e importa esas rutas (p. ej. «Abrir con» desde Dolphin)
// nekowall-gui --desktop          → iconos del escritorio de NekoWall (Wayland, encima del fondo animado)
// nekowall-gui --link=<enlace>     → Workshop con ese enlace analizado en «Pegar enlace»
// NEKOWALL_TIMING=1 imprime en stderr cuánto tarda en aparecer el primer fotograma.

#include "Bridge.h"
#include "Workshop.h"
#include "DesktopModel.h"
#include <cstring>
#include <cmath>
#ifdef NEKOWALL_CAJA
#include "CajaLayout.h"
#include "CajaScan.h"
#endif
#include <QScreen>
#include <QQmlComponent>

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


#ifdef NEKOWALL_CAJA
#include <QAbstractNativeEventFilter>
#include <xcb/xcb.h>
// Ventana de iconos en MATE. Es de tipo «escritorio» (_NET_WM_WINDOW_TYPE_DESKTOP), como la de Caja y la
// del vídeo (xwinwrap): Marco las deja en su capa más baja, en todos los escritorios, fuera de la barra de
// tareas y sin ocultarlas con «Mostrar escritorio». Dentro de esa capa manda el orden: se mantiene ENCIMA
// del vídeo (que se recrea al cambiar de fondo), reaccionando a cada cambio de _NET_CLIENT_LIST_STACKING.
static xcb_atom_t xAtom(xcb_connection_t *c, const char *n)
{
    xcb_intern_atom_reply_t *r = xcb_intern_atom_reply(c, xcb_intern_atom(c, 0, uint16_t(strlen(n)), n), nullptr);
    const xcb_atom_t a = r ? r->atom : XCB_ATOM_NONE;
    free(r);
    return a;
}

static bool hasAtom(xcb_connection_t *c, xcb_window_t w, xcb_atom_t prop, xcb_atom_t value)
{
    bool found = false;
    if (xcb_get_property_reply_t *r = xcb_get_property_reply(c, xcb_get_property(c, 0, w, prop, XCB_ATOM_ATOM, 0, 32), nullptr)) {
        const auto *as = static_cast<const xcb_atom_t *>(xcb_get_property_value(r));
        for (int k = 0; k < xcb_get_property_value_length(r) / 4; ++k)
            found = found || as[k] == value;
        free(r);
    }
    return found;
}

class X11StackGuard : public QObject, public QAbstractNativeEventFilter
{
public:
    X11StackGuard(QWindow *win, xcb_connection_t *c) : QObject(win), m_c(c), m_win(xcb_window_t(win->winId()))
    {
        m_root = xcb_setup_roots_iterator(xcb_get_setup(c)).data->root;
        m_stacking = xAtom(c, "_NET_CLIENT_LIST_STACKING");
        m_type = xAtom(c, "_NET_WM_WINDOW_TYPE");
        m_desktop = xAtom(c, "_NET_WM_WINDOW_TYPE_DESKTOP");
        m_timer.setSingleShot(true);
        m_timer.setInterval(60);
        connect(&m_timer, &QTimer::timeout, this, &X11StackGuard::check);
        m_poll.setInterval(3000);                     // por si se pierde algún aviso
        connect(&m_poll, &QTimer::timeout, this, &X11StackGuard::check);
        m_poll.start();
        qApp->installNativeEventFilter(this);
        m_timer.start();
    }
    ~X11StackGuard() override { qApp->removeNativeEventFilter(this); }

    bool nativeEventFilter(const QByteArray &type, void *message, qintptr *) override
    {
        if (type != "xcb_generic_event_t")
            return false;
        auto *ev = static_cast<xcb_generic_event_t *>(message);
        if ((ev->response_type & 0x7f) == XCB_PROPERTY_NOTIFY) {
            auto *pn = reinterpret_cast<xcb_property_notify_event_t *>(ev);
            if (pn->window == m_root && pn->atom == m_stacking && !m_timer.isActive())
                m_timer.start();
        }
        return false;
    }

private:
    void check()
    {
        xcb_get_property_reply_t *r = xcb_get_property_reply(
            m_c, xcb_get_property(m_c, 0, m_root, m_stacking, XCB_ATOM_WINDOW, 0, 4096), nullptr);
        if (!r)
            return;
        const auto *wins = static_cast<const xcb_window_t *>(xcb_get_property_value(r));
        const int n = xcb_get_property_value_length(r) / 4;
        int mine = -1;
        for (int i = 0; i < n; ++i)
            if (wins[i] == m_win)
                mine = i;
        bool raise = false;
        // Ninguna otra ventana de tipo escritorio (el vídeo) puede quedar encima de nosotros.
        for (int i = mine + 1; mine >= 0 && i < n && !raise; ++i)
            raise = hasAtom(m_c, wins[i], m_type, m_desktop);
        free(r);
        if (raise) {
            const uint32_t mode = XCB_STACK_MODE_ABOVE;
            xcb_configure_window(m_c, m_win, XCB_CONFIG_WINDOW_STACK_MODE, &mode);
            xcb_flush(m_c);
        }
    }

    xcb_connection_t *m_c;
    xcb_window_t m_win, m_root = 0;
    xcb_atom_t m_stacking = 0, m_type = 0, m_desktop = 0;
    QTimer m_timer, m_poll;
};

// Antes de mostrarla: tipo «escritorio» (propiedad que respeta el plugin xcb de Qt: NetWmType Desktop = 2).
static void x11PrepareDesktopWindow(QWindow *win)
{
    win->setProperty("_q_xcb_wm_window_type", 0x000002);
}

static void x11DesktopWindow(QWindow *win)
{
    auto *x11 = qGuiApp->nativeInterface<QNativeInterface::QX11Application>();
    if (!x11)
        return;
    new X11StackGuard(win, x11->connection());
}
#endif

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

#ifdef NEKOWALL_CAJA
    // Banco de pruebas: dónde colocaría NekoWall cada icono del escritorio de Caja (sin posiciones guardadas).
    if (app.arguments().contains(QStringLiteral("--caja-layout-test"))) {
        CajaSettings cs = CajaSettings::load();
        for (const QString &a : app.arguments())
            if (a.startsWith(QStringLiteral("--hint-metrics=")))
                cs.font.hintMetrics = a.mid(15).toInt();
            else if (a.startsWith(QStringLiteral("--round=")))
                cs.font.roundPositions = a.mid(8).toInt();
            else if (a.startsWith(QStringLiteral("--hint=")))
                cs.font.hinting = a.mid(7);
            else if (a.startsWith(QStringLiteral("--aa=")))
                cs.font.antialias = a.mid(5);
        QScreen *scr = QGuiApplication::primaryScreen();
        const QRect full = scr->geometry(), avail = scr->availableGeometry();
        cs.font.dpi = CajaScan::xsettingsDpi() > 0 ? CajaScan::xsettingsDpi() : scr->logicalDotsPerInch();
        for (const QString &a : app.arguments())
            if (a.startsWith(QStringLiteral("--dpi=")))
                cs.font.dpi = a.mid(6).toDouble();
        // Caja coloca con la letra general del sistema y después aplica la del escritorio sin recolocar.
        CajaFontOptions placeFont = cs.font;
        if (!app.arguments().contains(QStringLiteral("--colocar-con-letra-escritorio")))
            placeFont.font = cs.interfaceFont;
        CajaLayout place(placeFont);
        CajaLayout lay(cs.font);
        lay.ellipsisLimit = cs.ellipsisLimit;
        lay.pixelsPerUnit = cs.zoomSize / 48.0;
        lay.canvasWidth = int(avail.width() / lay.pixelsPerUnit);
        lay.canvasHeight = int(avail.height() / lay.pixelsPerUnit);
        const bool ignoreSaved = app.arguments().contains(QStringLiteral("--ignore-saved"));
        QList<CajaEntry> es = CajaScan::entries(cs);
        QList<CajaIcon> icons;
        for (const CajaEntry &e : std::as_const(es)) {
            CajaIcon i;
            i.name = e.display;
            i.iconW = i.iconH = cs.zoomSize;
            i.positioned = e.positioned && !ignoreSaved;
            i.additional = e.additionalFor(CajaSettings::captionPieces(48));   // se coloca con el zoom estándar
            i.x = e.x;
            i.y = e.y;
            icons << i;
        }
        place.ellipsisLimit = cs.placeEllipsisLimit;   // el contenedor aún no es «escritorio» al colocar
        // …y con el zoom estándar (iconos de 48, 1 unidad = 1 píxel): el zoom del escritorio se aplica después.
        place.pixelsPerUnit = 1.0;
        place.canvasWidth = avail.width();
        place.canvasHeight = avail.height();
        for (CajaIcon &i : icons)
            i.iconW = i.iconH = 48;
        place.layout(icons);
        for (CajaIcon &i : icons)
            i.iconW = i.iconH = cs.zoomSize;
        for (int k = 0; k < icons.size(); ++k) {   // lo que se ve: letra del escritorio y pies de su zoom
            CajaIcon &i = icons[k];
            i.additional = es.at(k).additionalFor(CajaSettings::captionPieces(cs.zoomSize));
            i.g = lay.measure(i.name, i.iconW, i.iconH, i.additional);
            // Con otro zoom, Caja vuelve a encajar cada icono dentro de la pantalla (en unidades de ese zoom).
            lay.setPosition(i, i.x, i.y);
        }
        std::printf("pantalla %dx%d · útil %d,%d %dx%d · dpi %.4f · letra %s · límite %d\n", full.width(), full.height(),
                    avail.x(), avail.y(), avail.width(), avail.height(), cs.font.dpi, qPrintable(cs.font.font), cs.ellipsisLimit);
        for (int k = 0; k < es.size(); ++k)
        {
            const CajaIcon &ic = icons.at(k);
            // Recuadro como lo da la accesibilidad de Caja (bounds del elemento: x0,y0 y ancho/alto +1).
            // En píxeles de pantalla (mundo × zoom + márgenes del área de trabajo), como la accesibilidad.
            const double ppu = lay.pixelsPerUnit;
            const int bx = avail.x() - full.x() + int(std::floor((int(ic.x) + ic.g.total.x0) * ppu));
            const int by = avail.y() - full.y() + int(std::floor((int(ic.y) + ic.g.total.y0) * ppu));
            const int bw = int(std::ceil((ic.g.total.x1 - ic.g.total.x0 + 1) * ppu));
            const int bh = int(std::ceil((ic.g.total.y1 - ic.g.total.y0 + 1) * ppu));
            std::printf("%s\t%d,%d\tguardada=%s\tlíneas=%d\trecuadro=%d,%d %dx%d\n", qPrintable(es.at(k).display), int(ic.x), int(ic.y),
                        es.at(k).positioned ? qPrintable(QStringLiteral("%1,%2").arg(es.at(k).x).arg(es.at(k).y)) : "-",
                        ic.g.lines, bx, by, bw, bh);
            if (qEnvironmentVariableIsSet("NEKOWALL_CAJA_DEBUG")) {
                const CajaRect l = lay.boundsForLayout(ic), e = lay.boundsForEntire(ic);
                std::printf("    layout y %d..%d (filas %d..%d) · entero y %d..%d · texto alto %d/%d/%d\n", l.y0, l.y1,
                            int(std::floor((l.y0 - 10) / 20.0)), int(std::floor((l.y1 - 10) / 20.0)), e.y0, e.y1,
                            ic.g.textHeight, ic.g.textHeightForLayout, ic.g.textHeightForEntire);
            }
        }
        return 0;
    }
#endif
    // Iconos del escritorio: proceso aparte, ligero (sin Bridge ni CLI), una sola instancia.
    if (app.arguments().contains(QStringLiteral("--desktop"))) {
        QString runDir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
        if (runDir.isEmpty())
            runDir = QDir::tempPath();
        static QLockFile only(runDir + QStringLiteral("/nekowall-desktop.lock"));
        only.setStaleLockTime(0);
        if (!only.tryLock(100)) {
            std::fprintf(stderr, "Los iconos del escritorio de NekoWall ya están en marcha.\n");
            return 0;
        }
        const bool wayland = QGuiApplication::platformName().startsWith(QStringLiteral("wayland"));
#ifdef NEKOWALL_CAJA
        // MATE (X11): una ventana que imita al escritorio de Caja, encima del vídeo de fondo.
        if (!wayland) {
            QQuickStyle::setStyle(QStringLiteral("Fusion"));
            static QQmlApplicationEngine deskX;
            deskX.addImageProvider(QStringLiteral("icono"), new ThemeIconProvider);
            static QQmlComponent compX(&deskX);
            compX.loadFromModule("NekoWall", "DesktopIconsX11");
            if (compX.isError()) {
                for (const auto &e : compX.errors())
                    std::fprintf(stderr, "%s\n", qPrintable(e.toString()));
                return 1;
            }
            auto *model = new DesktopModel(QString(), nullptr, DesktopModel::Caja);
            deskX.addImageProvider(QStringLiteral("etiqueta"), new CajaLabelProvider(model));
            if (app.arguments().contains(QStringLiteral("--capture-caja"))) {
                // Antes de sustituir a Caja: guardar dónde tiene ahora cada icono.
                model->setSaveNewPositions(false);
                model->setViewSize(QGuiApplication::primaryScreen()->virtualGeometry().width(),
                                   QGuiApplication::primaryScreen()->virtualGeometry().height());
                std::printf("posiciones tomadas de Caja: %d\n", model->captureCajaPositions());
                return 0;
            }
            QObject *obj = compX.createWithInitialProperties({{QStringLiteral("dm"), QVariant::fromValue<QObject *>(model)}});
            auto *win = qobject_cast<QQuickWindow *>(obj);
            if (!win) {
                for (const auto &e : compX.errors())
                    std::fprintf(stderr, "%s\n", qPrintable(e.toString()));
                return 1;
            }
            model->setParent(win);
            win->setGeometry(QGuiApplication::primaryScreen()->virtualGeometry());
            x11PrepareDesktopWindow(win);
            win->show();
            x11DesktopWindow(win);
            QObject::connect(QGuiApplication::primaryScreen(), &QScreen::virtualGeometryChanged, win,
                             [win](const QRect &g) { win->setGeometry(g); });
            return app.exec();
        }
#endif
        if (!wayland) {
            std::fprintf(stderr, "Los iconos del escritorio de NekoWall funcionan en Wayland y en MATE.\n");
            return 1;
        }
        // Mismo perfil de pcmanfm-qt que su escritorio (--profile=lxqt …).
        for (const QString &a : app.arguments())
            if (a.startsWith(QStringLiteral("--profile=")))
                qputenv("NEKOWALL_PCMANFM_PROFILE", a.mid(10).toUtf8());
        QQuickStyle::setStyle(QStringLiteral("Fusion"));
        static QQmlApplicationEngine desk;
        desk.addImageProvider(QStringLiteral("icono"), new ThemeIconProvider);
        static QQmlComponent comp(&desk);
        comp.loadFromModule("NekoWall", "DesktopIcons");
        if (comp.isError()) {
            for (const auto &e : comp.errors())
                std::fprintf(stderr, "%s\n", qPrintable(e.toString()));
            return 1;
        }
        // Un escritorio por pantalla (como pcmanfm-qt en Wayland), también al conectar/desconectar pantallas.
        static QHash<QScreen *, QPair<DesktopModel *, QObject *>> desktops;
        auto addScreen = [](QScreen *screen) {
            if (desktops.contains(screen))
                return;
            auto *model = new DesktopModel(screen->name());
            QObject *obj = comp.createWithInitialProperties({{QStringLiteral("dm"), QVariant::fromValue<QObject *>(model)}});
            auto *win = qobject_cast<QQuickWindow *>(obj);
            if (!win) {
                for (const auto &e : comp.errors())
                    std::fprintf(stderr, "%s\n", qPrintable(e.toString()));
                delete obj;
                delete model;
                return;
            }
            model->setParent(win);
            win->setScreen(screen);
            win->show();
            desktops.insert(screen, {model, obj});
        };
        auto removeScreen = [](QScreen *screen) {
            if (auto d = desktops.take(screen); d.second)
                d.second->deleteLater();
        };
        for (QScreen *s : QGuiApplication::screens())
            addScreen(s);
        QObject::connect(&app, &QGuiApplication::screenAdded, &app, addScreen);
        QObject::connect(&app, &QGuiApplication::screenRemoved, &app, removeScreen);
        return app.exec();
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
