// Workshop: buscar y descargar fondos de Wallhaven (wallhaven.cc, API pública sin clave).
//
// - Siempre contenido apto (purity=100, SFW): no hay opción para cambiarlo.
// - Las descargas van a la carpeta propia (nekowallpapers) como «wallhaven-<id>.<ext>»; como esa
//   carpeta está vigilada, aparecen solas en la biblioteca.
// - No se conecta a nada hasta que el usuario abre la página Workshop.
#pragma once

#include <QDateTime>
#include <QAbstractListModel>
#include <QHash>
#include <QTimer>
#include <QUrl>
#include <QObject>
#include <QQmlNetworkAccessManagerFactory>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

class QNetworkAccessManager;
class QNetworkReply;

// Las miniaturas que pide QML (Image { source: "https://…" }) se guardan en ~/.cache/nekowall/workshop:
// volver a ver una búsqueda no gasta red ni el límite de Wallhaven.
class CachedNetworkFactory : public QQmlNetworkAccessManagerFactory
{
public:
    QNetworkAccessManager *create(QObject *parent) override;
    static QString cacheDir();
    static void clearAll();   // borra las miniaturas guardadas (limpieza diaria)
};

// Resultados para la rejilla. Al llegar una página nueva solo se AÑADEN filas: la rejilla no vuelve
// arriba. Cada fila tiene un único rol, «wp», con el mapa del fondo.
class WorkshopModel : public QAbstractListModel
{
    Q_OBJECT
public:
    using QAbstractListModel::QAbstractListModel;
    int rowCount(const QModelIndex &parent = {}) const override { return parent.isValid() ? 0 : int(m_items.size()); }
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override { return {{Qt::UserRole + 1, "wp"}}; }
    void clear();
    void append(const QVariantList &items);

private:
    QVariantList m_items;
};

class Workshop : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QObject *model READ model CONSTANT)
    Q_PROPERTY(QVariantList results READ results NOTIFY resultsChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY loadingChanged)
    Q_PROPERTY(bool hasMore READ hasMore NOTIFY resultsChanged)
    Q_PROPERTY(int total READ total NOTIFY resultsChanged)
    Q_PROPERTY(bool searched READ searched NOTIFY resultsChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(QString folder READ folder WRITE setFolder NOTIFY folderChanged)
    // Rutas de la biblioteca: un fondo cuenta como «Descargado» solo si está en ella (si el usuario lo
    // eliminó, se puede volver a descargar aunque quede el archivo).
    Q_PROPERTY(QStringList libraryPaths READ libraryPaths WRITE setLibraryPaths NOTIFY downloadedChanged)
    // id → porcentaje (0-100) de las descargas en curso
    Q_PROPERTY(QVariantMap progress READ progress NOTIFY progressChanged)
    // ids de Wallhaven que ya están en la carpeta
    Q_PROPERTY(QStringList downloaded READ downloaded NOTIFY downloadedChanged)

public:
    explicit Workshop(QObject *parent = nullptr);

    QVariantList results() const { return m_results; }
    QObject *model() const { return m_model; }
    bool loading() const { return m_loading; }
    bool hasMore() const { return m_page < m_lastPage; }
    int total() const { return m_total; }
    bool searched() const { return m_searched; }
    QString error() const { return m_error; }
    QString folder() const { return m_folder; }
    void setFolder(const QString &f);
    QVariantMap progress() const { return m_progress; }
    QStringList downloaded() const { return m_downloaded; }
    QStringList libraryPaths() const { return m_libraryPaths; }
    void setLibraryPaths(const QStringList &paths);

    // sorting: popular | recent | toplist | random · categories: "111" (general, anime, personas)
    // atleast: "1920x1080" o "" para cualquier tamaño.
    Q_INVOKABLE void search(const QString &query, const QString &sorting, const QString &categories,
                            const QString &atleast);
    Q_INVOKABLE void loadMore();
    Q_INVOKABLE void fetchTags(const QString &id, int attempt = 1);
    Q_INVOKABLE void download(const QVariantMap &wallpaper, bool applyAfter);
    Q_INVOKABLE QString fileFor(const QString &id) const;   // ruta en la biblioteca o ""

signals:
    void resultsChanged();
    void loadingChanged();
    void errorChanged();
    void folderChanged();
    void progressChanged();
    void downloadedChanged();
    void tagsReady(const QString &id, const QVariantList &tags, const QString &uploader);
    void downloadFinished(const QString &id, const QString &path, bool applyAfter);
    void downloadFailed(const QString &id, const QString &message);

private:
    void fetchPage(int page, bool prefetch, int attempt = 1);
    void applyPage(const QByteArray &data, int page);
    QUrl pageUrl(int page) const;
    static QString searchesDir();
    QString cachePath(int page) const;
    QByteArray readCache(int page) const;
    void writeCache(int page, const QByteArray &data) const;
    static void writeCacheFor(const QUrl &url, const QByteArray &data);
    static QString cacheFileFor(const QUrl &url);
    void cleanupIfDue();
    void clearCaches();
    static QDateTime lastCleanupMark(const QDateTime &now);
    static bool retryable(QNetworkReply *reply);
    void setLoading(bool on);
    void setError(const QString &e);
    static QString idOfPath(const QString &path);   // «…/wallhaven-1pqgxg (1920x1080).jpg» → «1pqgxg»
    static QString networkError(QNetworkReply *reply);

    QNetworkAccessManager *m_net = nullptr;
    WorkshopModel *m_model = nullptr;
    QVariantList m_results;
    bool m_loading = false;
    bool m_searched = false;
    QString m_error;
    QString m_folder;
    QVariantMap m_progress;
    QStringList m_downloaded;
    QStringList m_libraryPaths;
    QHash<QString, QString> m_pathById;

    QString m_query, m_sorting, m_categories, m_atleast, m_seed;
    int m_page = 0;
    int m_lastPage = 0;
    int m_total = 0;
    int m_generation = 0;   // descarta respuestas de búsquedas anteriores
    QHash<int, QByteArray> m_prefetched;   // páginas pedidas por adelantado
    int m_prefetchPage = 0;                // página que se está pidiendo por adelantado (0 = ninguna)
    bool m_waitPrefetch = false;           // el usuario ya llegó a esa página: mostrarla al llegar
    QTimer m_cleanupTimer;
};
