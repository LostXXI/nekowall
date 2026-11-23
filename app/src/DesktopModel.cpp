// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
#include "DesktopModel.h"

#include <QClipboard>
#include <QCollator>
#include <QCoreApplication>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QFontDatabase>
#include <QFontMetrics>
#include <QFontInfo>
#include <cstdio>
#include <QGuiApplication>
#include <QIcon>
#include <QImageReader>
#include <QLocale>
#include <QMimeData>
#include <QMimeDatabase>
#include <QProcess>
#include <QRegularExpression>
#include <QSet>
#include <QSettings>
#include <QStandardPaths>
#include <QStorageInfo>
#include <QTextStream>
#include <algorithm>

// ── Iconos del tema ──────────────────────────────────────────────────────────

QPixmap ThemeIconProvider::requestPixmap(const QString &id, QSize *size, const QSize &requested)
{
    const int px = requested.width() > 0 ? requested.width() : 48;
    QIcon icon;
    if (QFileInfo(id).isAbsolute() && QFileInfo::exists(id))
        icon = QIcon(id);                                  // Icon= con ruta completa
    else
        icon = QIcon::fromTheme(id);
    for (const char *alt : {"text-x-generic", "unknown", "application-x-executable"})
        if (icon.isNull())
            icon = QIcon::fromTheme(QString::fromLatin1(alt));
    const QPixmap pm = icon.pixmap(px, px);
    if (size)
        *size = pm.size();
    return pm;
}

// Fuera de KDE, Qt a menudo no sabe qué tema de iconos usa el sistema (sin qt6ct ni portal): se toma el
// de GTK (~/.config/gtk-3.0/settings.ini) o el primero instalado de los habituales.
static void pickIconTheme()
{
    if (QIcon::hasThemeIcon(QStringLiteral("folder")))
        return;
    QStringList candidates;
    // LXQt (lxqt.conf: icon_theme=…), luego GTK.
    QFile lxqt(QDir::homePath() + QStringLiteral("/.config/lxqt/lxqt.conf"));
    if (lxqt.open(QIODevice::ReadOnly | QIODevice::Text)) {
        for (const QString &l : QString::fromUtf8(lxqt.readAll()).split(QLatin1Char('\n')))
            if (l.trimmed().startsWith(QStringLiteral("icon_theme")))
                candidates << l.section(QLatin1Char('='), 1).trimmed();
    }
    QFile gtk(QDir::homePath() + QStringLiteral("/.config/gtk-3.0/settings.ini"));
    if (gtk.open(QIODevice::ReadOnly | QIODevice::Text)) {
        for (const QString &l : QString::fromUtf8(gtk.readAll()).split(QLatin1Char('\n')))
            if (l.trimmed().startsWith(QStringLiteral("gtk-icon-theme-name")))
                candidates << l.section(QLatin1Char('='), 1).trimmed();
    }
    candidates << QStringLiteral("Papirus-Dark") << QStringLiteral("Papirus") << QStringLiteral("Adwaita")
               << QStringLiteral("breeze-dark") << QStringLiteral("breeze") << QStringLiteral("elementary")
               << QStringLiteral("Tela") << QStringLiteral("gnome");
    for (const QString &t : std::as_const(candidates)) {
        for (const QString &base : QIcon::themeSearchPaths())
            if (QFileInfo::exists(base + QLatin1Char('/') + t + QStringLiteral("/index.theme"))) {
                QIcon::setThemeName(t);
                if (QIcon::hasThemeIcon(QStringLiteral("folder")))
                    return;
            }
    }
}

// ── Utilidades ───────────────────────────────────────────────────────────────

static QString desktopDir()
{
    const QString d = QStandardPaths::writableLocation(QStandardPaths::DesktopLocation);
    return d.isEmpty() ? QDir::homePath() + QStringLiteral("/Desktop") : d;
}

static QString trashDir()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation) + QStringLiteral("/Trash");
}

static bool hasProgram(const QString &name)
{
    return !QStandardPaths::findExecutable(name).isEmpty();
}

// Terminal para lanzadores con Terminal=true y «Abrir una terminal aquí».
static QStringList terminalCommand()
{
    const QString env = qEnvironmentVariable("TERMINAL");
    if (!env.isEmpty() && hasProgram(env))
        return {env};
    for (const char *t : {"qterminal", "foot", "alacritty", "kitty", "xfce4-terminal", "konsole", "lxterminal",
                          "mate-terminal", "gnome-terminal", "xterm"})
        if (hasProgram(QString::fromLatin1(t)))
            return {QString::fromLatin1(t)};
    return {};
}

static QString localized(const QHash<QString, QString> &keys, const QString &key)
{
    const QString lang = QLocale().name();                         // es_CL
    const QString shortLang = lang.section(QLatin1Char('_'), 0, 0);  // es
    for (const QString &k : {QStringLiteral("%1[%2]").arg(key, lang), QStringLiteral("%1[%2]").arg(key, shortLang), key})
        if (keys.contains(k))
            return keys.value(k);
    return {};
}

// Claves de [Desktop Entry] de un archivo .desktop.
static QHash<QString, QString> readDesktopEntry(const QString &path)
{
    QHash<QString, QString> keys;
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly | QIODevice::Text))
        return keys;
    bool inEntry = false;
    QTextStream in(&f);
    while (!in.atEnd()) {
        const QString line = in.readLine().trimmed();
        if (line.startsWith(QLatin1Char('['))) {
            inEntry = line == QStringLiteral("[Desktop Entry]");
            continue;
        }
        const qsizetype eq = line.indexOf(QLatin1Char('='));
        if (inEntry && eq > 0 && !line.startsWith(QLatin1Char('#')))
            keys.insert(line.left(eq).trimmed(), line.mid(eq + 1).trimmed());
    }
    return keys;
}

// ── Aplicaciones instaladas (para «Abrir con») ───────────────────────────────

namespace {
struct App {
    QString id, name, icon, exec;
    QStringList mimes;
    bool terminal = false;
};
}

static const QList<App> &installedApps()
{
    static QList<App> apps;
    static bool loaded = false;
    if (loaded)
        return apps;
    loaded = true;
    QSet<QString> seen;
    QStringList dirs = QStandardPaths::standardLocations(QStandardPaths::ApplicationsLocation);
    for (const QString &dir : std::as_const(dirs)) {
        QDirIterator it(dir, {QStringLiteral("*.desktop")}, QDir::Files, QDirIterator::Subdirectories);
        while (it.hasNext()) {
            const QString path = it.next();
            QString id = QDir(dir).relativeFilePath(path);
            id.replace(QLatin1Char('/'), QLatin1Char('-'));
            if (seen.contains(id))
                continue;                       // el del usuario tiene prioridad
            seen.insert(id);
            const auto keys = readDesktopEntry(path);
            if (keys.value(QStringLiteral("Type")) != QStringLiteral("Application")
                || keys.value(QStringLiteral("NoDisplay")) == QStringLiteral("true")
                || keys.value(QStringLiteral("Hidden")) == QStringLiteral("true")
                || keys.value(QStringLiteral("Exec")).isEmpty())
                continue;
            App a;
            a.id = id;
            a.name = localized(keys, QStringLiteral("Name"));
            a.icon = keys.value(QStringLiteral("Icon"));
            a.exec = keys.value(QStringLiteral("Exec"));
            a.terminal = keys.value(QStringLiteral("Terminal")) == QStringLiteral("true");
            a.mimes = keys.value(QStringLiteral("MimeType")).split(QLatin1Char(';'), Qt::SkipEmptyParts);
            apps << a;
        }
    }
    return apps;
}

// Programa predeterminado para un tipo (mimeapps.list).
static QString defaultAppFor(const QString &mime)
{
    QStringList files;
    for (const QString &d : QStandardPaths::standardLocations(QStandardPaths::GenericConfigLocation))
        files << d + QStringLiteral("/mimeapps.list");
    for (const QString &d : QStandardPaths::standardLocations(QStandardPaths::ApplicationsLocation))
        files << d + QStringLiteral("/mimeapps.list");
    for (const QString &file : std::as_const(files)) {
        QSettings s(file, QSettings::IniFormat);
        const QVariant v = s.value(QStringLiteral("Default Applications/") + mime);
        const QStringList ids = v.typeId() == QMetaType::QStringList ? v.toStringList()
                                                                     : v.toString().split(QLatin1Char(';'), Qt::SkipEmptyParts);
        if (!ids.isEmpty())
            return ids.first().trimmed();
    }
    return {};
}

// ── Modelo ───────────────────────────────────────────────────────────────────

DesktopModel::DesktopModel(const QString &screenName, QObject *parent)
    : QAbstractListModel(parent)
    , m_screenName(screenName)
    , m_folder(desktopDir())
{
    QDir().mkpath(m_folder);
    pickIconTheme();

    // Perfil de pcmanfm-qt (el mismo que usaba su escritorio); si no hay, uno propio con el mismo formato.
    const QString base = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + QStringLiteral("/pcmanfm-qt/");
    QString profile = qEnvironmentVariable("NEKOWALL_PCMANFM_PROFILE");
    if (profile.isEmpty())
        for (const char *p : {"default", "lxqt"})
            if (QFileInfo::exists(base + QString::fromLatin1(p) + QStringLiteral("/settings.conf"))) {
                profile = QString::fromLatin1(p);
                break;
            }
    m_profileDir = profile.isEmpty()
        ? QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + QStringLiteral("/nekowall/escritorio")
        : base + profile;
    QDir().mkpath(m_profileDir);
    m_settingsFile = m_profileDir + QStringLiteral("/settings.conf");
    m_positionsFile = m_profileDir + QStringLiteral("/desktop-items-%1.conf")
                                         .arg(m_screenName.isEmpty() ? QStringLiteral("0") : m_screenName);

    m_reloadTimer.setSingleShot(true);
    m_reloadTimer.setInterval(150);
    connect(&m_reloadTimer, &QTimer::timeout, this, &DesktopModel::reload);
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, this, [this](const QString &dir) {
        if (dir.startsWith(trashDir()))
            updateTrashShortcut();
        else
            m_reloadTimer.start();
    });
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, this, [this](const QString &file) {
        // Los programas guardan reemplazando el archivo: hay que volver a vigilarlo.
        QTimer::singleShot(200, this, [this, file] {
            if (!m_watcher.files().contains(file) && QFileInfo::exists(file))
                m_watcher.addPath(file);
            loadSettings();
            reload();
        });
    });
    m_watcher.addPath(m_folder);
    if (QFileInfo::exists(m_settingsFile))
        m_watcher.addPath(m_settingsFile);
    QDir().mkpath(trashDir() + QStringLiteral("/files"));
    m_watcher.addPath(trashDir() + QStringLiteral("/files"));

    loadSettings();
    loadPositions();
    reload();
}

int DesktopModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : int(m_items.size());
}

QVariant DesktopModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_items.size())
        return {};
    const Item &it = m_items.at(index.row());
    switch (role) {
    case NameRole: return it.name;
    case FileNameRole: return it.fileName;
    case PathRole: return it.path;
    case IconRole: return it.icon;
    case ThumbRole: return it.thumb;
    case IsDirRole: return it.dir;
    case IsLauncherRole: return it.launcher;
    case XRole: return it.pos.x();
    case YRole: return it.pos.y();
    case SelectedRole: return it.selected;
    case StickyRole: return m_customStore.contains(it.fileName);
    case CutRole: return it.cut;
    case IsTrashRole: return it.fileName == QStringLiteral("trash-can.desktop");
    case HiddenRole: return it.hidden;
    }
    return {};
}

QHash<int, QByteArray> DesktopModel::roleNames() const
{
    return {{NameRole, "name"}, {FileNameRole, "fileName"}, {PathRole, "path"}, {IconRole, "icon"},
            {ThumbRole, "thumb"}, {IsDirRole, "isDir"}, {IsLauncherRole, "isLauncher"}, {XRole, "posX"},
            {YRole, "posY"}, {SelectedRole, "selected"}, {StickyRole, "sticky"}, {CutRole, "cut"},
            {IsTrashRole, "isTrash"}, {HiddenRole, "hiddenFile"}};
}

// ── Ajustes de pcmanfm-qt ────────────────────────────────────────────────────

void DesktopModel::loadSettings()
{
    QSettings s(m_settingsFile, QSettings::IniFormat);
    s.beginGroup(QStringLiteral("Desktop"));
    m_iconSize = std::clamp(s.value(QStringLiteral("DesktopIconSize"), 48).toInt(), 16, 256);
    m_baseMargins = s.value(QStringLiteral("DesktopCellMargins"), QSize(3, 1)).toSize()
                        .expandedTo(QSize(0, 0)).boundedTo(QSize(48, 48));
    const QString font = s.value(QStringLiteral("Font")).toString();
    m_font = QGuiApplication::font();
    if (!font.isEmpty())
        m_font.fromString(font);
    m_fg = QColor(s.value(QStringLiteral("FgColor"), QStringLiteral("#ffffff")).toString());
    if (!m_fg.isValid())
        m_fg = Qt::white;
    m_shadow = QColor(s.value(QStringLiteral("ShadowColor"), QStringLiteral("#000000")).toString());
    m_showHidden = s.value(QStringLiteral("ShowHidden"), false).toBool();
    m_sortColumn = s.value(QStringLiteral("SortColumn"), QStringLiteral("name")).toString();
    m_sortAscending = s.value(QStringLiteral("SortOrder"), QStringLiteral("ascending")).toString() != QStringLiteral("descending");
    m_folderFirst = s.value(QStringLiteral("SortFolderFirst"), true).toBool();
    m_hiddenLast = s.value(QStringLiteral("SortHiddenLast"), false).toBool();
    m_allSticky = s.value(QStringLiteral("AllSticky"), false).toBool();
    m_hideItems = s.value(m_screenName.isEmpty() ? QStringLiteral("HideItems")
                                                 : QStringLiteral("HideItems-%1").arg(m_screenName), false).toBool();
    m_noTooltip = s.value(QStringLiteral("NoItemTooltip"), false).toBool();
    m_defaultFm = s.value(QStringLiteral("OpenWithDefaultFileManager"), false).toBool();
    m_shortcuts = s.value(QStringLiteral("DesktopShortcuts")).toStringList();
    const QVariantList l = s.value(QStringLiteral("WorkAreaMargins")).toList();
    m_workMargins = QMargins(12, 12, 12, 12);
    if (l.size() >= 4)
        m_workMargins = QMargins(std::clamp(l.at(0).toInt(), 0, 200), std::clamp(l.at(1).toInt(), 0, 200),
                                 std::clamp(l.at(2).toInt(), 0, 200), std::clamp(l.at(3).toInt(), 0, 200));
    s.endGroup();
    s.beginGroup(QStringLiteral("Behavior"));
    m_singleClick = s.value(QStringLiteral("SingleClick"), false).toBool();
    m_confirmDelete = s.value(QStringLiteral("ConfirmDelete"), true).toBool();
    m_confirmTrash = s.value(QStringLiteral("ConfirmTrash"), false).toBool();
    m_useTrash = s.value(QStringLiteral("UseTrash"), true).toBool();
    s.endGroup();
    m_thumbnails = s.value(QStringLiteral("Thumbnail/ShowThumbnails"), true).toBool();
    m_backupAsHidden = s.value(QStringLiteral("FolderView/BackupAsHidden"), false).toBool();
    emit settingsChanged();
    ensureShortcuts();
}

void DesktopModel::writeSetting(const QString &key, const QVariant &value)
{
    {
        QSettings s(m_settingsFile, QSettings::IniFormat);
        s.setValue(key, value);
    }
    if (!m_watcher.files().contains(m_settingsFile))
        m_watcher.addPath(m_settingsFile);
    loadSettings();
    reload();
}

QVariantMap DesktopModel::prefs() const
{
    return {{QStringLiteral("iconSize"), m_iconSize},
            {QStringLiteral("fontFamily"), m_font.family()},
            {QStringLiteral("fontSize"), m_font.pointSize() > 0 ? m_font.pointSize() : 9},
            {QStringLiteral("fgColor"), m_fg.name()},
            {QStringLiteral("shadowColor"), m_shadow.isValid() ? m_shadow.name() : QStringLiteral("#000000")},
            {QStringLiteral("shadowOn"), m_shadow.isValid()},
            {QStringLiteral("singleClick"), m_singleClick},
            {QStringLiteral("showHidden"), m_showHidden},
            {QStringLiteral("marginW"), m_baseMargins.width()},
            {QStringLiteral("marginH"), m_baseMargins.height()},
            {QStringLiteral("workLeft"), m_workMargins.left()},
            {QStringLiteral("workTop"), m_workMargins.top()},
            {QStringLiteral("workRight"), m_workMargins.right()},
            {QStringLiteral("workBottom"), m_workMargins.bottom()},
            {QStringLiteral("tooltips"), !m_noTooltip},
            {QStringLiteral("allSticky"), m_allSticky},
            {QStringLiteral("defaultFm"), m_defaultFm},
            {QStringLiteral("shortcuts"), m_shortcuts},
            {QStringLiteral("folder"), m_folder},
            {QStringLiteral("profile"), QFileInfo(m_profileDir).fileName()}};
}

void DesktopModel::savePrefs(const QVariantMap &p)
{
    {
        QSettings s(m_settingsFile, QSettings::IniFormat);
        QFont f = m_font;
        f.setFamily(p.value(QStringLiteral("fontFamily"), m_font.family()).toString());
        f.setPointSize(std::clamp(p.value(QStringLiteral("fontSize"), 9).toInt(), 5, 72));
        s.setValue(QStringLiteral("Desktop/DesktopIconSize"), p.value(QStringLiteral("iconSize"), 48).toInt());
        s.setValue(QStringLiteral("Desktop/Font"), f.toString());
        s.setValue(QStringLiteral("Desktop/FgColor"), p.value(QStringLiteral("fgColor")).toString());
        s.setValue(QStringLiteral("Desktop/ShadowColor"),
                   p.value(QStringLiteral("shadowOn")).toBool() ? p.value(QStringLiteral("shadowColor")).toString() : QString());
        s.setValue(QStringLiteral("Behavior/SingleClick"), p.value(QStringLiteral("singleClick")).toBool());
        s.setValue(QStringLiteral("Desktop/ShowHidden"), p.value(QStringLiteral("showHidden")).toBool());
        s.setValue(QStringLiteral("Desktop/DesktopCellMargins"),
                   QSize(p.value(QStringLiteral("marginW"), 3).toInt(), p.value(QStringLiteral("marginH"), 1).toInt()));
        s.setValue(QStringLiteral("Desktop/WorkAreaMargins"),
                   QVariantList{p.value(QStringLiteral("workLeft"), 12).toInt(), p.value(QStringLiteral("workTop"), 12).toInt(),
                                p.value(QStringLiteral("workRight"), 12).toInt(), p.value(QStringLiteral("workBottom"), 12).toInt()});
        s.setValue(QStringLiteral("Desktop/NoItemTooltip"), !p.value(QStringLiteral("tooltips"), true).toBool());
        s.setValue(QStringLiteral("Desktop/AllSticky"), p.value(QStringLiteral("allSticky")).toBool());
        s.setValue(QStringLiteral("Desktop/OpenWithDefaultFileManager"), p.value(QStringLiteral("defaultFm")).toBool());
        if (p.contains(QStringLiteral("shortcuts"))) {
            const QStringList sc = p.value(QStringLiteral("shortcuts")).toStringList();
            // Como pcmanfm-qt: los accesos que se quitan se borran del escritorio.
            const QList<QPair<QString, QString>> files{{QStringLiteral("Home"), QStringLiteral("user-home.desktop")},
                                                       {QStringLiteral("Trash"), QStringLiteral("trash-can.desktop")},
                                                       {QStringLiteral("Computer"), QStringLiteral("computer.desktop")},
                                                       {QStringLiteral("Network"), QStringLiteral("network.desktop")}};
            for (const auto &f : files)
                if (m_shortcuts.contains(f.first) && !sc.contains(f.first))
                    QFile::remove(m_folder + QLatin1Char('/') + f.second);
            s.setValue(QStringLiteral("Desktop/DesktopShortcuts"), sc);
        }
    }
    if (!m_watcher.files().contains(m_settingsFile))
        m_watcher.addPath(m_settingsFile);
    loadSettings();
    reload();
}

QStringList DesktopModel::fontFamilies() const
{
    return QFontDatabase::families();
}

void DesktopModel::setSort(const QString &column) { writeSetting(QStringLiteral("Desktop/SortColumn"), column); }
void DesktopModel::setSortAscending(bool on)
{
    writeSetting(QStringLiteral("Desktop/SortOrder"), on ? QStringLiteral("ascending") : QStringLiteral("descending"));
}
void DesktopModel::setFolderFirst(bool on) { writeSetting(QStringLiteral("Desktop/SortFolderFirst"), on); }
void DesktopModel::setHiddenLast(bool on) { writeSetting(QStringLiteral("Desktop/SortHiddenLast"), on); }
void DesktopModel::setShowHidden(bool on) { writeSetting(QStringLiteral("Desktop/ShowHidden"), on); }
void DesktopModel::setHideItems(bool on)
{
    writeSetting(m_screenName.isEmpty() ? QStringLiteral("Desktop/HideItems")
                                        : QStringLiteral("Desktop/HideItems-%1").arg(m_screenName), on);
}

// ── Accesos de pcmanfm-qt (Inicio, Papelera, Equipo, Red) ────────────────────

void DesktopModel::ensureShortcuts()
{
    const bool pcm = hasProgram(QStringLiteral("pcmanfm-qt"));
    auto write = [this](const QString &file, const QString &content) {
        QFile f(m_folder + QLatin1Char('/') + file);
        if (f.exists())
            return;
        if (f.open(QIODevice::WriteOnly | QIODevice::Text))
            f.write(content.toUtf8());
    };
    const QString home = QDir::homePath();
    if (m_shortcuts.contains(QStringLiteral("Home")))
        write(QStringLiteral("user-home.desktop"),
              QStringLiteral("[Desktop Entry]\nType=Application\nExec=%1 %2\nIcon=user-home\nName=%3\n")
                  .arg(pcm ? QStringLiteral("pcmanfm-qt") : QStringLiteral("xdg-open"), home, qEnvironmentVariable("USER")));
    if (m_shortcuts.contains(QStringLiteral("Computer")))
        write(QStringLiteral("computer.desktop"),
              QStringLiteral("[Desktop Entry]\nType=Application\nExec=pcmanfm-qt computer:///\nIcon=computer\nName=Equipo\n"));
    if (m_shortcuts.contains(QStringLiteral("Network")))
        write(QStringLiteral("network.desktop"),
              QStringLiteral("[Desktop Entry]\nType=Application\nExec=pcmanfm-qt network:///\nIcon=folder-network\nName=Red\n"));
    updateTrashShortcut();
}

int DesktopModel::trashCount() const
{
    return int(QDir(trashDir() + QStringLiteral("/files")).entryList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden).size());
}

void DesktopModel::updateTrashShortcut()
{
    if (!m_shortcuts.contains(QStringLiteral("Trash")) || !m_useTrash)
        return;
    const int n = trashCount();
    const QString name = n == 0 ? QStringLiteral("Papelera (vacía)")
                       : n == 1 ? QStringLiteral("Papelera (un elemento)")
                                : QStringLiteral("Papelera (%1 elementos)").arg(n);
    const QString content = QStringLiteral("[Desktop Entry]\nType=Application\nExec=%1 trash:///\nIcon=%2\nName=%3\n")
        .arg(hasProgram(QStringLiteral("pcmanfm-qt")) ? QStringLiteral("pcmanfm-qt") : QStringLiteral("xdg-open"),
             n > 0 ? QStringLiteral("user-trash-full") : QStringLiteral("user-trash"), name);
    QFile f(m_folder + QStringLiteral("/trash-can.desktop"));
    if (f.open(QIODevice::ReadOnly) && QString::fromUtf8(f.readAll()) == content)
        return;
    f.close();
    if (f.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text))
        f.write(content.toUtf8());
}

void DesktopModel::emptyTrash()
{
    const QString t = trashDir();
    QDir(t + QStringLiteral("/files")).removeRecursively();
    QDir(t + QStringLiteral("/info")).removeRecursively();
    QDir().mkpath(t + QStringLiteral("/files"));
    QDir().mkpath(t + QStringLiteral("/info"));
    m_watcher.addPath(t + QStringLiteral("/files"));
    updateTrashShortcut();
    emit message(QStringLiteral("Papelera vaciada"));
}

// ── Lista de archivos ────────────────────────────────────────────────────────

DesktopModel::Item DesktopModel::readLauncher(const QString &path)
{
    Item it;
    it.path = path;
    it.fileName = QFileInfo(path).fileName();
    it.launcher = true;
    it.name = QFileInfo(path).completeBaseName();
    it.icon = QStringLiteral("application-x-executable");
    const auto keys = readDesktopEntry(path);
    const QString name = localized(keys, QStringLiteral("Name"));
    if (!name.isEmpty())
        it.name = name;
    if (!keys.value(QStringLiteral("Icon")).isEmpty())
        it.icon = keys.value(QStringLiteral("Icon"));
    it.exec = keys.value(QStringLiteral("Exec"));
    it.terminal = keys.value(QStringLiteral("Terminal")) == QStringLiteral("true");
    if (keys.value(QStringLiteral("Type")) == QStringLiteral("Link"))
        it.url = keys.value(QStringLiteral("URL"));
    it.mime = QStringLiteral("application/x-desktop");
    return it;
}

void DesktopModel::sortItems(QList<Item> &items) const
{
    QCollator coll;
    coll.setNumericMode(true);
    coll.setCaseSensitivity(Qt::CaseInsensitive);
    QMimeDatabase mimes;
    const QString col = m_sortColumn;
    std::stable_sort(items.begin(), items.end(), [&](const Item &a, const Item &b) {
        if (m_folderFirst && a.dir != b.dir)
            return a.dir;
        if (m_hiddenLast && a.hidden != b.hidden)
            return !a.hidden;
        int c = 0;
        if (col == QStringLiteral("size"))
            c = a.size < b.size ? -1 : a.size > b.size ? 1 : 0;
        else if (col == QStringLiteral("mtime") || col == QStringLiteral("crtime") || col == QStringLiteral("dtime"))
            c = a.mtime < b.mtime ? -1 : a.mtime > b.mtime ? 1 : 0;
        else if (col == QStringLiteral("type"))
            c = coll.compare(mimes.mimeTypeForName(a.mime).comment(), mimes.mimeTypeForName(b.mime).comment());
        if (c == 0)
            c = coll.compare(a.name, b.name);
        return m_sortAscending ? c < 0 : c > 0;
    });
}

void DesktopModel::reload()
{
    QHash<QString, bool> selected, cut;
    for (const Item &it : std::as_const(m_items)) {
        if (it.selected) selected.insert(it.fileName, true);
        if (it.cut) cut.insert(it.fileName, true);
    }
    const QString currentName = m_current >= 0 && m_current < m_items.size() ? m_items.at(m_current).fileName : QString();

    QList<Item> items;
    QMimeDatabase mimes;
    const QList<QByteArray> imageTypes = QImageReader::supportedMimeTypes();
    QDir::Filters filter = QDir::AllEntries | QDir::NoDotAndDotDot | QDir::System;
    if (m_showHidden)
        filter |= QDir::Hidden;
    const QFileInfoList entries = QDir(m_folder).entryInfoList(filter, QDir::Name);
    for (const QFileInfo &fi : entries) {
        const bool backup = fi.fileName().endsWith(QLatin1Char('~'));
        if (backup && m_backupAsHidden && !m_showHidden)
            continue;
        Item it;
        if (fi.fileName().endsWith(QStringLiteral(".desktop")) && fi.isFile()) {
            it = readLauncher(fi.absoluteFilePath());
        } else {
            it.name = fi.fileName();
            it.fileName = fi.fileName();
            it.path = fi.absoluteFilePath();
            it.dir = fi.isDir();
            const QMimeType m = it.dir ? mimes.mimeTypeForName(QStringLiteral("inode/directory")) : mimes.mimeTypeForFile(fi);
            it.mime = m.name();
            if (it.dir)
                it.icon = QStringLiteral("folder");
            else
                it.icon = QIcon::hasThemeIcon(m.iconName()) ? m.iconName() : m.genericIconName();
            if (m_thumbnails && !it.dir && fi.size() < 40ll * 1024 * 1024 && imageTypes.contains(m.name().toLatin1()))
                it.thumb = QUrl::fromLocalFile(it.path).toString();
        }
        it.hidden = fi.isHidden() || (backup && m_backupAsHidden);
        it.size = fi.size();
        it.mtime = fi.lastModified().toSecsSinceEpoch();
        it.selected = selected.contains(it.fileName);
        it.cut = cut.contains(it.fileName);
        items << it;
    }
    sortItems(items);

    // Archivos soltados/pegados: quedan fijos donde se soltaron (como pcmanfm-qt).
    bool posChanged = false;
    for (auto p = m_pending.begin(); p != m_pending.end();) {
        if (QFileInfo::exists(m_folder + QLatin1Char('/') + p.key())) {
            m_customStore[p.key()] = p.value();
            posChanged = true;
            p = m_pending.erase(p);
        } else {
            ++p;
        }
    }
    // Posiciones de archivos que ya no existen: fuera (onRowsAboutToBeRemoved de pcmanfm-qt).
    for (auto c = m_customStore.begin(); c != m_customStore.end();) {
        if (!QFileInfo::exists(m_folder + QLatin1Char('/') + c.key())) {
            c = m_customStore.erase(c);
            posChanged = true;
        } else {
            ++c;
        }
    }

    beginResetModel();
    m_items = items;
    m_current = -1;
    for (int i = 0; i < m_items.size(); ++i)
        if (m_items.at(i).fileName == currentName)
            m_current = i;
    endResetModel();
    relayout();
    if (posChanged)
        savePositions();
    emit countChanged();
    emit selectionChanged();
}

// ── Posiciones (desktop-items-0.conf, el mismo archivo que pcmanfm-qt) ──────

void DesktopModel::loadPositions()
{
    m_customStore.clear();
    QSettings file(m_positionsFile, QSettings::IniFormat);
    const QStringList names = file.childGroups();
    for (const QString &name : names) {
        file.beginGroup(name);
        m_customStore.insert(name, file.value(QStringLiteral("pos")).toPoint());
        file.endGroup();
    }
}

void DesktopModel::savePositions()
{
    QSettings file(m_positionsFile, QSettings::IniFormat);
    file.clear();
    QStringList names = m_customStore.keys();
    std::sort(names.begin(), names.end());
    for (const QString &name : std::as_const(names)) {
        file.beginGroup(name);
        file.setValue(QStringLiteral("pos"), m_customStore.value(name));
        file.endGroup();
    }
}

void DesktopModel::setViewSize(int w, int h)
{
    if (QSize(w, h) == m_view || w <= 0 || h <= 0)
        return;
    m_view = QSize(w, h);
    relayout();
}

// FolderView::updateGridSize + DesktopWindow::removeBottomGap de libfm-qt/pcmanfm-qt.
void DesktopModel::computeGrid()
{
    const QFontMetrics fm(m_font);
    const int textW = fm.averageCharWidth() * 13;
    const int textH = fm.lineSpacing() * 3;
    const QSize base(std::max(m_iconSize, textW) + 4, m_iconSize + textH + 4);
    QSize m = m_baseMargins;
    const int wah = m_view.height() - m_workMargins.top() - m_workMargins.bottom();
    for (int i = 0; wah > 0 && i < 6; ++i) {           // pcmanfm-qt lo repite en cada recolocación
        const int cellH = base.height() + 2 * m.height();
        const int n = wah / cellH;
        if (n <= 0)
            break;
        const int gap = wah % cellH;
        const qreal exact = qreal(cellH - gap) / qreal(2 * n + 2);
        const int sub = int(exact) + (int(exact) == exact ? 0 : 1);
        QSize next = m;
        if (sub > 0 && m.height() - sub >= m_baseMargins.height())
            next -= QSize(0, sub);
        else
            next += QSize(0, (gap / n) / 2);
        if (next == m)
            break;
        m = next;
    }
    m_margins = m;
    m_grid = QSize(base.width() + 2 * m.width(), base.height() + 2 * m.height());
    if (qEnvironmentVariableIsSet("NEKOWALL_DESKTOP_DEBUG"))
        std::fprintf(stderr, "rejilla: vista=%dx%d letra=%s px=%d lineSpacing=%d avgChar=%d base=%dx%d margen=%dx%d celda=%dx%d\n",
                     m_view.width(), m_view.height(), qPrintable(m_font.toString()), QFontInfo(m_font).pixelSize(),
                     fm.lineSpacing(), fm.averageCharWidth(), base.width(), base.height(), m.width(), m.height(),
                     m_grid.width(), m_grid.height());
    m_workArea = QRect(QPoint(0, 0), m_view).marginsRemoved(m_workMargins);
}

void DesktopModel::alignToGrid(QPoint &p) const
{
    const int w = (p.x() - m_workArea.left()) / m_grid.width();
    const int h = (p.y() - m_workArea.top()) / m_grid.height();
    p = QPoint(m_workArea.left() + w * m_grid.width(), m_workArea.top() + h * m_grid.height());
}

QPoint DesktopModel::cellFor(qreal x, qreal y) const
{
    if (m_grid.isEmpty() || m_workArea.isEmpty())
        return {};
    const int cols = std::max(1, m_workArea.width() / m_grid.width());
    const int rows = std::max(1, m_workArea.height() / m_grid.height());
    const int c = std::clamp(int((x - m_workArea.left()) / m_grid.width()), 0, cols - 1);
    const int r = std::clamp(int((y - m_workArea.top()) / m_grid.height()), 0, rows - 1);
    return {m_workArea.left() + c * m_grid.width(), m_workArea.top() + r * m_grid.height()};
}

// DesktopWindow::retrieveCustomPos + relayoutItems de pcmanfm-qt.
void DesktopModel::relayout()
{
    if (m_view.isEmpty())
        return;
    computeGrid();
    const int gw = m_grid.width(), gh = m_grid.height();
    const QRect wa = m_workArea;

    QHash<QString, QPoint> custom;
    QList<QPoint> used;
    QStringList names = m_customStore.keys();
    std::sort(names.begin(), names.end());
    for (const QString &name : std::as_const(names)) {
        if (!QFileInfo::exists(m_folder + QLatin1Char('/') + name))
            continue;
        QPoint p = m_customStore.value(name);
        if (p.x() >= wa.left() && p.y() >= wa.top() && p.y() + gh <= wa.bottom() + 1) {
            alignToGrid(p);
            while (used.contains(p)) {
                p.setY(p.y() + gh);
                if (p.y() + gh > wa.bottom() + 1) {
                    p.setX(p.x() + gw);
                    p.setY(wa.top());
                }
            }
            if (p.x() + gw <= wa.right() + 1) {
                custom.insert(name, p);
                used << p;
            }
        }
    }

    QPoint pos = wa.topLeft();
    bool stickyChanged = false;
    int row = 0, guard = 0;
    while (row < m_items.size() && guard++ < 100000) {
        Item &it = m_items[row];
        if (custom.contains(it.fileName)) {
            it.pos = custom.value(it.fileName);
            ++row;
            continue;
        }
        bool busy = false;
        for (const QPoint &c : std::as_const(used))
            if (QRect(c, m_grid).contains(pos)) {
                busy = true;
                break;
            }
        if (!busy) {
            it.pos = pos;
            if (m_allSticky && pos.x() + gw <= wa.right() + 1) {
                m_customStore.insert(it.fileName, pos);
                stickyChanged = true;
            }
            ++row;
        }
        pos.setY(pos.y() + gh);
        if (pos.y() + gh > wa.bottom() + 1) {
            pos.setX(pos.x() + gw);
            pos.setY(wa.top());
        }
    }
    if (stickyChanged)
        savePositions();
    if (!m_items.isEmpty())
        emit dataChanged(index(0), index(int(m_items.size()) - 1), {XRole, YRole, StickyRole});
    emit geometryChanged();
}

QRect DesktopModel::itemRect(const Item &it) const
{
    return QRect(it.pos + QPoint(m_margins.width(), m_margins.height()),
                 m_grid - QSize(2 * m_margins.width(), 2 * m_margins.height()));
}

int DesktopModel::rowAt(qreal x, qreal y) const
{
    if (m_hideItems)
        return -1;
    for (int i = 0; i < m_items.size(); ++i)
        if (itemRect(m_items.at(i)).contains(QPoint(int(x), int(y))))
            return i;
    return -1;
}

// ── Selección ────────────────────────────────────────────────────────────────

int DesktopModel::selectedCount() const
{
    return int(std::count_if(m_items.cbegin(), m_items.cend(), [](const Item &i) { return i.selected; }));
}

QList<int> DesktopModel::selectedRows() const
{
    QList<int> rows;
    for (int i = 0; i < m_items.size(); ++i)
        if (m_items.at(i).selected)
            rows << i;
    return rows;
}

void DesktopModel::emitSelection()
{
    if (!m_items.isEmpty())
        emit dataChanged(index(0), index(int(m_items.size()) - 1), {SelectedRole});
    emit selectionChanged();
}

void DesktopModel::select(int row, int mode)
{
    if (row < 0 || row >= m_items.size())
        return;
    if (mode == 1) {
        m_items[row].selected = !m_items[row].selected;
        m_anchor = row;
    } else if (mode == 2 && m_anchor >= 0 && m_anchor < m_items.size()) {
        const int a = std::min(m_anchor, row), b = std::max(m_anchor, row);
        for (int i = 0; i < m_items.size(); ++i)
            m_items[i].selected = i >= a && i <= b;
    } else {
        for (Item &it : m_items)
            it.selected = false;
        m_items[row].selected = true;
        m_anchor = row;
    }
    m_current = row;
    emitSelection();
}

void DesktopModel::clearSelection()
{
    for (Item &it : m_items)
        it.selected = false;
    emitSelection();
}

void DesktopModel::selectAll()
{
    for (Item &it : m_items)
        it.selected = true;
    emitSelection();
}

void DesktopModel::invertSelection()
{
    for (Item &it : m_items)
        it.selected = !it.selected;
    emitSelection();
}

void DesktopModel::beginRubberBand(bool additive)
{
    m_rubberBase.clear();
    for (const Item &it : std::as_const(m_items))
        m_rubberBase << (additive && it.selected);
}

void DesktopModel::rubberBand(qreal x, qreal y, qreal w, qreal h)
{
    const QRect r = QRectF(x, y, w, h).normalized().toAlignedRect();
    bool changed = false;
    for (int i = 0; i < m_items.size(); ++i) {
        // La parte visible: el icono y su nombre (no la celda entera).
        const Item &it = m_items.at(i);
        const QRect vis(it.pos.x() + (m_grid.width() - m_iconSize) / 2, it.pos.y() + m_margins.height(),
                        m_iconSize, m_grid.height() - 2 * m_margins.height());
        const bool sel = (i < m_rubberBase.size() && m_rubberBase.at(i)) || r.intersects(vis);
        if (sel != it.selected) {
            m_items[i].selected = sel;
            changed = true;
        }
    }
    if (changed)
        emitSelection();
}

// Flechas como en pcmanfm-qt: al icono más cercano en esa dirección.
int DesktopModel::navigate(int key, bool extend)
{
    if (m_items.isEmpty() || m_hideItems)
        return -1;
    int target = -1;
    if (m_current < 0 || m_current >= m_items.size()) {
        // Sin actual: el primero visualmente (arriba a la izquierda).
        target = 0;
        for (int i = 1; i < m_items.size(); ++i) {
            const QPoint a = m_items.at(i).pos, b = m_items.at(target).pos;
            if (a.x() < b.x() || (a.x() == b.x() && a.y() < b.y()))
                target = i;
        }
    } else if (key == Qt::Key_Home || key == Qt::Key_End) {
        target = key == Qt::Key_Home ? 0 : int(m_items.size()) - 1;
    } else {
        const QPoint c = m_items.at(m_current).pos;
        int dx = 0, dy = 0;
        switch (key) {
        case Qt::Key_Left: dx = -1; break;
        case Qt::Key_Right: dx = 1; break;
        case Qt::Key_Up: case Qt::Key_PageUp: dy = -1; break;
        case Qt::Key_Down: case Qt::Key_PageDown: dy = 1; break;
        default: return m_current;
        }
        qint64 best = -1;
        for (int i = 0; i < m_items.size(); ++i) {
            const QPoint p = m_items.at(i).pos - c;
            const int along = dx ? p.x() * dx : p.y() * dy;
            const int across = dx ? std::abs(p.y()) : std::abs(p.x());
            if (along <= 0)
                continue;
            const qint64 score = qint64(along) + 4ll * across;
            if (best < 0 || score < best) {
                best = score;
                target = i;
            }
        }
        if (target < 0)
            return m_current;
        if (key == Qt::Key_PageUp || key == Qt::Key_PageDown) {   // hasta el extremo de la columna
            int t = target;
            while (true) {
                int next = -1;
                for (int i = 0; i < m_items.size(); ++i)
                    if (m_items.at(i).pos.x() == m_items.at(t).pos.x()
                        && (m_items.at(i).pos.y() - m_items.at(t).pos.y()) * dy > 0
                        && (next < 0 || std::abs(m_items.at(i).pos.y() - m_items.at(t).pos.y())
                                            < std::abs(m_items.at(next).pos.y() - m_items.at(t).pos.y())))
                        next = i;
                if (next < 0)
                    break;
                t = next;
            }
            target = t;
        }
    }
    if (extend) {
        m_items[target].selected = true;
        m_current = target;
        emitSelection();
    } else {
        select(target, 0);
    }
    return target;
}

// ── Abrir ────────────────────────────────────────────────────────────────────

void DesktopModel::launch(const QString &exec, const QString &file, bool terminal, const QString &what)
{
    QStringList args = QProcess::splitCommand(exec);
    QStringList out;
    for (QString a : std::as_const(args)) {
        if (a == QStringLiteral("%f") || a == QStringLiteral("%F") || a == QStringLiteral("%u") || a == QStringLiteral("%U")) {
            if (!file.isEmpty())
                out << file;
            continue;
        }
        a.replace(QRegularExpression(QStringLiteral("%[fFuU]")), file);
        a.remove(QRegularExpression(QStringLiteral("%[dDnNickvm]")));
        a.replace(QStringLiteral("%%"), QStringLiteral("%"));
        if (!a.isEmpty())
            out << a;
    }
    if (out.isEmpty()) {
        emit message(QStringLiteral("No se pudo abrir «%1»").arg(what));
        return;
    }
    if (terminal) {
        const QStringList term = terminalCommand();
        if (!term.isEmpty()) {
            const QString t = QFileInfo(term.first()).fileName();
            QStringList pre = term;
            if (t != QStringLiteral("foot") && t != QStringLiteral("kitty"))
                pre << (t == QStringLiteral("gnome-terminal") ? QStringLiteral("--") : QStringLiteral("-e"));
            out = pre + out;
        }
    }
    const QString prog = out.takeFirst();
    if (QProcess::startDetached(prog, out, QDir::homePath()))
        emit message(QStringLiteral("Abriendo «%1»…").arg(what));
    else
        emit message(QStringLiteral("No se pudo abrir «%1»").arg(what));
}

void DesktopModel::open(int row)
{
    if (row < 0 || row >= m_items.size())
        return;
    const Item &it = m_items.at(row);
    if (it.launcher) {
        if (!it.url.isEmpty()) {
            QProcess::startDetached(QStringLiteral("xdg-open"), {it.url});
            emit message(QStringLiteral("Abriendo «%1»…").arg(it.name));
        } else {
            launch(it.exec, {}, it.terminal, it.name);
        }
        return;
    }
    // Carpetas: con pcmanfm-qt, salvo «Abrir con el gestor de archivos predeterminado» (como pcmanfm-qt).
    bool ok;
    if (it.dir && !m_defaultFm && hasProgram(QStringLiteral("pcmanfm-qt")))
        ok = QProcess::startDetached(QStringLiteral("pcmanfm-qt"), {it.path});
    else
        ok = QProcess::startDetached(QStringLiteral("xdg-open"), {it.path})
             || QDesktopServices::openUrl(QUrl::fromLocalFile(it.path));
    emit message(ok ? QStringLiteral("Abriendo «%1»…").arg(it.name)
                    : QStringLiteral("No hay ningún programa para abrir «%1»").arg(it.name));
}

void DesktopModel::openSelected()
{
    const QList<int> rows = selectedRows();
    for (int r : rows)
        open(r);
}

QVariantList DesktopModel::appsFor(int row)
{
    QVariantList list;
    if (row < 0 || row >= m_items.size() || m_items.at(row).launcher)
        return list;
    QMimeDatabase mimes;
    const QMimeType m = mimes.mimeTypeForName(m_items.at(row).mime);
    QStringList types{m.name()};
    types << m.allAncestors();
    const QString def = defaultAppFor(m.name());
    QList<App> found;
    for (const App &a : installedApps())
        for (const QString &t : std::as_const(types))
            if (a.mimes.contains(t)) {
                found << a;
                break;
            }
    QCollator coll;
    std::sort(found.begin(), found.end(), [&](const App &a, const App &b) {
        if ((a.id == def) != (b.id == def))
            return a.id == def;
        return coll.compare(a.name, b.name) < 0;
    });
    for (const App &a : std::as_const(found).mid(0, 14))
        list << QVariantMap{{QStringLiteral("id"), a.id}, {QStringLiteral("name"), a.name},
                            {QStringLiteral("icon"), a.icon}, {QStringLiteral("isDefault"), a.id == def}};
    return list;
}

void DesktopModel::openWith(int row, const QString &appId)
{
    if (row < 0 || row >= m_items.size())
        return;
    for (const App &a : installedApps())
        if (a.id == appId) {
            launch(a.exec, m_items.at(row).path, a.terminal, m_items.at(row).name);
            return;
        }
}

void DesktopModel::reveal(int row)
{
    if (row >= 0 && row < m_items.size())
        QProcess::startDetached(QStringLiteral("xdg-open"), {QFileInfo(m_items.at(row).path).absolutePath()});
}

void DesktopModel::openFolder()
{
    if (!m_defaultFm && hasProgram(QStringLiteral("pcmanfm-qt")))
        QProcess::startDetached(QStringLiteral("pcmanfm-qt"), {m_folder});
    else
        QProcess::startDetached(QStringLiteral("xdg-open"), {m_folder});
}

void DesktopModel::openTerminal()
{
    const QStringList term = terminalCommand();
    if (term.isEmpty()) {
        emit message(QStringLiteral("No se encontró ninguna terminal"));
        return;
    }
    QProcess::startDetached(term.first(), term.mid(1), m_folder);
}

void DesktopModel::openNekoWall()
{
    QProcess::startDetached(QCoreApplication::applicationFilePath(), {});
}

// ── Operaciones con archivos ─────────────────────────────────────────────────

QString DesktopModel::freeName(const QString &dir, const QString &name) const
{
    if (!QFileInfo::exists(dir + QLatin1Char('/') + name))
        return name;
    const QFileInfo fi(dir + QLatin1Char('/') + name);
    const bool isDir = fi.isDir();
    const QString base = isDir ? name : fi.completeBaseName();
    const QString ext = isDir || fi.suffix().isEmpty() ? QString() : QLatin1Char('.') + fi.suffix();
    for (int n = 2;; ++n) {
        const QString candidate = QStringLiteral("%1 (%2)%3").arg(base).arg(n).arg(ext);
        if (!QFileInfo::exists(dir + QLatin1Char('/') + candidate))
            return candidate;
    }
}

void DesktopModel::transfer(const QStringList &sources, const QString &destDir, bool move, const QPoint &dropAt)
{
    QPoint cell = dropAt;
    QSet<QPoint> taken;
    for (auto c = m_customStore.cbegin(); c != m_customStore.cend(); ++c)
        taken.insert(c.value());
    int count = 0;
    for (const QString &src : sources) {
        const QFileInfo fi(src);
        if (!fi.exists() && !fi.isSymLink())
            continue;
        if (QDir(fi.absolutePath()) == QDir(destDir) && move)
            continue;                                 // ya está ahí
        if (fi.isDir() && (destDir + QLatin1Char('/')).startsWith(fi.absoluteFilePath() + QLatin1Char('/')))
            continue;                                 // una carpeta dentro de sí misma
        const QString name = freeName(destDir, fi.fileName());
        const QString dest = destDir + QLatin1Char('/') + name;
        if (!dropAt.isNull() && QDir(destDir) == QDir(m_folder)) {
            int guard = 0;
            while (taken.contains(cell) && guard++ < 500) {
                cell.setY(cell.y() + m_grid.height());
                if (cell.y() + m_grid.height() > m_workArea.bottom() + 1)
                    cell = QPoint(cell.x() + m_grid.width(), m_workArea.top());
            }
            m_pending.insert(name, cell);
            taken.insert(cell);
        }
        auto *p = new QProcess(this);
        connect(p, &QProcess::finished, this, [this, p, name](int code) {
            if (code != 0)
                emit message(QStringLiteral("No se pudo copiar/mover «%1»").arg(name));
            p->deleteLater();
            m_reloadTimer.start();
        });
        if (move)
            p->start(QStringLiteral("mv"), {QStringLiteral("--"), src, dest});
        else
            p->start(QStringLiteral("cp"), {QStringLiteral("-a"), QStringLiteral("--"), src, dest});
        ++count;
    }
    if (count > 0)
        emit message(move ? (count == 1 ? QStringLiteral("Moviendo 1 elemento…") : QStringLiteral("Moviendo %1 elementos…").arg(count))
                          : (count == 1 ? QStringLiteral("Copiando 1 elemento…") : QStringLiteral("Copiando %1 elementos…").arg(count)));
}

void DesktopModel::trashSelected()
{
    if (!m_useTrash) {
        deleteSelected();
        return;
    }
    int ok = 0, fail = 0;
    for (int r : selectedRows()) {
        if (QFile::moveToTrash(m_items.at(r).path)) {
            m_customStore.remove(m_items.at(r).fileName);
            ++ok;
        } else {
            ++fail;
        }
    }
    if (fail)
        emit message(QStringLiteral("No se pudo mover %1 elemento(s) a la papelera").arg(fail));
    else if (ok)
        emit message(ok == 1 ? QStringLiteral("Movido a la papelera") : QStringLiteral("%1 elementos movidos a la papelera").arg(ok));
    savePositions();
    m_reloadTimer.start();
}

void DesktopModel::deleteSelected()
{
    int fail = 0;
    for (int r : selectedRows()) {
        const Item &it = m_items.at(r);
        const QFileInfo fi(it.path);
        const bool ok = fi.isDir() && !fi.isSymLink() ? QDir(it.path).removeRecursively() : QFile::remove(it.path);
        if (ok)
            m_customStore.remove(it.fileName);
        else
            ++fail;
    }
    if (fail)
        emit message(QStringLiteral("No se pudo eliminar %1 elemento(s)").arg(fail));
    savePositions();
    m_reloadTimer.start();
}

QString DesktopModel::renameItem(int row, const QString &newNameIn)
{
    if (row < 0 || row >= m_items.size())
        return {};                           // el icono desapareció mientras se renombraba
    Item &it = m_items[row];
    const QString newName = newNameIn.trimmed();
    if (newName.isEmpty() || newName == it.name)
        return {};
    if (it.launcher) {
        // Lanzador: se cambia su nombre visible (Name=), como libfm-qt.
        QFile f(it.path);
        if (!f.open(QIODevice::ReadOnly | QIODevice::Text))
            return QStringLiteral("No se pudo leer el lanzador");
        QStringList lines = QString::fromUtf8(f.readAll()).split(QLatin1Char('\n'));
        f.close();
        const QString lang = QLocale().name(), shortLang = lang.section(QLatin1Char('_'), 0, 0);
        bool inEntry = false, done = false;
        for (QString &l : lines) {
            const QString t = l.trimmed();
            if (t.startsWith(QLatin1Char('[')))
                inEntry = t == QStringLiteral("[Desktop Entry]");
            else if (inEntry && (t.startsWith(QStringLiteral("Name=")) || t.startsWith(QStringLiteral("Name[%1]=").arg(lang))
                                 || t.startsWith(QStringLiteral("Name[%1]=").arg(shortLang)))) {
                l = l.section(QLatin1Char('='), 0, 0) + QLatin1Char('=') + newName;
                done = true;
            }
        }
        if (!done)
            return QStringLiteral("El lanzador no tiene nombre");
        if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text))
            return QStringLiteral("No se pudo guardar el lanzador");
        f.write(lines.join(QLatin1Char('\n')).toUtf8());
        f.close();
        m_reloadTimer.start();
        return {};
    }
    if (newName.contains(QLatin1Char('/')) || newName == QStringLiteral(".") || newName == QStringLiteral(".."))
        return QStringLiteral("El nombre no puede contener «/»");
    const QString dest = m_folder + QLatin1Char('/') + newName;
    if (QFileInfo::exists(dest))
        return QStringLiteral("Ya existe «%1»").arg(newName);
    if (!QDir(m_folder).rename(it.fileName, newName))
        return QStringLiteral("No se pudo renombrar");
    if (m_customStore.contains(it.fileName)) {
        m_customStore.insert(newName, m_customStore.take(it.fileName));
        savePositions();
    }
    it.fileName = newName;                 // conserva la selección tras recargar
    reload();
    return {};
}

int DesktopModel::createFolder()
{
    const QString name = freeName(m_folder, QStringLiteral("Carpeta nueva"));
    if (!QDir(m_folder).mkdir(name)) {
        emit message(QStringLiteral("No se pudo crear la carpeta"));
        return -1;
    }
    reload();
    for (int i = 0; i < m_items.size(); ++i)
        if (m_items.at(i).fileName == name) {
            select(i, 0);
            return i;
        }
    return -1;
}

int DesktopModel::createFile()
{
    const QString name = freeName(m_folder, QStringLiteral("Archivo nuevo"));
    QFile f(m_folder + QLatin1Char('/') + name);
    if (!f.open(QIODevice::WriteOnly)) {
        emit message(QStringLiteral("No se pudo crear el archivo"));
        return -1;
    }
    f.close();
    reload();
    for (int i = 0; i < m_items.size(); ++i)
        if (m_items.at(i).fileName == name) {
            select(i, 0);
            return i;
        }
    return -1;
}

bool DesktopModel::createLauncher(const QString &name, const QString &command, const QString &icon, bool terminal)
{
    if (name.trimmed().isEmpty() || command.trimmed().isEmpty())
        return false;
    QString file = name.trimmed();
    file.replace(QRegularExpression(QStringLiteral("[/\\\\]")), QStringLiteral("-"));
    file = freeName(m_folder, file + QStringLiteral(".desktop"));
    QFile f(m_folder + QLatin1Char('/') + file);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Text))
        return false;
    f.write(QStringLiteral("[Desktop Entry]\nType=Application\nName=%1\nExec=%2\nIcon=%3\nTerminal=%4\n")
                .arg(name.trimmed(), command.trimmed(), icon.trimmed().isEmpty() ? QStringLiteral("application-x-executable") : icon.trimmed(),
                     terminal ? QStringLiteral("true") : QStringLiteral("false")).toUtf8());
    f.close();
    f.setPermissions(f.permissions() | QFileDevice::ExeOwner | QFileDevice::ExeUser);
    m_reloadTimer.start();
    return true;
}

void DesktopModel::copySelected(bool cut)
{
    QList<QUrl> urls;
    for (int i = 0; i < m_items.size(); ++i) {
        m_items[i].cut = cut && m_items.at(i).selected;
        if (m_items.at(i).selected)
            urls << QUrl::fromLocalFile(m_items.at(i).path);
    }
    if (!m_items.isEmpty())
        emit dataChanged(index(0), index(int(m_items.size()) - 1), {CutRole});
    if (urls.isEmpty())
        return;
    auto *mime = new QMimeData;
    mime->setUrls(urls);
    QByteArray gnome = cut ? "cut" : "copy";
    for (const QUrl &u : std::as_const(urls))
        gnome += '\n' + u.toEncoded();
    mime->setData(QStringLiteral("x-special/gnome-copied-files"), gnome);
    mime->setData(QStringLiteral("application/x-kde-cutselection"), cut ? "1" : "0");
    QGuiApplication::clipboard()->setMimeData(mime);
}

void DesktopModel::copyPaths()
{
    QStringList paths;
    for (int r : selectedRows())
        paths << m_items.at(r).path;
    QGuiApplication::clipboard()->setText(paths.join(QLatin1Char('\n')));
}

bool DesktopModel::canPaste() const
{
    const QMimeData *m = QGuiApplication::clipboard()->mimeData();
    if (!m)
        return false;
    for (const QUrl &u : m->urls())
        if (u.isLocalFile())
            return true;
    return false;
}

void DesktopModel::paste(int intoRow)
{
    const QMimeData *m = QGuiApplication::clipboard()->mimeData();
    if (!m)
        return;
    bool cut = m->data(QStringLiteral("application/x-kde-cutselection")) == "1";
    const QByteArray g = m->data(QStringLiteral("x-special/gnome-copied-files"));
    if (!g.isEmpty())
        cut = g.startsWith("cut");
    QStringList sources;
    for (const QUrl &u : m->urls())
        if (u.isLocalFile())
            sources << u.toLocalFile();
    QString dest = m_folder;
    if (intoRow >= 0 && intoRow < m_items.size() && m_items.at(intoRow).dir)
        dest = m_items.at(intoRow).path;
    transfer(sources, dest, cut, {});
    if (cut) {
        QGuiApplication::clipboard()->clear();
        for (Item &it : m_items)
            it.cut = false;
    }
}

void DesktopModel::dropUrls(const QStringList &uris, qreal x, qreal y, bool copy)
{
    QStringList sources;
    for (const QString &s : uris) {
        const QUrl u(s.trimmed());
        if (u.isLocalFile())
            sources << u.toLocalFile();
    }
    if (sources.isEmpty())
        return;
    const int r = rowAt(x, y);
    if (r >= 0 && m_items.at(r).fileName == QStringLiteral("trash-can.desktop")) {
        for (const QString &s : std::as_const(sources))
            QFile::moveToTrash(s);
        return;
    }
    const QString dest = r >= 0 && m_items.at(r).dir ? m_items.at(r).path : m_folder;
    // Como libfm: mover si es el mismo disco, copiar si no (Ctrl = copiar).
    const bool sameFs = QStorageInfo(sources.first()).rootPath() == QStorageInfo(dest).rootPath();
    transfer(sources, dest, !copy && sameFs, dest == m_folder ? cellFor(x, y) : QPoint());
}

// Arrastre dentro del escritorio: los iconos quedan fijos donde se sueltan (stickToPosition).
void DesktopModel::moveSelection(qreal dx, qreal dy)
{
    QList<int> rows = selectedRows();
    if (rows.isEmpty())
        return;
    std::sort(rows.begin(), rows.end(), [this](int a, int b) {
        const QPoint pa = m_items.at(a).pos, pb = m_items.at(b).pos;
        return pa.x() != pb.x() ? pa.x() < pb.x() : pa.y() < pb.y();
    });
    QSet<QPoint> taken;
    for (int i = 0; i < m_items.size(); ++i)
        if (!m_items.at(i).selected && m_customStore.contains(m_items.at(i).fileName))
            taken.insert(m_items.at(i).pos);
    for (int r : std::as_const(rows)) {
        const Item &it = m_items.at(r);
        QPoint p = cellFor(it.pos.x() + dx + m_grid.width() / 2.0, it.pos.y() + dy + m_grid.height() / 2.0);
        int guard = 0;
        while (taken.contains(p) && guard++ < 1000) {
            p.setY(p.y() + m_grid.height());
            if (p.y() + m_grid.height() > m_workArea.bottom() + 1)
                p = QPoint(p.x() + m_grid.width(), m_workArea.top());
            if (p.x() + m_grid.width() > m_workArea.right() + 1) {
                p = it.pos;                           // no hay sitio: se queda donde estaba
                break;
            }
        }
        taken.insert(p);
        m_customStore.insert(it.fileName, p);
    }
    savePositions();
    relayout();
}

void DesktopModel::dropSelectionOn(int row)
{
    if (row < 0 || row >= m_items.size() || m_items.at(row).selected)
        return;
    const Item &target = m_items.at(row);
    if (target.fileName == QStringLiteral("trash-can.desktop")) {
        trashSelected();
        return;
    }
    if (!target.dir)
        return;
    QStringList sources;
    for (int r : selectedRows())
        sources << m_items.at(r).path;
    transfer(sources, target.path, true, {});
}

bool DesktopModel::isDropTarget(int row) const
{
    if (row < 0 || row >= m_items.size() || m_items.at(row).selected)
        return false;
    return m_items.at(row).dir || m_items.at(row).fileName == QStringLiteral("trash-can.desktop");
}

bool DesktopModel::isSticky(int row) const
{
    return row >= 0 && row < m_items.size() && m_customStore.contains(m_items.at(row).fileName);
}

void DesktopModel::setStickySelected(bool on)
{
    for (int r : selectedRows()) {
        if (on)
            m_customStore.insert(m_items.at(r).fileName, m_items.at(r).pos);
        else
            m_customStore.remove(m_items.at(r).fileName);
    }
    savePositions();
    relayout();
}

QVariantMap DesktopModel::properties(int row) const
{
    if (row < 0 || row >= m_items.size())
        return {};
    const Item &it = m_items.at(row);
    const QFileInfo fi(it.path);
    QMimeDatabase mimes;
    const QLocale loc;
    QString size;
    if (it.dir) {
        const int n = int(QDir(it.path).entryList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden).size());
        size = n == 1 ? QStringLiteral("1 elemento") : QStringLiteral("%1 elementos").arg(n);
    } else {
        size = loc.formattedDataSize(fi.size()) + QStringLiteral(" (%1 bytes)").arg(loc.toString(fi.size()));
    }
    QString perms;
    const auto p = fi.permissions();
    perms += p & QFileDevice::ReadOwner ? QLatin1Char('r') : QLatin1Char('-');
    perms += p & QFileDevice::WriteOwner ? QLatin1Char('w') : QLatin1Char('-');
    perms += p & QFileDevice::ExeOwner ? QLatin1Char('x') : QLatin1Char('-');
    perms += p & QFileDevice::ReadGroup ? QLatin1Char('r') : QLatin1Char('-');
    perms += p & QFileDevice::WriteGroup ? QLatin1Char('w') : QLatin1Char('-');
    perms += p & QFileDevice::ExeGroup ? QLatin1Char('x') : QLatin1Char('-');
    perms += p & QFileDevice::ReadOther ? QLatin1Char('r') : QLatin1Char('-');
    perms += p & QFileDevice::WriteOther ? QLatin1Char('w') : QLatin1Char('-');
    perms += p & QFileDevice::ExeOther ? QLatin1Char('x') : QLatin1Char('-');
    return {{QStringLiteral("name"), it.name},
            {QStringLiteral("fileName"), it.fileName},
            {QStringLiteral("icon"), it.icon},
            {QStringLiteral("type"), mimes.mimeTypeForName(it.mime).comment()},
            {QStringLiteral("mime"), it.mime},
            {QStringLiteral("size"), size},
            {QStringLiteral("location"), fi.absolutePath()},
            {QStringLiteral("target"), fi.isSymLink() ? fi.symLinkTarget() : (it.launcher ? it.exec : QString())},
            {QStringLiteral("modified"), loc.toString(fi.lastModified(), QLocale::LongFormat)},
            {QStringLiteral("accessed"), loc.toString(fi.lastRead(), QLocale::LongFormat)},
            {QStringLiteral("owner"), fi.owner()},
            {QStringLiteral("permissions"), perms}};
}
