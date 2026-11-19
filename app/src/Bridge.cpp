#include "Bridge.h"

#include <QCoreApplication>
#include <QDesktopServices>
#include <QGuiApplication>
#include <QScreen>
#include <QCursor>
#include <QDir>
#ifdef HAVE_KWINDOWEFFECTS
#include <KWindowEffects>
#endif
#include <QImageReader>
#include <QPainter>
#include <QLinearGradient>
#include <QDateTime>
#include <QFile>
#include <QUrl>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QSettings>
#include <QStandardPaths>

Bridge::Bridge(QObject *parent)
    : QObject(parent), m_cli(findCli()), m_reducedMotion(readReducedMotion())
{
    if (QScreen *sc = QGuiApplication::primaryScreen()) {
        const QSize px = sc->size() * sc->devicePixelRatio();
        m_screenW = px.width();
        m_screenH = px.height();
    }
    m_wayland = QGuiApplication::platformName().startsWith(QStringLiteral("wayland"));
#ifdef HAVE_KWINDOWEFFECTS
    m_glass = KWindowEffects::isEffectAvailable(KWindowEffects::BlurBehind);
#endif
    if (qEnvironmentVariableIsSet("NEKOWALL_GLASS"))   // forzar para pruebas: 0/1
        m_glass = qEnvironmentVariableIntValue("NEKOWALL_GLASS") != 0;

    QSettings st;
    m_largePolicy = st.value(QStringLiteral("video/largePolicy"), QStringLiteral("ask")).toString();
    const QString dataDir = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation) + QStringLiteral("/nekowall");
    m_mascotPath = st.value(QStringLiteral("hud/mascot"), dataDir + QStringLiteral("/mascota.jpg")).toString();
    m_showMascot = st.value(QStringLiteral("hud/showMascot"), true).toBool();
    // Punto focal de la ilustración por defecto (ojo brillante: 63 % ancho, 47 % alto).
    m_mascotFocusX = st.value(QStringLiteral("hud/mascotFocusX"), 0.63).toDouble();
    m_mascotFocusY = st.value(QStringLiteral("hud/mascotFocusY"), 0.47).toDouble();

    // Copiar fondos a una carpeta añadida los muestra solos: se espera 1,5 s sin cambios (copias
    // de muchos archivos) y se revisa.
    m_watcher = new QFileSystemWatcher(this);
    m_syncTimer = new QTimer(this);
    m_syncTimer->setSingleShot(true);
    m_syncTimer->setInterval(1500);
    connect(m_watcher, &QFileSystemWatcher::directoryChanged, m_syncTimer, qOverload<>(&QTimer::start));
    connect(m_syncTimer, &QTimer::timeout, this, &Bridge::syncFolders);

    // Cualquier cambio en la biblioteca, venga de donde venga, se ve aquí medio segundo después.
    // Se vigila el archivo y su carpeta (SQLite crea y borra el «-journal» en cada escritura, y así
    // también se nota si el archivo se reemplaza).
    m_dbPath = qEnvironmentVariable("NEKOWALL_DB");
    if (m_dbPath.isEmpty())
        m_dbPath = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation)
            + QStringLiteral("/nekowall/library.db");
    m_dbWatcher = new QFileSystemWatcher(this);
    m_dbTimer = new QTimer(this);
    m_dbTimer->setSingleShot(true);
    m_dbTimer->setInterval(500);
    connect(m_dbWatcher, &QFileSystemWatcher::fileChanged, m_dbTimer, qOverload<>(&QTimer::start));
    connect(m_dbWatcher, &QFileSystemWatcher::directoryChanged, m_dbTimer, qOverload<>(&QTimer::start));
    connect(m_dbTimer, &QTimer::timeout, this, [this] {
        watchDb();
        loadList();
    });
    watchDb();

    auto screensMoved = [this] { emit screensChanged(); emit targetChanged(); emit statusChanged(); };
    connect(qGuiApp, &QGuiApplication::screenAdded, this, screensMoved);
    connect(qGuiApp, &QGuiApplication::screenRemoved, this, screensMoved);
}

// ── Varias pantallas ───────────────────────────────────────────────────────

QVariantList Bridge::screens() const
{
    QVariantList out;
    const auto list = QGuiApplication::screens();
    for (int i = 0; i < list.size(); ++i) {
        const QScreen *s = list[i];
        QVariantMap m;
        m[QStringLiteral("name")] = s->name();
        m[QStringLiteral("label")] = QStringLiteral("Pantalla %1 · %2").arg(i + 1).arg(s->name());
        m[QStringLiteral("primary")] = s == QGuiApplication::primaryScreen();
        out << m;
    }
    return out;
}

bool Bridge::multiScreen() const
{
    return m_perMonitor && QGuiApplication::screens().size() > 1;
}

QString Bridge::chosenScreen() const
{
    if (!m_perMonitor || m_target == QStringLiteral("*"))
        return QString();
    QStringList names;
    for (const QScreen *s : QGuiApplication::screens())
        names << s->name();
    if (!m_target.isEmpty() && names.contains(m_target))
        return m_target;
    if (names.contains(m_hostScreen))
        return m_hostScreen;
    return QGuiApplication::primaryScreen() ? QGuiApplication::primaryScreen()->name() : QString();
}

QString Bridge::resolvedScreen() const
{
    // Con una sola pantalla se aplica «a todas» (el camino de siempre): el resultado es el mismo.
    return multiScreen() ? chosenScreen() : QString();
}

QString Bridge::targetLabel() const
{
    if (!m_perMonitor)
        return QString();
    const QString r = chosenScreen();
    if (r.isEmpty())
        return QStringLiteral("Todas las pantallas");
    for (const QVariant &v : screens()) {
        const QVariantMap m = v.toMap();
        if (m.value(QStringLiteral("name")).toString() == r)
            return m.value(QStringLiteral("label")).toString();
    }
    return r;
}

void Bridge::setTarget(const QString &t)
{
    if (t == m_target)
        return;
    m_target = t;
    emit targetChanged();
    emit statusChanged();   // currentId depende del destino
}

void Bridge::setHostScreen(const QString &s)
{
    if (s == m_hostScreen)
        return;
    m_hostScreen = s;
    emit targetChanged();
    emit statusChanged();
}

void Bridge::cycleTarget()
{
    if (!m_perMonitor)
        return;
    QStringList order;
    for (const QScreen *s : QGuiApplication::screens())
        order << s->name();
    order << QStringLiteral("*");
    const QString now = chosenScreen().isEmpty() ? QStringLiteral("*") : chosenScreen();
    setTarget(order.value((order.indexOf(now) + 1) % order.size()));
}

int Bridge::cursorScreenIndex() const
{
    QScreen *s = QGuiApplication::screenAt(QCursor::pos());
    return s ? int(QGuiApplication::screens().indexOf(s)) : -1;
}

QString Bridge::currentId() const
{
    const QString r = resolvedScreen();
    if (!r.isEmpty() && m_currentByScreen.contains(r))
        return m_currentByScreen.value(r);
    return m_currentId;
}

QString Bridge::mascotUrl() const
{
    if (!m_showMascot || !QFileInfo::exists(m_mascotPath))
        return QString();
    const QString baked = bakedMascot();
    return baked.isEmpty() ? QString() : QUrl::fromLocalFile(baked).toString();
}

// Misma geometría que dibujaba el HUD: panel = 62 % del ancho a la derecha; ilustración a 1,3× la altura
// de la pantalla con su punto focal en (78 %, 22 %) del panel; fundido alfa hacia la izquierda × 0,8.
QString Bridge::bakedMascot() const
{
    QScreen *sc = QGuiApplication::primaryScreen();
    if (!sc)
        return QString();
    const QSize scr = sc->size() * sc->devicePixelRatio();
    const QFileInfo src(m_mascotPath);
    const QString key = QStringLiteral("%1|%2|%3|%4|%5x%6").arg(src.canonicalFilePath())
                            .arg(src.lastModified().toSecsSinceEpoch()).arg(m_mascotFocusX).arg(m_mascotFocusY)
                            .arg(scr.width()).arg(scr.height());
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation) + QStringLiteral("/nekowall");
    const QString out = dir + QStringLiteral("/hud-art-%1.png")
                            .arg(QString::number(qHash(key), 16));
    if (QFileInfo::exists(out))
        return out;

    QImageReader reader(src.filePath());
    reader.setAutoTransform(true);
    const QSize orig = reader.size();
    if (!orig.isValid())
        return QString();
    const int panelW = qRound(scr.width() * 0.62), H = scr.height();
    const double h = H * 1.3, w = h * orig.width() / orig.height();
    reader.setScaledSize(QSize(qRound(w), qRound(h)));   // decodifica ya al tamaño final
    const QImage img = reader.read();
    if (img.isNull())
        return QString();

    QImage art(panelW, H, QImage::Format_ARGB32_Premultiplied);
    art.fill(Qt::transparent);
    QPainter p(&art);
    p.setRenderHint(QPainter::SmoothPixmapTransform);
    p.drawImage(QPointF(panelW * 0.78 - w * m_mascotFocusX, H * 0.22 - h * m_mascotFocusY), img);
    QLinearGradient fade(0, 0, panelW, 0);
    fade.setColorAt(0.0, QColor(0, 0, 0, 0));
    fade.setColorAt(0.35, QColor(0, 0, 0, qRound(255 * 0.8 / 3)));
    fade.setColorAt(0.7, QColor(0, 0, 0, qRound(255 * 0.8)));
    fade.setColorAt(1.0, QColor(0, 0, 0, qRound(255 * 0.8)));
    p.setCompositionMode(QPainter::CompositionMode_DestinationIn);
    p.fillRect(art.rect(), fade);
    p.end();

    QDir().mkpath(dir);
    // Quitar versiones anteriores (otra imagen, otra resolución).
    for (const QFileInfo &old : QDir(dir).entryInfoList({QStringLiteral("hud-art-*.png")}, QDir::Files))
        QFile::remove(old.filePath());
    return art.save(out, "PNG") ? out : QString();
}

void Bridge::setShowMascot(bool on)
{
    if (on == m_showMascot)
        return;
    m_showMascot = on;
    QSettings().setValue(QStringLiteral("hud/showMascot"), on);
    emit mascotChanged();
}

void Bridge::setMascotImage(const QVariant &url)
{
    const QStringList l = toLocalPaths({url});
    if (l.isEmpty())
        return;
    const QString dataDir = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation) + QStringLiteral("/nekowall");
    QDir().mkpath(dataDir);
    const QString dest = dataDir + QStringLiteral("/mascota.") + QFileInfo(l.first()).suffix().toLower();
    if (QFileInfo(l.first()).canonicalFilePath() != QFileInfo(dest).canonicalFilePath()) {
        QFile::remove(dest);
        if (!QFile::copy(l.first(), dest)) {
            emit notice(QStringLiteral("No se pudo copiar la imagen"), true);
            return;
        }
    }
    m_mascotPath = dest;
    m_mascotFocusX = m_mascotFocusY = 0.5;
    QSettings st;
    st.setValue(QStringLiteral("hud/mascot"), dest);
    st.setValue(QStringLiteral("hud/mascotFocusX"), 0.5);
    st.setValue(QStringLiteral("hud/mascotFocusY"), 0.5);
    m_showMascot = true;
    st.setValue(QStringLiteral("hud/showMascot"), true);
    emit mascotChanged();
    emit notice(QStringLiteral("Ilustración del selector actualizada"), false);
}

void Bridge::setLargeVideoPolicy(const QString &p)
{
    if (p == m_largePolicy)
        return;
    m_largePolicy = p;
    QSettings().setValue(QStringLiteral("video/largePolicy"), p);
    emit largeVideoPolicyChanged();
}

// Orden: $NEKOWALL_CLI → junto al ejecutable → ../target/release (desarrollo) → PATH.
QString Bridge::findCli()
{
    const QString env = qEnvironmentVariable("NEKOWALL_CLI");
    if (!env.isEmpty())
        return env;
    const QString sibling = QCoreApplication::applicationDirPath() + QStringLiteral("/nekowall");
    if (QFileInfo(sibling).isExecutable())
        return sibling;
#ifdef NEKOWALL_DEV_CLI
    if (QFileInfo(QStringLiteral(NEKOWALL_DEV_CLI)).isExecutable())
        return QStringLiteral(NEKOWALL_DEV_CLI);
#endif
    const QString inPath = QStandardPaths::findExecutable(QStringLiteral("nekowall"));
    return inPath.isEmpty() ? QStringLiteral("nekowall") : inPath;
}

// KDE guarda «Velocidad de animación» en kdeglobals; 0 = animaciones desactivadas.
bool Bridge::readReducedMotion()
{
    if (qEnvironmentVariableIsSet("NEKOWALL_REDUCED_MOTION"))
        return qEnvironmentVariableIntValue("NEKOWALL_REDUCED_MOTION") != 0;
    const QString path = QStandardPaths::locate(QStandardPaths::GenericConfigLocation,
                                                QStringLiteral("kdeglobals"));
    if (path.isEmpty())
        return false;
    QSettings kde(path, QSettings::IniFormat);
    return kde.value(QStringLiteral("KDE/AnimationDurationFactor"), 1.0).toDouble() <= 0.0;
}

void Bridge::run(const QStringList &args, Done done)
{
    auto *p = new QProcess(this);
    connect(p, &QProcess::finished, this, [p, done](int code, QProcess::ExitStatus st) {
        done(st == QProcess::NormalExit ? code : -1, p->readAllStandardOutput(),
             p->readAllStandardError());
        p->deleteLater();
    });
    connect(p, &QProcess::errorOccurred, this, [this, p, done](QProcess::ProcessError e) {
        if (e != QProcess::FailedToStart)
            return;
        done(-1, {}, QStringLiteral("no se encontró la CLI «%1»").arg(m_cli).toUtf8());
        p->deleteLater();
    });
    p->start(m_cli, args);
}

void Bridge::setBusy(bool b)
{
    if (m_busy == b)
        return;
    m_busy = b;
    emit busyChanged();
}

static QString cleanError(const QByteArray &err)
{
    QString s = QString::fromUtf8(err).trimmed();
    if (s.startsWith(QStringLiteral("error: ")))
        s = s.mid(7);
    return s.isEmpty() ? QStringLiteral("error desconocido") : s;
}

void Bridge::refresh()
{
    loadList();
    loadStatus();
    loadVideoSettings();
    syncFolders();   // la lista se muestra ya; si hay fondos nuevos en las carpetas, se recarga
}

void Bridge::syncFolders()
{
    if (m_syncRunning) {
        m_syncAgain = true;
        return;
    }
    m_syncRunning = true;
    run({QStringLiteral("sync"), QStringLiteral("--json")},
        [this](int code, const QByteArray &out, const QByteArray &) {
            m_syncRunning = false;
            if (code == 0) {
                const QJsonObject o = QJsonDocument::fromJson(out).object();
                const int added = o.value(QStringLiteral("added")).toInt();
                const int missing = o.value(QStringLiteral("missing")).toInt();
                const int removed = o.value(QStringLiteral("removed")).toInt();
                if (added > 0 || removed > 0 || (m_lastMissing >= 0 && missing != m_lastMissing)) {
                    m_thumbsRequested = false;   // los nuevos necesitan miniatura
                    loadList();
                }
                if (added > 0)
                    emit notice(added == 1 ? QStringLiteral("1 fondo nuevo en tus carpetas")
                                           : QStringLiteral("%1 fondos nuevos en tus carpetas").arg(added), false);
                m_lastMissing = missing;
                const QString own = o.value(QStringLiteral("default_dir")).toString();
                if (own != m_defaultFolder) {
                    m_defaultFolder = own;
                    emit defaultFolderChanged();
                }

                // Vigilar las carpetas y subcarpetas actuales (con un límite razonable).
                QStringList dirs;
                for (const QJsonValue &d : o.value(QStringLiteral("dirs")).toArray())
                    if (dirs.size() < 2000)
                        dirs << d.toString();
                const QStringList old = m_watcher->directories();
                QStringList gone, fresh;
                for (const QString &d : old)
                    if (!dirs.contains(d))
                        gone << d;
                for (const QString &d : std::as_const(dirs))
                    if (!old.contains(d))
                        fresh << d;
                if (!gone.isEmpty())
                    m_watcher->removePaths(gone);
                if (!fresh.isEmpty())
                    m_watcher->addPaths(fresh);

                // Archivos aún copiándose: otra revisión en unos segundos.
                if (o.value(QStringLiteral("recent")).toInt() > 0)
                    QTimer::singleShot(3500, this, &Bridge::syncFolders);
            }
            if (m_syncAgain) {
                m_syncAgain = false;
                syncFolders();
            }
        });
}

void Bridge::loadVideoSettings()
{
    run({QStringLiteral("video-settings"), QStringLiteral("--json")},
        [this](int code, const QByteArray &out, const QByteArray &) {
            if (code != 0)
                return;
            const QJsonObject o = QJsonDocument::fromJson(out).object();
            m_pauseMode = o.value(QStringLiteral("pause_mode")).toInt();
            m_batteryPauses = o.value(QStringLiteral("battery_pauses")).toBool();
            m_batteryLevel = o.value(QStringLiteral("battery_level")).toInt();
            m_videoPlugin = o.value(QStringLiteral("plugin")).toBool();
            emit videoSettingsChanged();
        });
}

void Bridge::setVideoSettings(int pauseMode, bool batteryPauses, int batteryLevel)
{
    run({QStringLiteral("video-settings"), QStringLiteral("--json"),
         QStringLiteral("--pause-mode"), QString::number(pauseMode),
         QStringLiteral("--battery"), batteryPauses ? QStringLiteral("on") : QStringLiteral("off"),
         QStringLiteral("--battery-level"), QString::number(batteryLevel)},
        [this](int code, const QByteArray &, const QByteArray &err) {
            if (code != 0)
                emit notice(QStringLiteral("No se pudo cambiar la pausa: ") + cleanError(err), true);
            else
                emit notice(QStringLiteral("Pausa del vídeo actualizada"), false);
            loadVideoSettings();   // refleja lo que de verdad quedó guardado
        });
}

bool Bridge::isLarge(const QVariantMap &v) const
{
    return v.value(QStringLiteral("width")).toInt() > m_screenW
        || v.value(QStringLiteral("height")).toInt() > m_screenH;
}

QStringList Bridge::toLocalPaths(const QVariantList &paths)
{
    QStringList out;
    for (const QVariant &v : paths) {
        const QUrl url = v.toUrl();
        const QString local = url.isLocalFile() ? url.toLocalFile() : v.toString();
        if (!local.isEmpty())
            out << local;
    }
    return out;
}

void Bridge::watchDb()
{
    // Tras reemplazar el archivo (o si aún no existía) hay que volver a añadirlo.
    const QString dir = QFileInfo(m_dbPath).absolutePath();
    if (!m_dbWatcher->directories().contains(dir) && QFileInfo(dir).isDir())
        m_dbWatcher->addPath(dir);
    if (!m_dbWatcher->files().contains(m_dbPath) && QFileInfo::exists(m_dbPath))
        m_dbWatcher->addPath(m_dbPath);
}

void Bridge::loadList()
{
    run({QStringLiteral("list"), QStringLiteral("--json")},
        [this](int code, const QByteArray &out, const QByteArray &err) {
            if (code != 0) {
                emit error(cleanError(err));
                return;
            }
            const QVariantList fresh = QJsonDocument::fromJson(out).array().toVariantList();
            // Sin cambios no se redibuja nada (el selector se quedaría donde está igualmente,
            // pero así no parpadea al aplicar un fondo, que también escribe en la biblioteca).
            if (!m_loaded || fresh != m_wallpapers) {
                emit wallpapersAboutToChange();
                m_wallpapers = fresh;
                m_loaded = true;
                m_thumbsRequested = false;   // lo nuevo puede necesitar miniatura (si falla, la lista
                                             // ya no cambia y no se reintenta en bucle)
                emit wallpapersChanged();
            }

            // Si falta alguna miniatura, se generan en segundo plano y se recarga.
            bool missingThumb = false;
            for (const QVariant &v : std::as_const(m_wallpapers)) {
                const QVariantMap w = v.toMap();
                if (!w.value(QStringLiteral("missing")).toBool()
                    && w.value(QStringLiteral("thumb")).toString().isEmpty()) {
                    missingThumb = true;
                    break;
                }
            }
            if (missingThumb && !m_thumbsRequested)
                generateThumbs();
        });
}

void Bridge::generateThumbs()
{
    m_thumbsRequested = true;
    run({QStringLiteral("thumbs")}, [this](int, const QByteArray &, const QByteArray &) {
        loadList();
    });
}

void Bridge::loadStatus()
{
    run({QStringLiteral("status"), QStringLiteral("--json")},
        [this](int code, const QByteArray &out, const QByteArray &err) {
            if (code != 0) {
                emit error(cleanError(err));
                return;
            }
            const QJsonObject o = QJsonDocument::fromJson(out).object();
            m_backend = o.value(QStringLiteral("backend")).toString();
            m_video = o.value(QStringLiteral("capabilities")).toObject()
                          .value(QStringLiteral("video")).toBool();
            m_currentId = o.value(QStringLiteral("current")).toObject()
                              .value(QStringLiteral("id")).toString();
            const bool perMonitor = o.value(QStringLiteral("capabilities")).toObject()
                                        .value(QStringLiteral("per_monitor")).toBool();
            m_currentByScreen.clear();
            for (const QJsonValue &mv : o.value(QStringLiteral("monitors")).toArray()) {
                const QJsonObject mo = mv.toObject();
                m_currentByScreen.insert(mo.value(QStringLiteral("name")).toString(),
                                         mo.value(QStringLiteral("current")).toObject().value(QStringLiteral("id")).toString());
            }
            if (perMonitor != m_perMonitor) {
                m_perMonitor = perMonitor;
                emit screensChanged();
                emit targetChanged();
            }
            m_dbPath = o.value(QStringLiteral("db")).toString();
            watchDb();
            m_thumbsDir = o.value(QStringLiteral("thumbs_dir")).toString();
            emit statusChanged();
        });
}

void Bridge::apply(const QString &id)
{
    if (m_busy || id.isEmpty())
        return;
    setBusy(true);
    const QString monitor = resolvedScreen();   // "" = todas
    QStringList args{QStringLiteral("apply"), id};
    if (!monitor.isEmpty())
        args << QStringLiteral("--monitor") << monitor;
    run(args,
        [this, id, monitor](int code, const QByteArray &, const QByteArray &err) {
            setBusy(false);
            if (code == 0) {
                if (monitor.isEmpty()) {
                    m_currentId = id;
                    for (auto it = m_currentByScreen.begin(); it != m_currentByScreen.end(); ++it)
                        it.value() = id;
                } else {
                    m_currentByScreen.insert(monitor, id);
                    if (QGuiApplication::primaryScreen() && QGuiApplication::primaryScreen()->name() == monitor)
                        m_currentId = id;
                }
                emit statusChanged();
                emit applied(true, id, QString());
            } else {
                emit applied(false, id, cleanError(err));
            }
        });
}

void Bridge::setFavorite(const QString &id, bool on)
{
    QStringList args{QStringLiteral("favorite"), id};
    if (!on)
        args << QStringLiteral("--off");
    run(args, [this, id, on](int code, const QByteArray &, const QByteArray &err) {
        if (code != 0) {
            emit error(cleanError(err));
            return;
        }
        emit wallpapersAboutToChange();
        for (QVariant &v : m_wallpapers) {
            QVariantMap w = v.toMap();
            if (w.value(QStringLiteral("id")).toString() == id) {
                w[QStringLiteral("favorite")] = on;
                v = w;
                break;
            }
        }
        emit wallpapersChanged();
    });
}

QString Bridge::shortcut() const
{
    // El atajo lo gestiona KDE (kglobalaccel); aquí solo se informa el que se registró.
    return QStringLiteral("Meta+Alt+W");
}

void Bridge::importPaths(const QVariantList &paths)
{
    const QStringList local = toLocalPaths(paths);
    if (local.isEmpty())
        return;
    setBusy(true);
    emit notice(QStringLiteral("Analizando…"), false);
    run(QStringList{QStringLiteral("probe")} + local,
        [this, local](int, const QByteArray &out, const QByteArray &) {
            setBusy(false);
            emit notice(QString(), false);   // quita «Analizando…»
            QVariantList large;
            for (const QVariant &v : QJsonDocument::fromJson(out).array().toVariantList())
                if (isLarge(v.toMap()))
                    large << v;
            if (large.isEmpty() || m_largePolicy == QStringLiteral("keep")) {
                doImport(local, {}, QStringLiteral("keep"));
            } else if (m_largePolicy == QStringLiteral("ask")) {
                QVariantList ps;
                for (const QString &p : local)
                    ps << p;
                emit largeVideosFound(large, ps);   // la UI llama a importWithChoice
            } else {
                doImport(local, large, m_largePolicy);
            }
        });
}

void Bridge::importWithChoice(const QVariantList &paths, const QVariantList &videos,
                              const QString &choice, bool remember)
{
    if (remember)
        setLargeVideoPolicy(choice);
    doImport(toLocalPaths(paths), choice == QStringLiteral("keep") ? QVariantList{} : videos, choice);
}

void Bridge::doImport(const QStringList &local, const QVariantList &toOptimize, const QString &choice)
{
    QStringList args{QStringLiteral("import"), QStringLiteral("--json")};
    args << local;
    setBusy(true);
    emit notice(QStringLiteral("Importando…"), false);
    run(args, [this, toOptimize, choice](int code, const QByteArray &out, const QByteArray &err) {
        setBusy(false);
        if (code != 0) {
            emit notice(QStringLiteral("No se pudo importar: ") + cleanError(err), true);
            return;
        }
        const QJsonObject o = QJsonDocument::fromJson(out).object();
        const int added = o.value(QStringLiteral("added")).toInt();
        const int dups = o.value(QStringLiteral("duplicates")).toInt();
        const int unsup = o.value(QStringLiteral("unsupported")).toInt();
        const QJsonArray errs = o.value(QStringLiteral("errors")).toArray();
        QString text = QStringLiteral("Añadidos: %1").arg(added);
        if (dups)
            text += QStringLiteral(" · ya estaban: %1").arg(dups);
        if (unsup)
            text += QStringLiteral(" · no soportados: %1").arg(unsup);
        if (!errs.isEmpty())
            text += QStringLiteral(" · errores: %1 (%2)").arg(errs.size())
                        .arg(errs.first().toObject().value(QStringLiteral("error")).toString());
        emit notice(text, !errs.isEmpty() && added == 0);
        m_thumbsRequested = false;   // los nuevos necesitan miniatura
        loadList();
        syncFolders();   // empieza a vigilar la carpeta recién añadida
        if (!toOptimize.isEmpty())
            optimizeVideos(toOptimize, choice);
    });
}

void Bridge::removeWallpaper(const QString &id)
{
    QString path;
    for (const QVariant &v : std::as_const(m_wallpapers)) {
        const QVariantMap w = v.toMap();
        if (w.value(QStringLiteral("id")).toString() == id)
            path = w.value(QStringLiteral("path")).toString();
    }
    run({QStringLiteral("remove"), id},
        [this, id, path](int code, const QByteArray &, const QByteArray &err) {
            if (code != 0) {
                emit notice(QStringLiteral("No se pudo quitar: ") + cleanError(err), true);
                return;
            }
            emit removed(id, path);
            loadList();
        });
}

void Bridge::regenerateThumbs()
{
    if (m_busy)
        return;
    setBusy(true);
    emit notice(QStringLiteral("Regenerando miniaturas…"), false);
    run({QStringLiteral("thumbs"), QStringLiteral("--force")},
        [this](int code, const QByteArray &out, const QByteArray &err) {
            setBusy(false);
            ++m_thumbVersion;   // fuerza a QML a recargar las imágenes
            emit notice(code == 0 ? QString::fromUtf8(out).trimmed() : cleanError(err), code != 0);
            loadList();
        });
}

void Bridge::openHud()
{
    QProcess::startDetached(QCoreApplication::applicationFilePath(), {QStringLiteral("--hud")});
}

void Bridge::openShortcutSettings()
{
    const bool ok = m_backend == QStringLiteral("mate")
        ? QProcess::startDetached(QStringLiteral("mate-keybinding-properties"), {})
        : QProcess::startDetached(QStringLiteral("systemsettings"), {QStringLiteral("kcm_keys")});
    if (!ok)
        emit notice(QStringLiteral("No se pudo abrir la configuración de atajos"), true);
}

// Texto → cadena GVariant ('…' con \ y ' escapados): gsettings analiza el valor como GVariant, y un
// texto con comillas o espacios sin envolver da «expected end of input».
static QString gvariantString(QString s)
{
    s.replace(QLatin1Char('\\'), QStringLiteral("\\\\")).replace(QLatin1Char('\''), QStringLiteral("\\'"));
    return QLatin1Char('\'') + s + QLatin1Char('\'');
}

void Bridge::registerMateShortcut()
{
    // Dos atajos personalizados de MATE (efecto inmediato):
    //   Meta+Alt+W → selector rápido · Meta+Alt+D → mostrar/ocultar iconos con un vídeo de fondo
    const QString base = QStringLiteral("org.mate.control-center.keybinding:/org/mate/desktop/keybindings/");
    const QString exe = QCoreApplication::applicationFilePath();
    const QList<QStringList> keys = {
        {QStringLiteral("nekowall/"), QStringLiteral("NekoWall · Selector rápido"),
         QStringLiteral("\"%1\" --hud").arg(exe), QStringLiteral("<Mod4><Alt>w")},
        {QStringLiteral("nekowall-peek/"), QStringLiteral("NekoWall · Mostrar escritorio"),
         QStringLiteral("\"%1\" peek").arg(m_cli), QStringLiteral("<Mod4><Alt>d")},
    };
    for (const QStringList &k : keys) {
        const QList<QStringList> sets = {
            {QStringLiteral("name"), k[1]}, {QStringLiteral("action"), k[2]}, {QStringLiteral("binding"), k[3]}};
        for (const QStringList &kv : sets) {
            QProcess p;
            p.start(QStringLiteral("gsettings"), {QStringLiteral("set"), base + k[0], kv[0], gvariantString(kv[1])});
            if (!p.waitForFinished(3000) || p.exitCode() != 0) {
                emit notice(QStringLiteral("No se pudo registrar el atajo: ") + cleanError(p.readAllStandardError()), true);
                return;
            }
        }
    }
    emit notice(QStringLiteral("Atajos registrados: Meta+Alt+W (selector) y Meta+Alt+D (mostrar escritorio)"), false);
}

void Bridge::openDefaultFolder()
{
    if (m_defaultFolder.isEmpty())
        return;
    QDir().mkpath(m_defaultFolder);
    QDesktopServices::openUrl(QUrl::fromLocalFile(m_defaultFolder));
}

void Bridge::revealInFolder(const QString &path)
{
    QDesktopServices::openUrl(QUrl::fromLocalFile(QFileInfo(path).absolutePath()));
}

void Bridge::requestOptimize(const QString &path)
{
    run({QStringLiteral("probe"), path}, [this](int code, const QByteArray &out, const QByteArray &err) {
        const QVariantList arr = QJsonDocument::fromJson(out).array().toVariantList();
        if (code != 0 || arr.isEmpty() || arr.first().toMap().contains(QStringLiteral("error"))) {
            emit notice(QStringLiteral("No se pudo analizar el vídeo: ") + cleanError(err), true);
            return;
        }
        const QVariantMap v = arr.first().toMap();
        if (!isLarge(v) && v.value(QStringLiteral("fps")).toDouble() <= 31) {
            emit notice(QStringLiteral("«%1» ya es adecuado para tu pantalla (%2×%3, %4 fps)")
                            .arg(v.value(QStringLiteral("name")).toString())
                            .arg(v.value(QStringLiteral("width")).toInt())
                            .arg(v.value(QStringLiteral("height")).toInt())
                            .arg(qRound(v.value(QStringLiteral("fps")).toDouble())), false);
            return;
        }
        emit optimizeOffer(arr);
    });
}

void Bridge::optimizeVideos(const QVariantList &videos, const QString &choice)
{
    const int fps = choice == QStringLiteral("60") ? 60 : 30;
    for (const QVariant &v : videos) {
        const QVariantMap m = v.toMap();
        m_queue.append({m.value(QStringLiteral("path")).toString(),
                        m.value(QStringLiteral("name")).toString(), fps});
    }
    if (!m_taskRunning) {
        m_jobsDone = m_jobsFailed = 0;
        runNextOptimize();
    }
    emit taskChanged();
}

// Una conversión cada vez: ffmpeg ya usa todos los núcleos.
void Bridge::runNextOptimize()
{
    if (m_queue.isEmpty()) {
        m_taskRunning = false;
        emit taskChanged();
        if (m_jobsDone + m_jobsFailed > 0)
            emit notice(m_jobsFailed == 0
                            ? QStringLiteral("Optimización terminada: %1 vídeo(s) listos (el original sigue en su carpeta)").arg(m_jobsDone)
                            : QStringLiteral("Optimización: %1 listos, %2 con error").arg(m_jobsDone).arg(m_jobsFailed),
                        m_jobsFailed > 0);
        m_thumbsRequested = false;
        loadList();
        return;
    }
    const Job job = m_queue.takeFirst();
    m_taskRunning = true;
    m_taskProgress = 0;
    const int pending = m_queue.size();
    m_taskText = QStringLiteral("Optimizando «%1»%2").arg(job.name,
                     pending ? QStringLiteral(" (quedan %1)").arg(pending) : QString());
    emit taskChanged();

    auto *p = new QProcess(this);
    connect(p, &QProcess::readyReadStandardOutput, this, [this, p] {
        while (p->canReadLine()) {
            const QByteArray line = p->readLine().trimmed();
            if (line.startsWith("progress ")) {
                m_taskProgress = line.mid(9).toInt();
                emit taskChanged();
            }
        }
    });
    connect(p, &QProcess::finished, this, [this, p, job](int code, QProcess::ExitStatus st) {
        const QByteArray err = p->readAllStandardError();
        if (st == QProcess::NormalExit && code == 0) {
            ++m_jobsDone;
        } else {
            ++m_jobsFailed;
            emit notice(QStringLiteral("No se pudo optimizar «%1»: %2").arg(job.name, cleanError(err)), true);
        }
        p->deleteLater();
        runNextOptimize();
    });
    p->start(m_cli, {QStringLiteral("optimize"), job.path,
                     QStringLiteral("--height"), QString::number(qMin(m_screenH, m_screenW)),
                     QStringLiteral("--fps"), QString::number(job.fps),
                     QStringLiteral("--replace-in-library")});
}

QString Bridge::formatSize(qint64 bytes) const
{
    if (bytes <= 0)
        return QString();
    const double mb = bytes / (1024.0 * 1024.0);
    if (mb >= 1.0)
        return QStringLiteral("%1 MB").arg(mb, 0, 'f', 1);
    return QStringLiteral("%1 KB").arg(qMax<qint64>(1, bytes / 1024));
}
