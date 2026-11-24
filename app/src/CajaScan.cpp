// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
#include "CajaScan.h"

#include <QDir>
#include <QFileInfo>
#include <QMimeDatabase>
#include <QStandardPaths>
#include <QUrl>
#include <cstring>
#include <algorithm>
// GIO usa «signals» como nombre de campo y Qt lo define como macro: se aparta mientras se incluye.
#pragma push_macro("signals")
#undef signals
#include <gio/gio.h>
#pragma pop_macro("signals")
#include <libintl.h>
#include <xcb/xcb.h>

static GSettings *settingsFor(const char *schema)
{
    GSettingsSchemaSource *src = g_settings_schema_source_get_default();
    if (!src)
        return nullptr;
    GSettingsSchema *sc = g_settings_schema_source_lookup(src, schema, TRUE);
    if (!sc)
        return nullptr;
    g_settings_schema_unref(sc);
    return g_settings_new(schema);
}

static QString getStr(GSettings *s, const char *key, const QString &def)
{
    if (!s)
        return def;
    gchar *v = g_settings_get_string(s, key);
    const QString r = QString::fromUtf8(v);
    g_free(v);
    return r;
}

CajaSettings CajaSettings::load()
{
    CajaSettings c;
    if (GSettings *d = settingsFor("org.mate.caja.desktop")) {
        const QString f = getStr(d, "font", {});
        if (!f.isEmpty())
            c.font.font = f;
        c.ellipsisLimit = g_settings_get_int(d, "text-ellipsis-limit");
        c.computerVisible = g_settings_get_boolean(d, "computer-icon-visible");
        c.homeVisible = g_settings_get_boolean(d, "home-icon-visible");
        c.trashVisible = g_settings_get_boolean(d, "trash-icon-visible");
        c.networkVisible = g_settings_get_boolean(d, "network-icon-visible");
        c.volumesVisible = g_settings_get_boolean(d, "volumes-visible");
        g_object_unref(d);
    }
    if (GSettings *iv = settingsFor("org.mate.caja.icon-view")) {
        const QString z = getStr(iv, "default-zoom-level", QStringLiteral("standard"));
        static const QList<QPair<QString, int>> sizes{{QStringLiteral("smallest"), 16}, {QStringLiteral("smaller"), 24},
                                                      {QStringLiteral("small"), 32}, {QStringLiteral("standard"), 48},
                                                      {QStringLiteral("large"), 72}, {QStringLiteral("larger"), 96},
                                                      {QStringLiteral("largest"), 192}};
        for (const auto &p : sizes)
            if (p.first == z)
                c.zoomSize = p.second;
        // text-ellipsis-limit: ["3"] o ["2", "small:1", …] (get_text_ellipsis_limit_for_zoom de Caja).
        gchar **lim = g_settings_get_strv(iv, "text-ellipsis-limit");
        int def = 3, forZoom = -1;
        for (int k = 0; lim && lim[k]; ++k) {
            const QString e = QString::fromUtf8(lim[k]);
            const qsizetype colon = e.indexOf(QLatin1Char(':'));
            if (colon < 0)
                def = e.toInt();
            else if (e.left(colon) == QStringLiteral("standard"))   // se coloca con el zoom estándar
                forZoom = e.mid(colon + 1).toInt();
        }
        g_strfreev(lim);
        c.placeEllipsisLimit = forZoom >= 0 ? forZoom : def;
        gchar **cap = g_settings_get_strv(iv, "captions");
        c.captions.clear();
        for (int k = 0; cap && cap[k]; ++k)
            c.captions << QString::fromUtf8(cap[k]);
        g_strfreev(cap);
        g_object_unref(iv);
    }
    if (GSettings *p = settingsFor("org.mate.caja.preferences")) {
        c.directoriesFirst = g_settings_get_boolean(p, "sort-directories-first");
        c.singleClick = getStr(p, "click-policy", QStringLiteral("double")) == QStringLiteral("single");
        c.showHidden = g_settings_get_boolean(p, "show-hidden-files");
        c.showIconText = getStr(p, "show-icon-text", c.showIconText);
        c.confirmTrash = g_settings_get_boolean(p, "confirm-trash");
        c.thumbnails = getStr(p, "show-image-thumbnails", QStringLiteral("local-only"));
        c.dateFormat = getStr(p, "date-format", QStringLiteral("locale"));
        c.iecUnits = g_settings_get_boolean(p, "use-iec-units");
        g_object_unref(p);
    }
    if (GSettings *ui = settingsFor("org.mate.interface")) {
        c.interfaceFont = getStr(ui, "font-name", c.interfaceFont);
        c.iconTheme = getStr(ui, "icon-theme", {});
        g_object_unref(ui);
    }
    if (GSettings *fr = settingsFor("org.mate.font-rendering")) {
        c.font.antialias = getStr(fr, "antialiasing", QStringLiteral("rgba"));
        c.font.hinting = getStr(fr, "hinting", QStringLiteral("slight"));
        c.font.rgbaOrder = getStr(fr, "rgba-order", QStringLiteral("rgb"));
        const double dpi = g_settings_get_double(fr, "dpi");
        if (dpi > 0)
            c.font.dpi = dpi;
        g_object_unref(fr);
    }
    return c;
}

int CajaSettings::captionPieces(int zoomSize)
{
    // smallest 16, smaller 24, small 32 → 0 · standard 48 → 1 · large 72, larger 96 → 2 · largest 192 → 3
    return zoomSize <= 32 ? 0 : zoomSize <= 48 ? 1 : zoomSize <= 96 ? 2 : 3;
}

QString CajaEntry::additionalFor(int pieces) const
{
    QStringList parts;
    for (int k = 0; k < pieces && k < captionTexts.size(); ++k)
        if (!captionTexts.at(k).isNull())
            parts << captionTexts.at(k);
    return parts.join(QLatin1Char('\n'));
}

// Texto de un pie de icono como caja_file_get_string_attribute (los más comunes).
static QString captionText(const QString &attr, GFileInfo *info, const QString &path, bool dir, const CajaSettings &s)
{
    if (attr == QStringLiteral("size")) {
        if (dir) {
            const guint n = guint(QDir(path).entryList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden | QDir::System).size());
            gchar *t = g_strdup_printf(g_dngettext("caja", "%'u item", "%'u items", n), n);
            const QString r = QString::fromUtf8(t);
            g_free(t);
            return r;
        }
        gchar *t = s.iecUnits ? g_format_size_full(guint64(g_file_info_get_size(info)), G_FORMAT_SIZE_IEC_UNITS)
                              : g_format_size(guint64(g_file_info_get_size(info)));
        const QString r = QString::fromUtf8(t);
        g_free(t);
        return r;
    }
    if (attr == QStringLiteral("type") || attr == QStringLiteral("mime_type")) {
        const char *ct = g_file_info_get_content_type(info);
        if (!ct)
            return QStringLiteral("--");
        if (attr == QStringLiteral("mime_type")) {
            gchar *m = g_content_type_get_mime_type(ct);
            const QString r = QString::fromUtf8(m);
            g_free(m);
            return r;
        }
        gchar *dsc = g_content_type_get_description(ct);
        const QString r = QString::fromUtf8(dsc);
        g_free(dsc);
        return r;
    }
    if (attr.startsWith(QStringLiteral("date_"))) {
        const char *key = attr == QStringLiteral("date_accessed") ? G_FILE_ATTRIBUTE_TIME_ACCESS
                        : attr == QStringLiteral("date_changed") ? G_FILE_ATTRIBUTE_TIME_CHANGED : G_FILE_ATTRIBUTE_TIME_MODIFIED;
        const guint64 secs = g_file_info_get_attribute_uint64(info, key);
        GDateTime *dt = g_date_time_new_from_unix_local(gint64(secs));
        gchar *t = g_date_time_format(dt, s.dateFormat == QStringLiteral("iso") ? "%Y-%m-%d %H:%M:%S" : "%c");
        const QString r = QString::fromUtf8(t);
        g_free(t);
        g_date_time_unref(dt);
        return r;
    }
    if (attr == QStringLiteral("owner"))
        return QString::fromUtf8(g_file_info_get_attribute_string(info, G_FILE_ATTRIBUTE_OWNER_USER));
    if (attr == QStringLiteral("group"))
        return QString::fromUtf8(g_file_info_get_attribute_string(info, G_FILE_ATTRIBUTE_OWNER_GROUP));
    if (attr == QStringLiteral("permissions") || attr == QStringLiteral("octal_permissions")) {
        const guint32 m = g_file_info_get_attribute_uint32(info, G_FILE_ATTRIBUTE_UNIX_MODE);
        if (attr == QStringLiteral("octal_permissions"))
            return QString::number(m & 07777, 8);
        QString r = dir ? QStringLiteral("d") : QStringLiteral("-");
        const char *f = "rwxrwxrwx";
        for (int k = 0; k < 9; ++k)
            r += (m & (0400 >> k)) ? QLatin1Char(f[k]) : QLatin1Char('-');
        return r;
    }
    if (attr == QStringLiteral("where"))
        return QFileInfo(path).absolutePath();
    return QStringLiteral("--");
}

QString CajaScan::trName(const char *msgid)
{
    static bool bound = false;
    if (!bound) {
        bind_textdomain_codeset("caja", "UTF-8");
        bound = true;
    }
    return QString::fromUtf8(g_dgettext("caja", msgid));
}

// XSETTINGS (_XSETTINGS_S0 → _XSETTINGS_SETTINGS): Xft/DPI viene en 1024avos de punto por pulgada.
// GTK usa ese valor exacto (p. ej. 96,246), y con 96 redondeado los nombres saldrían 1-3 px más estrechos.
double CajaScan::xsettingsDpi()
{
    xcb_connection_t *c = xcb_connect(nullptr, nullptr);
    if (!c || xcb_connection_has_error(c)) {
        if (c) xcb_disconnect(c);
        return 0;
    }
    auto atom = [c](const char *n) {
        xcb_intern_atom_reply_t *r = xcb_intern_atom_reply(c, xcb_intern_atom(c, 0, uint16_t(std::strlen(n)), n), nullptr);
        const xcb_atom_t a = r ? r->atom : XCB_ATOM_NONE;
        free(r);
        return a;
    };
    double dpi = 0;
    const xcb_atom_t sel = atom("_XSETTINGS_S0"), prop = atom("_XSETTINGS_SETTINGS");
    xcb_get_selection_owner_reply_t *own = xcb_get_selection_owner_reply(c, xcb_get_selection_owner(c, sel), nullptr);
    if (own && own->owner) {
        xcb_get_property_reply_t *pr = xcb_get_property_reply(c, xcb_get_property(c, 0, own->owner, prop, XCB_ATOM_ANY, 0, 1 << 20), nullptr);
        if (pr && xcb_get_property_value_length(pr) > 12) {
            const auto *b = static_cast<const uint8_t *>(xcb_get_property_value(pr));
            const int len = xcb_get_property_value_length(pr);
            const bool msb = b[0] == 1;
            auto u16 = [&](int o) { return msb ? uint16_t(b[o] << 8 | b[o + 1]) : uint16_t(b[o + 1] << 8 | b[o]); };
            auto u32 = [&](int o) {
                return msb ? uint32_t(b[o]) << 24 | uint32_t(b[o + 1]) << 16 | uint32_t(b[o + 2]) << 8 | b[o + 3]
                           : uint32_t(b[o + 3]) << 24 | uint32_t(b[o + 2]) << 16 | uint32_t(b[o + 1]) << 8 | b[o];
            };
            const uint32_t n = u32(8);
            int o = 12;
            for (uint32_t k = 0; k < n && o + 4 <= len; ++k) {
                const uint8_t type = b[o];
                const int nameLen = u16(o + 2);
                const QByteArray name(reinterpret_cast<const char *>(b + o + 4), nameLen);
                o += 4 + ((nameLen + 3) & ~3) + 4;            // nombre (alineado a 4) + último cambio
                if (type == 0) {                               // entero
                    if (o + 4 > len) break;
                    if (name == "Xft/DPI")
                        dpi = int32_t(u32(o)) / 1024.0;
                    o += 4;
                } else if (type == 1) {                        // texto
                    if (o + 4 > len) break;
                    o += 4 + ((int(u32(o)) + 3) & ~3);
                } else {                                       // color
                    o += 8;
                }
            }
        }
        free(pr);
    }
    free(own);
    xcb_disconnect(c);
    return dpi > 0 ? dpi : 0;
}

QString CajaScan::desktopDir()
{
    const char *d = g_get_user_special_dir(G_USER_DIRECTORY_DESKTOP);
    const QString s = d ? QString::fromUtf8(d) : QString();
    return s.isEmpty() || s == QDir::homePath() ? QDir::homePath() + QStringLiteral("/Desktop") : s;
}

static QByteArray collationKey(const QString &name)
{
    gchar *k = g_utf8_collate_key_for_filename(name.toUtf8().constData(), -1);
    const QByteArray r(k);
    g_free(k);
    return r;
}

static void finish(CajaEntry &e)
{
    e.collation = collationKey(e.display);
    e.sortLast = !e.display.isEmpty() && (e.display.at(0) == QLatin1Char('.') || e.display.at(0) == QLatin1Char('#'));
}

// fm_desktop_icon_container_icons_compare + caja_file_compare_for_sort (por nombre).
void CajaScan::sortEntries(QList<CajaEntry> &list, bool directoriesFirst)
{
    std::stable_sort(list.begin(), list.end(), [directoriesFirst](const CajaEntry &a, const CajaEntry &b) {
        if (a.category != b.category)
            return a.category < b.category;
        if (directoriesFirst && a.dir != b.dir)
            return a.dir;
        if (a.sortLast != b.sortLast)
            return b.sortLast;
        return std::strcmp(a.collation.constData(), b.collation.constData()) < 0;
    });
}

// Nombre visible de un lanzador (.desktop de confianza: Caja muestra su Name=, traducido).
static QString launcherName(const QString &path, QString &icon)
{
    GKeyFile *kf = g_key_file_new();
    QString name;
    if (g_key_file_load_from_file(kf, path.toUtf8().constData(), G_KEY_FILE_NONE, nullptr)) {
        if (gchar *n = g_key_file_get_locale_string(kf, "Desktop Entry", "Name", nullptr, nullptr)) {
            name = QString::fromUtf8(n);
            g_free(n);
        }
        if (gchar *i = g_key_file_get_string(kf, "Desktop Entry", "Icon", nullptr)) {
            icon = QString::fromUtf8(i);
            g_free(i);
        }
    }
    g_key_file_free(kf);
    return name;
}

// Todos los nombres del icono, en el orden en que GTK los busca en el tema (el primero que exista).
static QString themedIconName(GIcon *gicon)
{
    QStringList out;
    if (G_IS_THEMED_ICON(gicon)) {
        const gchar *const *names = g_themed_icon_get_names(G_THEMED_ICON(gicon));
        for (int i = 0; names && names[i]; ++i)
            out << QString::fromUtf8(names[i]);
    }
    return out.join(QLatin1Char('|'));
}

QList<CajaEntry> CajaScan::entries(const CajaSettings &s)
{
    QList<CajaEntry> list;
    auto special = [&](const QString &key, const QString &display, const QString &icon, const QString &uri, int cat) {
        CajaEntry e;
        e.key = key;
        e.special = true;
        e.display = display;
        e.iconName = icon;
        e.uri = uri;
        e.category = cat;
        finish(e);
        list << e;
    };
    if (s.computerVisible)
        special(QStringLiteral("computer"), trName("Computer"), QStringLiteral("computer"), QStringLiteral("computer:///"), 0);
    if (s.homeVisible)
        special(QStringLiteral("home"), trName("%s's Home").replace(QStringLiteral("%s"), QString::fromUtf8(g_get_user_name())),
                QStringLiteral("user-home"), QUrl::fromLocalFile(QDir::homePath()).toString(), 1);
    if (s.networkVisible)
        special(QStringLiteral("network"), trName("Network Servers"), QStringLiteral("network-workgroup"), QStringLiteral("network:///"), 2);
    if (s.trashVisible) {
        GFile *t = g_file_new_for_uri("trash:///");
        GFileInfo *ti = g_file_query_info(t, "trash::item-count", G_FILE_QUERY_INFO_NONE, nullptr, nullptr);
        const bool full = ti && g_file_info_get_attribute_uint32(ti, "trash::item-count") > 0;
        if (ti) g_object_unref(ti);
        g_object_unref(t);
        special(QStringLiteral("trash"), trName("Trash"), full ? QStringLiteral("user-trash-full") : QStringLiteral("user-trash"),
                QStringLiteral("trash:///"), 5);
    }

    const QString dir = desktopDir();
    QDir::Filters f = QDir::AllEntries | QDir::NoDotAndDotDot | QDir::System;
    if (s.showHidden)
        f |= QDir::Hidden;
    QMimeDatabase mimes;
    for (const QFileInfo &fi : QDir(dir).entryInfoList(f, QDir::Name)) {
        if (!s.showHidden && fi.fileName().endsWith(QLatin1Char('~')))
            continue;   // copias de seguridad: ocultas como en Caja
        CajaEntry e;
        e.key = fi.fileName();
        e.path = fi.absoluteFilePath();
        e.uri = QUrl::fromLocalFile(e.path).toString();
        e.dir = fi.isDir();
        e.hidden = fi.isHidden();
        GFile *gf = g_file_new_for_path(e.path.toUtf8().constData());
        GFileInfo *info = g_file_query_info(gf, "standard::*,time::*,owner::*,unix::mode", G_FILE_QUERY_INFO_NONE, nullptr, nullptr);
        if (info) {
            e.display = QString::fromUtf8(g_file_info_get_display_name(info));
            e.iconName = themedIconName(g_file_info_get_icon(info));
            // Texto en los iconos (caja_file_get_icon con EMBEDDING_TEXT): los archivos de texto que usarían
            // «text-x-generic» van primero con «text-x-preview» (la hoja en blanco de los temas).
            const char *ct = g_file_info_get_content_type(info);
            if (!e.dir && ct && s.showIconText != QStringLiteral("never") && g_content_type_is_a(ct, "text/plain")
                && e.iconName.split(QLatin1Char('|')).contains(QStringLiteral("text-x-generic")))
                e.iconName.prepend(QStringLiteral("text-x-preview|"));
            const bool isLauncher = !e.dir && fi.suffix() == QStringLiteral("desktop");   // enlaces de Caja: sin pies
            for (int k = 0; k < 3 && k < s.captions.size(); ++k)
                e.captionTexts << (s.captions.at(k) == QStringLiteral("none") || isLauncher
                                       ? QString() : captionText(s.captions.at(k), info, e.path, e.dir, s));
            g_object_unref(info);
        }
        g_object_unref(gf);
        if (e.display.isEmpty())
            e.display = fi.fileName();
        if (!e.dir && fi.suffix() == QStringLiteral("desktop")) {
            QString icon;
            const QString n = launcherName(e.path, icon);
            if (!n.isEmpty()) {
                e.display = n;
                e.launcher = true;
            }
            if (!icon.isEmpty())
                e.iconName = icon;
        }
        if (!e.dir && s.thumbnails != QStringLiteral("never")) {
            const QString m = mimes.mimeTypeForFile(fi).name();
            if (m.startsWith(QStringLiteral("image/")) && fi.size() < 40ll * 1024 * 1024)
                e.thumb = QUrl::fromLocalFile(e.path).toString();
        }
        finish(e);
        list << e;
    }
    // Comprobado contra Caja 1.28: al colocar, los iconos especiales se ordenan por nombre junto con los
    // archivos (carpetas primero), sin la categoría de fm-icon-container.c.
    for (CajaEntry &e : list)
        e.category = 4;
    sortEntries(list, s.directoriesFirst);
    for (CajaEntry &e : list)
        e.positioned = readPosition(e, e.x, e.y);
    return list;
}

static QString desktopMetadataPath()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + QStringLiteral("/caja/desktop-metadata");
}

static bool parsePos(const QString &v, int &x, int &y)
{
    const QStringList p = v.split(QLatin1Char(','));
    if (p.size() != 2)
        return false;
    bool a = false, b = false;
    x = p.at(0).trimmed().toInt(&a);
    y = p.at(1).trimmed().toInt(&b);
    return a && b;
}

bool CajaScan::readPosition(const CajaEntry &e, int &x, int &y)
{
    if (e.special) {
        GKeyFile *kf = g_key_file_new();
        bool ok = false;
        if (g_key_file_load_from_file(kf, desktopMetadataPath().toUtf8().constData(), G_KEY_FILE_NONE, nullptr)) {
            if (gchar *v = g_key_file_get_string(kf, e.key.toUtf8().constData(), "caja-icon-position", nullptr)) {
                ok = parsePos(QString::fromUtf8(v), x, y);
                g_free(v);
            }
        }
        g_key_file_free(kf);
        return ok;
    }
    GFile *gf = g_file_new_for_path(e.path.toUtf8().constData());
    GFileInfo *info = g_file_query_info(gf, "metadata::caja-icon-position", G_FILE_QUERY_INFO_NONE, nullptr, nullptr);
    bool ok = false;
    if (info) {
        if (const char *v = g_file_info_get_attribute_string(info, "metadata::caja-icon-position"))
            ok = parsePos(QString::fromUtf8(v), x, y);
        g_object_unref(info);
    }
    g_object_unref(gf);
    return ok;
}

bool CajaScan::writePosition(const CajaEntry &e, int x, int y)
{
    const QByteArray v = QStringLiteral("%1,%2").arg(x).arg(y).toUtf8();
    if (e.special) {
        const QByteArray file = desktopMetadataPath().toUtf8();
        GKeyFile *kf = g_key_file_new();
        g_key_file_load_from_file(kf, file.constData(), G_KEY_FILE_KEEP_COMMENTS, nullptr);
        g_key_file_set_string(kf, e.key.toUtf8().constData(), "caja-icon-position", v.constData());
        QDir().mkpath(QFileInfo(QString::fromUtf8(file)).absolutePath());
        const bool ok = g_key_file_save_to_file(kf, file.constData(), nullptr);
        g_key_file_free(kf);
        return ok;
    }
    GFile *gf = g_file_new_for_path(e.path.toUtf8().constData());
    const bool ok = g_file_set_attribute_string(gf, "metadata::caja-icon-position", v.constData(), G_FILE_QUERY_INFO_NONE, nullptr, nullptr);
    g_object_unref(gf);
    return ok;
}

bool CajaScan::clearPosition(const CajaEntry &e)
{
    if (e.special) {
        const QByteArray file = desktopMetadataPath().toUtf8();
        GKeyFile *kf = g_key_file_new();
        bool ok = g_key_file_load_from_file(kf, file.constData(), G_KEY_FILE_KEEP_COMMENTS, nullptr);
        if (ok) {
            g_key_file_remove_key(kf, e.key.toUtf8().constData(), "caja-icon-position", nullptr);
            ok = g_key_file_save_to_file(kf, file.constData(), nullptr);
        }
        g_key_file_free(kf);
        return ok;
    }
    GFile *gf = g_file_new_for_path(e.path.toUtf8().constData());
    const bool ok = g_file_set_attribute(gf, "metadata::caja-icon-position", G_FILE_ATTRIBUTE_TYPE_INVALID, nullptr,
                                         G_FILE_QUERY_INFO_NONE, nullptr, nullptr);
    g_object_unref(gf);
    return ok;
}
