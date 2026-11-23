// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
#include "Workshop.h"

#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QMutex>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkDiskCache>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QPointer>
#include <QStandardPaths>
#include <QTimer>
#include <QUrlQuery>

namespace {

const QString kApi = QStringLiteral("https://wallhaven.cc/api/v1");
const QByteArray kAgent = QByteArrayLiteral("NekoWall/0.1");

QNetworkRequest request(const QUrl &url)
{
    QNetworkRequest r(url);
    r.setHeader(QNetworkRequest::UserAgentHeader, kAgent);
    r.setTransferTimeout(30000);
    return r;
}

// Solo «wallhaven-<id>.<ext>» con un id normal: el nombre del archivo sale de la respuesta y no
// debe poder escribir fuera de la carpeta.
bool validId(const QString &id)
{
    if (id.isEmpty() || id.size() > 16)
        return false;
    for (QChar c : id)
        if (!c.isLetterOrNumber())
            return false;
    return true;
}

} // namespace

namespace {
QMutex cachesLock;
QList<QPointer<QNetworkDiskCache>> caches;   // las cachés de miniaturas que QML está usando
}

QNetworkAccessManager *CachedNetworkFactory::create(QObject *parent)
{
    auto *nam = new QNetworkAccessManager(parent);
    auto *cache = new QNetworkDiskCache(nam);
    cache->setCacheDirectory(cacheDir());
    cache->setMaximumCacheSize(200LL * 1024 * 1024);
    nam->setCache(cache);
    QMutexLocker lock(&cachesLock);
    caches << QPointer<QNetworkDiskCache>(cache);
    return nam;
}

QString CachedNetworkFactory::cacheDir()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation)
        + QStringLiteral("/nekowall/workshop");
}

void CachedNetworkFactory::clearAll()
{
    QMutexLocker lock(&cachesLock);
    bool any = false;
    for (const QPointer<QNetworkDiskCache> &c : std::as_const(caches)) {
        if (c) {   // cada caché vive en el hilo de QML que la creó: se vacía desde ese hilo
            QMetaObject::invokeMethod(c.data(), [c] { if (c) c->clear(); }, Qt::QueuedConnection);
            any = true;
        }
    }
    if (!any)
        QDir(cacheDir()).removeRecursively();
}

QVariant WorkshopModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_items.size() || role != Qt::UserRole + 1)
        return {};
    return m_items.at(index.row());
}

void WorkshopModel::clear()
{
    beginResetModel();
    m_items.clear();
    endResetModel();
}

void WorkshopModel::append(const QVariantList &items)
{
    if (items.isEmpty())
        return;
    beginInsertRows({}, int(m_items.size()), int(m_items.size() + items.size() - 1));
    m_items << items;
    endInsertRows();
}

Workshop::Workshop(QObject *parent)
    : QObject(parent)
    , m_net(new QNetworkAccessManager(this))
    , m_model(new WorkshopModel(this))
{
    m_cleanupTimer.setSingleShot(true);
    connect(&m_cleanupTimer, &QTimer::timeout, this, &Workshop::cleanupIfDue);
    cleanupIfDue();
}

void Workshop::setFolder(const QString &f)
{
    if (f == m_folder)
        return;
    m_folder = f;
    emit folderChanged();
}

QString Workshop::idOfPath(const QString &path)
{
    const QString name = QFileInfo(path).fileName();
    if (!name.startsWith(QStringLiteral("wallhaven-")))
        return {};
    QString id;
    for (QChar c : name.mid(10)) {
        if (!c.isLetterOrNumber())
            break;
        id += c;
    }
    return validId(id) ? id : QString();
}

void Workshop::setLibraryPaths(const QStringList &paths)
{
    if (paths == m_libraryPaths)
        return;
    m_libraryPaths = paths;
    m_pathById.clear();
    for (const QString &p : paths) {
        const QString id = idOfPath(p);
        // Si hay varios (original y copia ajustada), vale cualquiera; se prefiere el primero visto.
        if (!id.isEmpty() && !m_pathById.contains(id))
            m_pathById.insert(id, p);
    }
    m_downloaded = m_pathById.keys();
    emit downloadedChanged();
}

QString Workshop::fileFor(const QString &id) const
{
    return m_pathById.value(id);
}

void Workshop::setLoading(bool on)
{
    if (on != m_loading) {
        m_loading = on;
        emit loadingChanged();
    }
}

void Workshop::setError(const QString &e)
{
    if (e != m_error) {
        m_error = e;
        emit errorChanged();
    }
}

QString Workshop::networkError(QNetworkReply *reply)
{
    const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    if (status == 429)
        return QStringLiteral("Wallhaven pide esperar un momento (demasiadas búsquedas seguidas). Prueba en un minuto.");
    if (status >= 500)
        return QStringLiteral("Wallhaven no responde ahora mismo (error %1). Prueba más tarde.").arg(status);
    switch (reply->error()) {
    case QNetworkReply::HostNotFoundError:
    case QNetworkReply::ConnectionRefusedError:
    case QNetworkReply::TemporaryNetworkFailureError:
    case QNetworkReply::NetworkSessionFailedError:
    case QNetworkReply::UnknownNetworkError:
        return QStringLiteral("Sin conexión con Wallhaven. Revisa tu internet.");
    case QNetworkReply::OperationCanceledError:
    case QNetworkReply::TimeoutError:
        return QStringLiteral("Wallhaven tardó demasiado en responder.");
    default:
        return reply->errorString();
    }
}

// Wallhaven a veces responde 502/503 un rato: se reintenta solo antes de mostrar un error.
bool Workshop::retryable(QNetworkReply *reply)
{
    const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    return status == 500 || status == 502 || status == 503 || status == 504
        || reply->error() == QNetworkReply::TimeoutError
        || reply->error() == QNetworkReply::TemporaryNetworkFailureError
        || reply->error() == QNetworkReply::RemoteHostClosedError;
}

void Workshop::search(const QString &query, const QString &sorting, const QString &categories,
                      const QString &atleast)
{
    m_query = query.trimmed();
    m_sorting = sorting;
    m_categories = categories == QStringLiteral("000") ? QStringLiteral("111") : categories;
    m_atleast = atleast;
    m_seed.clear();
    m_page = 0;
    m_lastPage = 0;
    m_total = 0;
    m_results.clear();
    m_model->clear();   // búsqueda nueva: la rejilla sí vuelve arriba
    m_searched = false;
    m_prefetched.clear();
    m_prefetchPage = 0;
    m_waitPrefetch = false;
    ++m_generation;
    setError({});
    emit resultsChanged();
    fetchPage(1, false);
}

void Workshop::loadMore()
{
    if (!m_loading && hasMore())
        fetchPage(m_page + 1, false);
}

QUrl Workshop::pageUrl(int page) const
{
    QUrlQuery q;
    if (!m_query.isEmpty())
        q.addQueryItem(QStringLiteral("q"), m_query);
    q.addQueryItem(QStringLiteral("categories"), m_categories);
    q.addQueryItem(QStringLiteral("purity"), QStringLiteral("100"));   // solo SFW, siempre
    if (m_sorting == QStringLiteral("recent")) {
        q.addQueryItem(QStringLiteral("sorting"), QStringLiteral("date_added"));
    } else if (m_sorting == QStringLiteral("toplist")) {
        q.addQueryItem(QStringLiteral("sorting"), QStringLiteral("toplist"));
        q.addQueryItem(QStringLiteral("topRange"), QStringLiteral("1M"));
    } else if (m_sorting == QStringLiteral("random")) {
        q.addQueryItem(QStringLiteral("sorting"), QStringLiteral("random"));
        if (!m_seed.isEmpty())
            q.addQueryItem(QStringLiteral("seed"), m_seed);   // mismas páginas al seguir bajando
    } else {
        q.addQueryItem(QStringLiteral("sorting"), m_query.isEmpty() ? QStringLiteral("favorites")
                                                                    : QStringLiteral("relevance"));
    }
    if (!m_atleast.isEmpty())
        q.addQueryItem(QStringLiteral("atleast"), m_atleast);
    q.addQueryItem(QStringLiteral("page"), QString::number(page));
    QUrl url(kApi + QStringLiteral("/search"));
    url.setQuery(q);
    return url;
}

// ── Búsquedas guardadas ────────────────────────────────────────────────────
// Cada página de resultados se guarda en ~/.cache/nekowall/busquedas: repetir una búsqueda del día
// es instantáneo y no gasta red. «Recientes» caduca a los 15 min (cambia a menudo); «Aleatorio» no
// se guarda. Todo se borra cada día a las 23:59 (ver cleanupIfDue).

QString Workshop::searchesDir()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation)
        + QStringLiteral("/nekowall/busquedas");
}

QString Workshop::cachePath(int page) const
{
    return cacheFileFor(pageUrl(page));
}

QString Workshop::cacheFileFor(const QUrl &url)
{
    const QByteArray key = url.toString().toUtf8();
    return searchesDir() + QLatin1Char('/')
        + QString::fromLatin1(QCryptographicHash::hash(key, QCryptographicHash::Sha1).toHex().left(20))
        + QStringLiteral(".json");
}

QByteArray Workshop::readCache(int page) const
{
    if (m_sorting == QStringLiteral("random"))
        return {};
    const QString path = cachePath(page);
    const QFileInfo info(path);
    if (!info.exists())
        return {};
    if (m_sorting == QStringLiteral("recent")
        && info.lastModified().secsTo(QDateTime::currentDateTime()) > 15 * 60)
        return {};
    QFile f(path);
    return f.open(QIODevice::ReadOnly) ? f.readAll() : QByteArray();
}

void Workshop::writeCache(int page, const QByteArray &data) const
{
    if (m_sorting != QStringLiteral("random"))
        writeCacheFor(pageUrl(page), data);
}

void Workshop::writeCacheFor(const QUrl &url, const QByteArray &data)
{
    if (QUrlQuery(url).queryItemValue(QStringLiteral("sorting")) == QStringLiteral("random")
        || data.isEmpty() || !QDir().mkpath(searchesDir()))
        return;
    QFile f(cacheFileFor(url));
    if (f.open(QIODevice::WriteOnly | QIODevice::Truncate))
        f.write(data);
}

// ── Pedir páginas ──────────────────────────────────────────────────────────
// prefetch = pedir en segundo plano la página siguiente a la que se ve, para que al bajar ya esté.

void Workshop::fetchPage(int page, bool prefetch, int attempt)
{
    if (attempt == 1) {
        // 1) Ya pedida por adelantado y guardada en memoria
        if (!prefetch && m_prefetched.contains(page)) {
            applyPage(m_prefetched.take(page), page);
            return;
        }
        // 2) En las búsquedas guardadas
        const QByteArray cached = readCache(page);
        if (!cached.isEmpty()) {
            if (prefetch)
                m_prefetched.insert(page, cached);
            else
                applyPage(cached, page);
            return;
        }
        // 3) Ya se está pidiendo por adelantado: esperar a que llegue
        if (!prefetch && m_prefetchPage == page) {
            m_waitPrefetch = true;
            setLoading(true);
            return;
        }
        if (prefetch) {
            if (m_prefetchPage != 0)
                return;
            m_prefetchPage = page;
        } else {
            setLoading(true);
            setError({});
        }
    }

    const int gen = m_generation;
    QNetworkReply *reply = m_net->get(request(pageUrl(page)));
    connect(reply, &QNetworkReply::finished, this, [this, reply, gen, page, prefetch, attempt] {
        reply->deleteLater();
        if (gen != m_generation) {
            // El usuario ya hizo otra búsqueda: la página se guarda igual (sirve si vuelve a ella).
            if (reply->error() == QNetworkReply::NoError)
                writeCacheFor(reply->url(), reply->readAll());
            return;
        }
        // Wallhaven falla rápido (≈1 s) cuando falla: se reintenta cada medio segundo.
        if (reply->error() != QNetworkReply::NoError && attempt < 8 && retryable(reply)) {
            QTimer::singleShot(500, this, [this, gen, page, prefetch, attempt] {
                if (gen == m_generation)
                    fetchPage(page, prefetch, attempt + 1);
            });
            return;
        }
        const bool waited = prefetch && m_waitPrefetch;
        if (prefetch) {
            m_prefetchPage = 0;
            m_waitPrefetch = false;
        }
        if (reply->error() != QNetworkReply::NoError) {
            if (!prefetch || waited) {
                setLoading(false);
                setError(networkError(reply));
            }
            return;
        }
        const QByteArray data = reply->readAll();
        writeCache(page, data);
        if (prefetch && !waited)
            m_prefetched.insert(page, data);
        else
            applyPage(data, page);
    });
}

void Workshop::applyPage(const QByteArray &data, int page)
{
    setLoading(false);
    const QJsonObject root = QJsonDocument::fromJson(data).object();
    const QJsonObject meta = root.value(QStringLiteral("meta")).toObject();
    QVariantList fresh;
    for (const QJsonValue &v : root.value(QStringLiteral("data")).toArray()) {
        const QJsonObject o = v.toObject();
        if (o.value(QStringLiteral("purity")).toString() != QStringLiteral("sfw"))
            continue;   // por si acaso: nunca mostrar otra cosa
        const QJsonObject th = o.value(QStringLiteral("thumbs")).toObject();
        QVariantMap w;
        w[QStringLiteral("id")] = o.value(QStringLiteral("id")).toString();
        w[QStringLiteral("url")] = o.value(QStringLiteral("url")).toString();
        w[QStringLiteral("full")] = o.value(QStringLiteral("path")).toString();
        w[QStringLiteral("thumb")] = th.value(QStringLiteral("large")).toString();
        w[QStringLiteral("width")] = o.value(QStringLiteral("dimension_x")).toInt();
        w[QStringLiteral("height")] = o.value(QStringLiteral("dimension_y")).toInt();
        w[QStringLiteral("resolution")] = o.value(QStringLiteral("resolution")).toString();
        w[QStringLiteral("size")] = o.value(QStringLiteral("file_size")).toDouble();
        w[QStringLiteral("type")] = o.value(QStringLiteral("file_type")).toString();
        w[QStringLiteral("category")] = o.value(QStringLiteral("category")).toString();
        w[QStringLiteral("favorites")] = o.value(QStringLiteral("favorites")).toInt();
        w[QStringLiteral("source")] = o.value(QStringLiteral("source")).toString();
        QStringList colors;
        for (const QJsonValue &c : o.value(QStringLiteral("colors")).toArray())
            colors << c.toString();
        w[QStringLiteral("colors")] = colors;
        if (validId(w.value(QStringLiteral("id")).toString()))
            fresh << w;
    }
    m_results << fresh;
    m_model->append(fresh);   // solo se añaden filas: la rejilla se queda donde estaba
    m_page = meta.value(QStringLiteral("current_page")).toInt(page);
    m_lastPage = meta.value(QStringLiteral("last_page")).toInt(m_page);
    m_total = meta.value(QStringLiteral("total")).toInt();
    const QJsonValue seed = meta.value(QStringLiteral("seed"));
    if (seed.isString())
        m_seed = seed.toString();
    m_searched = true;
    emit resultsChanged();
    if (hasMore())
        fetchPage(m_page + 1, true);   // la siguiente, por adelantado
}

// ── Limpieza diaria a las 23:59 ────────────────────────────────────────────
// Se borran las búsquedas guardadas y las miniaturas de Workshop. Si NekoWall estaba cerrado a esa
// hora, se borra al abrirlo. Las descargas (nekowallpapers) no se tocan nunca.

QDateTime Workshop::lastCleanupMark(const QDateTime &now)
{
    QDateTime mark(now.date(), QTime(23, 59));
    return now >= mark ? mark : mark.addDays(-1);
}

void Workshop::cleanupIfDue()
{
    const QString stampPath = QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation)
        + QStringLiteral("/nekowall/workshop-limpieza");
    const QDateTime now = QDateTime::currentDateTime();
    QFile stamp(stampPath);
    QDateTime last;
    if (stamp.open(QIODevice::ReadOnly))
        last = QDateTime::fromString(QString::fromUtf8(stamp.readAll()).trimmed(), Qt::ISODate);
    stamp.close();
    if (!last.isValid() || last < lastCleanupMark(now)) {
        if (last.isValid() || QFileInfo::exists(searchesDir()))
            clearCaches();
        QDir().mkpath(QFileInfo(stampPath).absolutePath());
        if (stamp.open(QIODevice::WriteOnly | QIODevice::Truncate))
            stamp.write(now.toString(Qt::ISODate).toUtf8());
    }
    // Próxima limpieza: hoy a las 23:59 o, si ya pasó, mañana.
    QDateTime next(now.date(), QTime(23, 59));
    if (now >= next)
        next = next.addDays(1);
    m_cleanupTimer.start(int(qBound<qint64>(1000, now.msecsTo(next) + 500, 24LL * 3600 * 1000)));
}

void Workshop::clearCaches()
{
    QDir(searchesDir()).removeRecursively();
    m_prefetched.clear();
    CachedNetworkFactory::clearAll();
}

void Workshop::fetchTags(const QString &id, int attempt)
{
    if (!validId(id))
        return;
    QNetworkReply *reply = m_net->get(request(QUrl(kApi + QStringLiteral("/w/") + id)));
    connect(reply, &QNetworkReply::finished, this, [this, reply, id, attempt] {
        reply->deleteLater();
        if (reply->error() != QNetworkReply::NoError) {
            if (attempt < 5 && retryable(reply))
                QTimer::singleShot(1000 * attempt, this, [this, id, attempt] { fetchTags(id, attempt + 1); });
            return;
        }   // las etiquetas son un extra: sin ellas el panel funciona igual
        const QJsonObject d = QJsonDocument::fromJson(reply->readAll()).object()
                                  .value(QStringLiteral("data")).toObject();
        QVariantList tags;
        for (const QJsonValue &t : d.value(QStringLiteral("tags")).toArray()) {
            const QJsonObject o = t.toObject();
            if (o.value(QStringLiteral("purity")).toString() == QStringLiteral("sfw"))
                tags << o.value(QStringLiteral("name")).toString();
        }
        const QString uploader = d.value(QStringLiteral("uploader")).toObject()
                                     .value(QStringLiteral("username")).toString();
        emit tagsReady(id, tags, uploader);
    });
}

void Workshop::download(const QVariantMap &w, bool applyAfter)
{
    const QString id = w.value(QStringLiteral("id")).toString();
    const QUrl url(w.value(QStringLiteral("full")).toString());
    if (!validId(id) || m_progress.contains(id))
        return;
    if (url.scheme() != QStringLiteral("https") || !url.host().endsWith(QStringLiteral("wallhaven.cc"))) {
        emit downloadFailed(id, QStringLiteral("enlace de descarga no válido"));
        return;
    }
    if (m_folder.isEmpty() || !QDir().mkpath(m_folder)) {
        emit downloadFailed(id, QStringLiteral("no encuentro la carpeta nekowallpapers"));
        return;
    }
    const QString existing = fileFor(id);
    if (!existing.isEmpty()) {   // ya está en la biblioteca: no se descarga dos veces
        // (si el usuario lo eliminó pero quedó el archivo, sí se descarga de nuevo y se reemplaza)
        emit downloadFinished(id, existing, applyAfter);
        return;
    }

    QString ext = QFileInfo(url.path()).suffix().toLower();
    if (ext != QStringLiteral("png") && ext != QStringLiteral("jpg") && ext != QStringLiteral("jpeg"))
        ext = QStringLiteral("jpg");
    const QString finalPath = QDir(m_folder).filePath(QStringLiteral("wallhaven-%1.%2").arg(id, ext));
    // Oculto mientras se descarga: la revisión de carpetas se salta los archivos que empiezan por «.».
    const QString partPath = QDir(m_folder).filePath(QStringLiteral(".wallhaven-%1.%2.part").arg(id, ext));
    auto *file = new QFile(partPath, this);
    if (!file->open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        delete file;
        emit downloadFailed(id, QStringLiteral("no se puede escribir en ") + m_folder);
        return;
    }

    m_progress[id] = 0;
    emit progressChanged();
    QNetworkReply *reply = m_net->get(request(url));
    connect(reply, &QNetworkReply::readyRead, file, [reply, file] { file->write(reply->readAll()); });
    connect(reply, &QNetworkReply::downloadProgress, this, [this, id](qint64 got, qint64 total) {
        if (total > 0) {
            const int pct = int(got * 100 / total);
            if (m_progress.value(id).toInt() != pct) {
                m_progress[id] = pct;
                emit progressChanged();
            }
        }
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply, file, id, partPath, finalPath, applyAfter] {
        reply->deleteLater();
        file->write(reply->readAll());
        file->close();
        file->deleteLater();
        m_progress.remove(id);
        emit progressChanged();
        if (reply->error() != QNetworkReply::NoError || file->size() == 0) {
            QFile::remove(partPath);
            emit downloadFailed(id, reply->error() != QNetworkReply::NoError ? networkError(reply)
                                                                              : QStringLiteral("el archivo llegó vacío"));
            return;
        }
        QFile::remove(finalPath);
        if (!QFile::rename(partPath, finalPath)) {
            QFile::remove(partPath);
            emit downloadFailed(id, QStringLiteral("no se pudo guardar en ") + m_folder);
            return;
        }
        emit downloadFinished(id, finalPath, applyAfter);
    });
}
