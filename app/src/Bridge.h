#pragma once
// Puente entre QML y el núcleo Rust. Todo pasa por la CLI `nekowall` (salida --json),
// siempre con QProcess asíncrono: la interfaz nunca espera a Plasma ni a ffmpeg.

#include <functional>

#include <QObject>
#include <QString>
#include <QVariantList>
#include <QVariantMap>

class QProcess;

class Bridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList wallpapers READ wallpapers NOTIFY wallpapersChanged)
    Q_PROPERTY(QString currentId READ currentId NOTIFY statusChanged)
    Q_PROPERTY(QString backend READ backend NOTIFY statusChanged)
    Q_PROPERTY(bool videoSupported READ videoSupported NOTIFY statusChanged)
    Q_PROPERTY(bool loaded READ loaded NOTIFY wallpapersChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(bool reducedMotion READ reducedMotion CONSTANT)
    Q_PROPERTY(QString dbPath READ dbPath NOTIFY statusChanged)
    Q_PROPERTY(QString thumbsDir READ thumbsDir NOTIFY statusChanged)
    Q_PROPERTY(int thumbVersion READ thumbVersion NOTIFY wallpapersChanged)
    Q_PROPERTY(QString shortcut READ shortcut CONSTANT)
    // Pantalla principal en píxeles reales (decide qué vídeos son «más grandes que la pantalla»).
    Q_PROPERTY(int screenWidth READ screenWidth CONSTANT)
    Q_PROPERTY(int screenHeight READ screenHeight CONSTANT)
    // Opciones de pausa del plugin de vídeo de Plasma.
    Q_PROPERTY(int pauseMode READ pauseMode NOTIFY videoSettingsChanged)
    Q_PROPERTY(bool batteryPauses READ batteryPauses NOTIFY videoSettingsChanged)
    Q_PROPERTY(int batteryLevel READ batteryLevel NOTIFY videoSettingsChanged)
    Q_PROPERTY(bool videoPluginInstalled READ videoPluginInstalled NOTIFY videoSettingsChanged)
    // Qué hacer al importar vídeos más grandes que la pantalla: ask | 30 | 60 | keep
    Q_PROPERTY(QString largeVideoPolicy READ largeVideoPolicy WRITE setLargeVideoPolicy NOTIFY largeVideoPolicyChanged)
    // Ilustración del selector (HUD): ruta del usuario, nunca empaquetada con la app.
    Q_PROPERTY(QString mascotUrl READ mascotUrl NOTIFY mascotChanged)
    Q_PROPERTY(bool showMascot READ showMascot WRITE setShowMascot NOTIFY mascotChanged)
    Q_PROPERTY(double mascotFocusX READ mascotFocusX NOTIFY mascotChanged)
    Q_PROPERTY(double mascotFocusY READ mascotFocusY NOTIFY mascotChanged)
    // Conversión en curso (barra de progreso).
    Q_PROPERTY(bool taskActive READ taskActive NOTIFY taskChanged)
    Q_PROPERTY(int taskProgress READ taskProgress NOTIFY taskChanged)
    Q_PROPERTY(QString taskText READ taskText NOTIFY taskChanged)

public:
    explicit Bridge(QObject *parent = nullptr);

    QVariantList wallpapers() const { return m_wallpapers; }
    QString currentId() const { return m_currentId; }
    QString backend() const { return m_backend; }
    bool videoSupported() const { return m_video; }
    bool loaded() const { return m_loaded; }
    bool busy() const { return m_busy; }
    bool reducedMotion() const { return m_reducedMotion; }

    Q_INVOKABLE void refresh();
    Q_INVOKABLE void apply(const QString &id);
    Q_INVOKABLE void setFavorite(const QString &id, bool on);
    Q_INVOKABLE QString formatSize(qint64 bytes) const;

    QString dbPath() const { return m_dbPath; }
    QString thumbsDir() const { return m_thumbsDir; }
    int thumbVersion() const { return m_thumbVersion; }
    QString shortcut() const;

    // Rutas locales o URLs file:// (lo que devuelven los diálogos de QML).
    Q_INVOKABLE void importPaths(const QVariantList &paths);
    Q_INVOKABLE void removeWallpaper(const QString &id);
    Q_INVOKABLE void regenerateThumbs();
    Q_INVOKABLE void openHud();
    Q_INVOKABLE void openShortcutSettings();
    Q_INVOKABLE void revealInFolder(const QString &path);

    int screenWidth() const { return m_screenW; }
    int screenHeight() const { return m_screenH; }
    int pauseMode() const { return m_pauseMode; }
    bool batteryPauses() const { return m_batteryPauses; }
    int batteryLevel() const { return m_batteryLevel; }
    bool videoPluginInstalled() const { return m_videoPlugin; }
    QString largeVideoPolicy() const { return m_largePolicy; }
    void setLargeVideoPolicy(const QString &p);
    bool taskActive() const { return !m_queue.isEmpty() || m_taskRunning; }
    int taskProgress() const { return m_taskProgress; }
    QString taskText() const { return m_taskText; }

    // Devuelve la ilustración ya «horneada» para el HUD: recortada al panel derecho, con el fundido
    // y la opacidad incluidos (se genera una vez en ~/.cache/nekowall/). Así el HUD solo dibuja una
    // imagen, sin capas ni máscaras: medido 90 → 63 MiB.
    QString mascotUrl() const;
    bool showMascot() const { return m_showMascot; }
    void setShowMascot(bool on);
    double mascotFocusX() const { return m_mascotFocusX; }
    double mascotFocusY() const { return m_mascotFocusY; }
    // Copia la imagen elegida a ~/.local/share/nekowall/mascota.<ext> (el punto focal vuelve al centro).
    Q_INVOKABLE void setMascotImage(const QVariant &url);

    Q_INVOKABLE void setVideoSettings(int pauseMode, bool batteryPauses, int batteryLevel);
    // Tras el diálogo de vídeos grandes. choice: "30" | "60" | "keep".
    Q_INVOKABLE void importWithChoice(const QVariantList &paths, const QVariantList &videos,
                                      const QString &choice, bool remember);
    // Clic derecho → «Optimizar para mi pantalla…»: analiza y, si conviene, emite optimizeOffer.
    Q_INVOKABLE void requestOptimize(const QString &path);
    Q_INVOKABLE void optimizeVideos(const QVariantList &videos, const QString &choice);

signals:
    // Mensajes para la barra de estado de la ventana principal.
    void notice(const QString &text, bool isError);
    void removed(const QString &id, const QString &path);
    // Se emite justo antes de reemplazar la lista: la UI congela su selección para restaurarla después.
    void wallpapersAboutToChange();
    void wallpapersChanged();
    void statusChanged();
    void busyChanged();
    // ok=false → message contiene el error legible de la CLI.
    void applied(bool ok, const QString &id, const QString &message);
    void error(const QString &message);
    void videoSettingsChanged();
    void largeVideoPolicyChanged();
    void taskChanged();
    void mascotChanged();
    // Vídeos más grandes que la pantalla encontrados al importar (la UI pregunta qué hacer).
    void largeVideosFound(const QVariantList &videos, const QVariantList &paths);
    // Un vídeo de la biblioteca que conviene optimizar (desde el menú contextual).
    void optimizeOffer(const QVariantList &videos);

private:
    using Done = std::function<void(int code, const QByteArray &out, const QByteArray &err)>;
    void run(const QStringList &args, Done done);
    void loadList();
    void loadStatus();
    void generateThumbs();
    void setBusy(bool b);
    void loadVideoSettings();
    void doImport(const QStringList &paths, const QVariantList &toOptimize, const QString &choice);
    void runNextOptimize();
    bool isLarge(const QVariantMap &video) const;
    static QStringList toLocalPaths(const QVariantList &paths);
    static QString findCli();
    static bool readReducedMotion();

    QString m_cli;
    QVariantList m_wallpapers;
    QString m_currentId;
    QString m_backend;
    bool m_video = false;
    bool m_loaded = false;
    bool m_busy = false;
    bool m_thumbsRequested = false;
    bool m_reducedMotion = false;
    QString m_dbPath;
    QString m_thumbsDir;
    int m_thumbVersion = 0;
    int m_screenW = 1920;
    int m_screenH = 1080;
    int m_pauseMode = 0;
    bool m_batteryPauses = true;
    int m_batteryLevel = 10;
    bool m_videoPlugin = false;
    QString m_largePolicy;
    struct Job { QString path; QString name; int fps; };
    QList<Job> m_queue;
    bool m_taskRunning = false;
    int m_taskProgress = 0;
    QString m_taskText;
    int m_jobsDone = 0;
    int m_jobsFailed = 0;
    QString bakedMascot() const;
    QString m_mascotPath;
    bool m_showMascot = true;
    double m_mascotFocusX = 0.5;
    double m_mascotFocusY = 0.5;
};
