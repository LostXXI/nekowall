// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Modo Caja (MATE) del modelo de iconos del escritorio: ajustes, archivos, orden, posiciones y colocación
// idénticos a los del escritorio de Caja (ver CajaLayout/CajaScan y la verificación contra Caja por AT-SPI).
#include "DesktopModel.h"

#include <QThread>

QImage CajaLabelProvider::requestImage(const QString &id, QSize *size, const QSize &)
{
    const QStringList p = id.split(QLatin1Char('/'));
    QImage img;
    if (p.size() >= 2 && m_model) {
        // Desde el hilo de imágenes de Qt Quick: se pide al hilo del modelo.
        auto get = [&] { img = m_model->cajaLabelImage(p.at(0).toInt(), p.at(1) == QStringLiteral("1")); };
        if (QThread::currentThread() == m_model->thread())
            get();
        else
            QMetaObject::invokeMethod(m_model, get, Qt::BlockingQueuedConnection);
    }
    if (size)
        *size = img.size();
    return img;
}

#ifdef NEKOWALL_CAJA

#include <QDir>
#include <QIcon>
#include <QFileInfo>
#include <QGuiApplication>
#include <QScreen>
#include <QStandardPaths>
#include <cmath>
#include <functional>
#include <algorithm>

#pragma push_macro("signals")
#undef signals
#include <gio/gio.h>
#ifdef NEKOWALL_ATSPI
#include <atspi/atspi.h>
#endif
#pragma pop_macro("signals")
#include <libintl.h>

static QString keyfilePath()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + QStringLiteral("/caja/desktop-metadata");
}

// [directory] de ~/.config/caja/desktop-metadata (mantener alineado, bloquear posición).
static bool directoryFlag(const char *key, bool def)
{
    GKeyFile *kf = g_key_file_new();
    bool v = def;
    if (g_key_file_load_from_file(kf, keyfilePath().toUtf8().constData(), G_KEY_FILE_NONE, nullptr)) {
        GError *err = nullptr;
        gchar *s = g_key_file_get_string(kf, "directory", key, &err);
        if (s) {
            v = QString::fromUtf8(s).trimmed() == QStringLiteral("true");
            g_free(s);
        }
        if (err)
            g_error_free(err);
    }
    g_key_file_free(kf);
    return v;
}

static void setDirectoryFlag(const char *key, bool on)
{
    const QByteArray file = keyfilePath().toUtf8();
    GKeyFile *kf = g_key_file_new();
    g_key_file_load_from_file(kf, file.constData(), G_KEY_FILE_KEEP_COMMENTS, nullptr);
    g_key_file_set_string(kf, "directory", key, on ? "true" : "false");
    QDir().mkpath(QFileInfo(QString::fromUtf8(file)).absolutePath());
    g_key_file_save_to_file(kf, file.constData(), nullptr);
    g_key_file_free(kf);
}

// Cambios en los ajustes de MATE → recargar (GLib y Qt comparten el bucle de eventos).
static void onSettingsChanged(GSettings *, gchar *, gpointer self)
{
    auto *m = static_cast<QObject *>(self);
    QMetaObject::invokeMethod(m, "cajaSettingsChanged", Qt::QueuedConnection);
}

void DesktopModel::cajaLoadSettings()
{
    static bool connected = false;
    if (!connected) {
        connected = true;
        for (const char *schema : {"org.mate.caja.desktop", "org.mate.caja.icon-view", "org.mate.caja.preferences",
                                   "org.mate.interface", "org.mate.font-rendering"}) {
            GSettingsSchemaSource *src = g_settings_schema_source_get_default();
            GSettingsSchema *sc = src ? g_settings_schema_source_lookup(src, schema, TRUE) : nullptr;
            if (!sc)
                continue;
            g_settings_schema_unref(sc);
            GSettings *s = g_settings_new(schema);   // vive mientras el proceso
            g_signal_connect(s, "changed", G_CALLBACK(onSettingsChanged), this);
        }
    }
    m_caja = CajaSettings::load();
    // Los iconos del tema de MATE (org.mate.interface icon-theme), como Caja.
    if (!m_caja.iconTheme.isEmpty() && QIcon::themeName() != m_caja.iconTheme)
        QIcon::setThemeName(m_caja.iconTheme);
    const double dpi = CajaScan::xsettingsDpi();
    if (dpi > 0)
        m_caja.font.dpi = dpi;
    else if (QGuiApplication::primaryScreen())
        m_caja.font.dpi = QGuiApplication::primaryScreen()->logicalDotsPerInch();
    // Caja coloca como un contenedor genérico: letra del sistema, límite de las vistas, zoom estándar.
    CajaFontOptions placeFont = m_caja.font;
    placeFont.font = m_caja.interfaceFont;
    m_place = std::make_unique<CajaLayout>(placeFont);
    m_place->ellipsisLimit = m_caja.placeEllipsisLimit;
    m_place->pixelsPerUnit = 1.0;
    m_show = std::make_unique<CajaLayout>(m_caja.font);
    m_show->ellipsisLimit = m_caja.ellipsisLimit;
    m_ppu = m_caja.zoomSize / 48.0;
    m_show->pixelsPerUnit = m_ppu;
    m_keepAligned = directoryFlag("caja-icon-view-keep-aligned", true);
    m_lockPositions = directoryFlag("caja-icon-view-lock-icons-position", false);
    m_place->keepAligned = m_show->keepAligned = m_keepAligned;

    // Lo que usa QML para dibujar: la letra del escritorio de Caja.
    const QString f = m_caja.font.font;
    const qsizetype sp = f.lastIndexOf(QLatin1Char(' '));
    bool ok = false;
    const double pt = sp > 0 ? f.mid(sp + 1).toDouble(&ok) : 0;
    m_font = QFont(ok ? f.left(sp) : f);
    if (ok && pt > 0)
        m_font.setPointSizeF(pt);
    m_iconSize = m_caja.zoomSize;
    m_singleClick = m_caja.singleClick;
    m_showHidden = m_caja.showHidden;
    m_confirmTrash = m_caja.confirmTrash;
    m_fg = Qt::white;
    m_shadow = QColor(0, 0, 0, 200);
    m_noTooltip = true;     // Caja no muestra descripciones emergentes en el escritorio
    emit settingsChanged();
}

void DesktopModel::cajaSettingsChanged()
{
    cajaLoadSettings();
    cajaReload();
}

QString DesktopModel::cajaText(const QString &msgid, const QString &domain) const
{
    return QString::fromUtf8(g_dgettext(domain.toUtf8().constData(), msgid.toUtf8().constData()));
}

void DesktopModel::cajaSavePosition(const Item &it)
{
    CajaEntry e;
    e.key = it.fileName;
    e.path = it.path;
    e.special = it.special;
    CajaScan::writePosition(e, int(it.cx), int(it.cy));   // Caja guarda «%d,%d» de la posición
}

void DesktopModel::cajaReload()
{
    QHash<QString, bool> selected, cut;
    QHash<QString, QPointF> known;            // colocados en esta sesión (aunque Caja no los haya guardado)
    for (const Item &it : std::as_const(m_items)) {
        if (it.selected) selected.insert(it.fileName, true);
        if (it.cut) cut.insert(it.fileName, true);
        if (it.placed) known.insert(it.fileName, {it.cx, it.cy});
    }
    const QString currentName = m_current >= 0 && m_current < m_items.size() ? m_items.at(m_current).fileName : QString();

    QList<Item> items;
    const QList<CajaEntry> entries = CajaScan::entries(m_caja);
    for (const CajaEntry &e : entries) {
        Item it;
        it.name = e.display;
        it.fileName = e.key;
        it.path = e.special ? QString() : e.path;
        it.url = e.uri;
        it.icon = e.iconName.isEmpty() ? QStringLiteral("text-x-generic") : e.iconName;
        it.thumb = e.thumb;
        it.dir = e.dir;
        it.launcher = e.launcher;
        it.hidden = e.hidden;
        it.special = e.special;
        it.mime = e.dir ? QStringLiteral("inode/directory") : QString();
        it.additional = e.additionalFor(CajaSettings::captionPieces(m_caja.zoomSize));
        if (e.positioned) {
            it.placed = true;
            it.cx = e.x;
            it.cy = e.y;
        } else if (known.contains(e.key)) {
            it.placed = true;
            it.cx = known.value(e.key).x();
            it.cy = known.value(e.key).y();
        }
        if (it.launcher) {
            // Orden del lanzador (para abrirlo).
            const Item l = readLauncher(e.path);
            it.exec = l.exec;
            it.terminal = l.terminal;
            if (!l.url.isEmpty())
                it.url = l.url;
        }
        it.selected = selected.contains(e.key);
        it.cut = cut.contains(e.key);
        items << it;
    }
    beginResetModel();
    m_items = items;
    m_current = -1;
    for (int i = 0; i < m_items.size(); ++i)
        if (m_items.at(i).fileName == currentName)
            m_current = i;
    endResetModel();
    cajaRelayout(m_cajaFirstLoad);
    m_cajaFirstLoad = false;
    emit countChanged();
    emit selectionChanged();
}

// Coloca los que no tienen sitio y calcula la geometría en píxeles de todos.
void DesktopModel::cajaRelayout(bool firstLoad)
{
    if (m_view.isEmpty() || !m_place || !m_show)
        return;
    // Área útil: la pantalla menos los paneles (_NET_WORKAREA), como icon_container_set_workarea.
    QRect full(QPoint(0, 0), m_view), avail = full;
    if (QScreen *scr = QGuiApplication::primaryScreen()) {
        const QRect g = scr->virtualGeometry(), a = scr->availableGeometry();
        avail = QRect(a.topLeft() - g.topLeft(), a.size());
    }
    m_origin = avail.topLeft();
    m_place->canvasWidth = avail.width();
    m_place->canvasHeight = avail.height();
    m_show->canvasWidth = int(avail.width() / m_ppu);
    m_show->canvasHeight = int(avail.height() / m_ppu);

    // 1. Los nuevos: al arrancar, como Caja (letra del sistema, zoom estándar); después, como Caja en marcha.
    const bool any = std::any_of(m_items.cbegin(), m_items.cend(), [](const Item &i) { return !i.placed; });
    if (any) {
        CajaLayout &eng = firstLoad ? *m_place : *m_show;
        QList<CajaIcon> icons;
        for (const Item &it : std::as_const(m_items)) {
            CajaIcon c;
            c.name = it.name;
            c.iconW = c.iconH = firstLoad ? 48 : m_caja.zoomSize;
            c.additional = firstLoad ? QString() : it.additional;
            c.positioned = it.placed;
            c.x = it.cx;
            c.y = it.cy;
            icons << c;
        }
        // Soltados o pegados con una posición: van al primer hueco desde ahí (has_lazy_position).
        eng.layout(icons);
        for (int k = 0; k < m_items.size(); ++k) {
            Item &it = m_items[k];
            if (it.placed)
                continue;
            if (m_pending.contains(it.fileName)) {
                const QPoint p = m_pending.take(it.fileName);
                QList<CajaIcon> others;
                for (int j = 0; j < icons.size(); ++j)
                    if (j != k && m_items.at(j).placed)
                        others << icons.at(j);
                int x = int((p.x() - m_origin.x()) / m_ppu), y = int((p.y() - m_origin.y()) / m_ppu);
                eng.findEmptyLocation(others, icons.at(k), x, y, x, y);
                eng.setPosition(icons[k], x, y);
            }
            it.cx = icons.at(k).x;
            it.cy = icons.at(k).y;
            it.placed = true;
            if (m_saveNew)
                cajaSavePosition(it);     // Caja guarda la posición de los iconos que coloca
        }
    }

    // 2. Lo que se ve: ajustes del escritorio; con otro zoom, Caja vuelve a encajar cada icono en la pantalla.
    for (Item &it : m_items) {
        CajaIcon c;
        c.name = it.name;
        c.iconW = c.iconH = m_caja.zoomSize;
        c.additional = it.additional;
        c.g = m_show->measure(c.name, c.iconW, c.iconH, c.additional);
        c.x = it.cx;
        c.y = it.cy;
        m_show->setPosition(c, c.x, c.y);
        it.cx = c.x;
        it.cy = c.y;
        // Píxeles: icono arriba en el centro, nombre debajo (compute_text_rectangle).
        const int ix = m_origin.x() + int(std::lround(it.cx * m_ppu));
        const int iy = m_origin.y() + int(std::lround(it.cy * m_ppu));
        const int ip = m_caja.zoomSize;
        const int tw = c.g.textWidth, th = c.g.textHeight;
        const int tx = ix + ip / 2 - tw / 2, ty = iy + ip;
        const QRect icon(ix, iy, ip, ip), text(tx, ty, tw, th + 1);
        it.box = icon.united(text);
        it.iconRect = icon.translated(-it.box.topLeft());
        it.textRect = text.translated(-it.box.topLeft());
        it.editableH = c.g.editableHeight;
        it.geom = c.g;
        it.pos = it.box.topLeft();
    }
    if (!m_items.isEmpty())
        emit dataChanged(index(0), index(int(m_items.size()) - 1));
    emit geometryChanged();
}

QImage DesktopModel::cajaLabelImage(int row, bool selected) const
{
    if (row < 0 || row >= m_items.size() || !m_show)
        return {};
    const Item &it = m_items.at(row);
    // caja-desktop.css: texto blanco con «text-shadow: 1px 1px alpha(#000000, 0.8)»; seleccionado, sin sombra.
    return m_show->renderLabel(it.name, it.additional, it.geom, Qt::white,
                               selected ? QColor(0, 0, 0, 0) : QColor::fromRgbF(0, 0, 0, 0.8f));
}

QRect DesktopModel::cajaHitRect(const Item &it) const
{
    return it.box;
}

// Arrastre: como caja_icon_container_move_icon (con «Mantener alineado» se ajusta a la rejilla).
void DesktopModel::cajaMove(qreal dx, qreal dy)
{
    if (m_lockPositions)
        return;                 // «Bloquear la posición de los iconos» de Caja
    for (Item &it : m_items) {
        if (!it.selected)
            continue;
        CajaIcon c;
        c.name = it.name;
        c.iconW = c.iconH = m_caja.zoomSize;
        c.additional = it.additional;
        c.g = m_show->measure(c.name, c.iconW, c.iconH, c.additional);
        c.x = it.cx;
        c.y = it.cy;
        int x = int(std::lround(it.cx + dx / m_ppu)), y = int(std::lround(it.cy + dy / m_ppu));
        if (m_keepAligned)
            m_show->snapPosition(c, x, y);
        m_show->setPosition(c, x, y);
        it.cx = c.x;
        it.cy = c.y;
        cajaSavePosition(it);
    }
    cajaRelayout(false);
}

void DesktopModel::organizeByName()
{
    if (m_flavor != Caja || !m_show)
        return;
    // «Organizar el escritorio por nombre»: columnas por nombre (lay_down_icons_vertical_desktop) y se fijan.
    QList<CajaIcon> icons;
    for (const Item &it : std::as_const(m_items)) {
        CajaIcon c;
        c.name = it.name;
        c.iconW = c.iconH = m_caja.zoomSize;
        c.additional = it.additional;
        icons << c;
    }
    const bool saved = m_show->alwaysFindEmpty;
    m_show->alwaysFindEmpty = false;
    m_show->layout(icons);
    m_show->alwaysFindEmpty = saved;
    for (int k = 0; k < m_items.size(); ++k) {
        m_items[k].cx = icons.at(k).x;
        m_items[k].cy = icons.at(k).y;
        m_items[k].placed = true;
        cajaSavePosition(m_items.at(k));
    }
    cajaRelayout(false);
}

void DesktopModel::setKeepAligned(bool on)
{
    if (m_flavor != Caja)
        return;
    setDirectoryFlag("caja-icon-view-keep-aligned", on);
    m_keepAligned = on;
    if (m_show)
        m_show->keepAligned = on;
    if (on) {
        // align_icons: cada icono al hueco libre más cercano de la rejilla.
        QList<CajaIcon> placed;
        for (Item &it : m_items) {
            CajaIcon c;
            c.name = it.name;
            c.iconW = c.iconH = m_caja.zoomSize;
            c.additional = it.additional;
            c.g = m_show->measure(c.name, c.iconW, c.iconH, c.additional);
            c.x = it.cx;
            c.y = it.cy;
            int x = int(it.cx), y = int(it.cy);
            m_show->findEmptyLocation(placed, c, x, y, x, y);
            m_show->setPosition(c, x, y);
            it.cx = c.x;
            it.cy = c.y;
            placed << c;
            cajaSavePosition(it);
        }
    }
    emit settingsChanged();
    cajaRelayout(false);
}

// Lee de Caja (accesibilidad) dónde tiene ahora cada icono y lo guarda en sus metadatos, para que al
// reemplazarlo el escritorio quede exactamente como el usuario lo veía. Devuelve cuántos guardó.
int DesktopModel::captureCajaPositions()
{
#ifdef NEKOWALL_ATSPI
    if (m_flavor != Caja)
        return 0;
    if (atspi_init() > 1)
        return 0;
    QHash<QString, QRect> found;
    std::function<void(AtspiAccessible *, int)> walk = [&](AtspiAccessible *o, int depth) {
        if (!o || depth > 8)
            return;
        gchar *role = atspi_accessible_get_role_name(o, nullptr);
        gchar *name = atspi_accessible_get_name(o, nullptr);
        if (role && name && QByteArray(role) == "icon" && name[0]) {
            if (AtspiComponent *comp = atspi_accessible_get_component_iface(o)) {
                if (AtspiRect *r = atspi_component_get_extents(comp, ATSPI_COORD_TYPE_SCREEN, nullptr)) {
                    found.insert(QString::fromUtf8(name), QRect(r->x, r->y, r->width, r->height));
                    g_free(r);
                }
                g_object_unref(comp);
            }
        }
        g_free(role);
        g_free(name);
        const int n = std::min(atspi_accessible_get_child_count(o, nullptr), 2000);
        for (int i = 0; i < n; ++i) {
            AtspiAccessible *c = atspi_accessible_get_child_at_index(o, i, nullptr);
            walk(c, depth + 1);
            if (c)
                g_object_unref(c);
        }
    };
    AtspiAccessible *desk = atspi_get_desktop(0);
    const int apps = atspi_accessible_get_child_count(desk, nullptr);
    for (int i = 0; i < apps; ++i) {
        AtspiAccessible *app = atspi_accessible_get_child_at_index(desk, i, nullptr);
        gchar *n = app ? atspi_accessible_get_name(app, nullptr) : nullptr;
        if (n && QByteArray(n) == "caja")
            walk(app, 0);
        g_free(n);
        if (app)
            g_object_unref(app);
    }
    g_object_unref(desk);
    int saved = 0;
    for (Item &it : m_items) {
        if (!found.contains(it.name))
            continue;
        // El recuadro empieza en el nombre si es más ancho que el icono: x del icono = x + mitad de lo que sobresale.
        const QRect r = found.value(it.name);
        CajaIcon c;
        c.name = it.name;
        c.iconW = c.iconH = m_caja.zoomSize;
        c.additional = it.additional;
        c.g = m_show->measure(c.name, c.iconW, c.iconH, c.additional);
        const int x = int(std::floor((r.x() - m_origin.x()) / m_ppu)) - c.g.total.x0;
        const int y = int(std::floor((r.y() - m_origin.y()) / m_ppu)) - c.g.total.y0;
        CajaEntry e;
        e.key = it.fileName;
        e.path = it.path;
        e.special = it.special;
        int sx = 0, sy = 0;
        if (CajaScan::readPosition(e, sx, sy))
            continue;               // ya tenía posición guardada: es la que Caja respeta
        it.cx = x;
        it.cy = y;
        it.placed = true;
        cajaSavePosition(it);
        ++saved;
    }
    cajaRelayout(false);
    return saved;
#else
    return 0;
#endif
}

#else   // sin modo Caja (no hay Pango/GIO al compilar): versiones vacías para QML

void DesktopModel::cajaSettingsChanged() {}
QString DesktopModel::cajaText(const QString &msgid, const QString &) const { return msgid; }
void DesktopModel::organizeByName() {}
void DesktopModel::setKeepAligned(bool) {}
int DesktopModel::captureCajaPositions() { return 0; }
QImage DesktopModel::cajaLabelImage(int, bool) const { return {}; }

#endif // NEKOWALL_CAJA
