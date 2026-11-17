#pragma once
// Puente entre QML y el núcleo Rust. Todo pasa por la CLI `nekowall` (salida --json),
// siempre con QProcess asíncrono: la interfaz nunca espera a Plasma ni a ffmpeg.

#include <functional>

#include <QFileSystemWatcher>
#include <QHash>
#include <QObject>
#include <QTimer>
#include <QString>
#include <QVariantList>
#include <QVariantMap>

class QProcess;

class Bridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList wallpapers READ wallpapers NOTIFY wallpapersChanged)
    // Fondo actual de la pantalla de destino (o el de la principal).
    Q_PROPERTY(QString currentId READ currentId NOTIFY statusChanged)
    // Varias pantallas. target: "" = la pantalla de esta ventana (donde está el ratón), "*" = todas,
    // o el nombre de una pantalla. hostScreen: pantalla en la que está la ventana (lo pone el QML).
    Q_PROPERTY(QVariantList screens READ screens NOTIFY screensChanged)
    Q_PROPERTY(bool multiScreen READ multiScreen NOTIFY screensChanged)
    // Mostrar los botones de pantalla (siempre que el escritorio admita fondos por pantalla, aunque haya una).
    Q_PROPERTY(bool screenPicker READ screenPicker NOTIFY screensChanged)
    Q_PROPERTY(QString target READ target WRITE setTarget NOTIFY targetChanged)
    Q_PROPERTY(QString hostScreen READ hostScreen WRITE setHostScreen NOTIFY targetChanged)
    Q_PROPERTY(QString targetLabel READ targetLabel NOTIFY targetChanged)
    Q_PROPERTY(QString backend READ backend NOTIFY statusChanged)
    Q_PROPERTY(bool videoSupported READ videoSupported NOTIFY statusChanged)
    Q_PROPERTY(bool loaded READ loaded NOTIFY wallpapersChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(bool reducedMotion READ reducedMotion CONSTANT)
    Q_PROPERTY(QString dbPath READ dbPath NOTIFY statusChanged)
    Q_PROPERTY(QString thumbsDir READ thumbsDir NOTIFY statusChanged)
    Q_PROPERTY(int thumbVersion READ thumbVersion NOTIFY wallpapersChanged)
    Q_PROPERTY(QString shortcut READ shortcut CONSTANT)
    // Carpeta propia (Imágenes/nekowallpapers): lo que se copia ahí aparece solo.
    Q_PROPERTY(QString defaultFolder READ defaultFolder NOTIFY defaultFolderChanged)
    // Wayland (layer-shell) o X11 (ventana a pantalla completa, siempre encima).
    Q_PROPERTY(bool wayland READ wayland CONSTANT)
    // El compositor puede desenfocar detrás de la ventana (KWin): HUD de cristal; si no, fondo opaco.
    Q_PROPERTY(bool glass READ glass CONSTANT)
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
    QString defaultFolder() const { return m_defaultFolder; }
    QString currentId() const;
    QVariantList screens() const;
    bool multiScreen() const;
    bool screenPicker() const { return m_perMonitor; }
    // Pantalla elegida (aunque haya una sola); "" = todas.
    QString chosenScreen() const;
    QString target() const { return m_target; }
    void setTarget(const QString &t);
    QString hostScreen() const { return m_hostScreen; }
    void setHostScreen(const QString &s);
    QString targetLabel() const;
    // Pantalla en la que se aplicará: "" = todas.
    QString resolvedScreen() const;
    // Siguiente destino: cada pantalla y luego «todas» (tecla P del selector).
    Q_INVOKABLE void cycleTarget();
    // Índice (en Qt.application.screens) de la pantalla donde está el ratón; -1 si no se sabe.
    Q_INVOKABLE int cursorScreenIndex() const;
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
    bool wayland() const { return m_wayland; }
    bool glass() const { return m_glass; }

    // Rutas locales o URLs file:// (lo que devuelven los diálogos de QML).
    Q_INVOKABLE void importPaths(const QVariantList &paths);
    Q_INVOKABLE void removeWallpaper(const QString &id);
    Q_INVOKABLE void regenerateThumbs();
    Q_INVOKABLE void openHud();
    Q_INVOKABLE void openShortcutSettings();
    // MATE: registra Meta+Alt+W → «nekowall-gui --hud» como atajo personalizado (gsettings, efecto inmediato).
    Q_INVOKABLE void registerMateShortcut();
    Q_INVOKABLE void revealInFolder(const QString &path);
    Q_INVOKABLE void openDefaultFolder();

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
    void defaultFolderChanged();
    void screensChanged();
    void targetChanged();
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
    void syncFolders();
    bool isLarge(const QVariantMap &video) const;
    static QStringList toLocalPaths(const QVariantList &paths);
    static QString findCli();
    static bool readReducedMotion();

    QString m_cli;
    // Carpetas añadidas: se revisan al abrir y, mientras NekoWall está abierto, al cambiar.
    QFileSystemWatcher *m_watcher = nullptr;
    QTimer *m_syncTimer = nullptr;
    bool m_syncRunning = false;
    bool m_syncAgain = false;
    int m_lastMissing = -1;
    QString m_defaultFolder;
    QString m_target;
    QString m_hostScreen;
    bool m_perMonitor = false;
    QHash<QString, QString> m_currentByScreen;   // pantalla → id del fondo
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
    bool m_wayland = false;
    bool m_glass = false;
    QString m_mascotPath;
    bool m_showMascot = true;
    double m_mascotFocusX = 0.5;
    double m_mascotFocusY = 0.5;
};
