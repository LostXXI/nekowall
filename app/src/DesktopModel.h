// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Iconos del escritorio propios de NekoWall (Wayland: Wayfire, sway, niri…): el contenido de la carpeta
// Escritorio en una capa transparente ENCIMA del vídeo de fondo.
// (El escritorio de pcmanfm-qt pinta un fondo opaco: con él, o se ven los iconos o se ve el vídeo.)
//
// Imita al escritorio de pcmanfm-qt lo más fielmente posible: lee y escribe SUS archivos
// (~/.config/pcmanfm-qt/<perfil>/settings.conf y desktop-items-0.conf), así que tamaño de iconos, letra,
// colores, márgenes, orden, archivos ocultos, clic simple… y la POSICIÓN de cada icono son las mismas
// que con pcmanfm-qt, y se puede volver a él sin perder nada. La rejilla se calcula como libfm-qt
// (FolderView::updateGridSize + DesktopWindow::removeBottomGap / relayoutItems / alignToGrid).
#pragma once

#include <QAbstractListModel>
#include <QColor>
#include <QFileSystemWatcher>
#include <QFont>
#include <QHash>
#include <QImage>
#include <QPoint>
#include <QQuickImageProvider>
#include <QRect>
#include <QSize>
#include <QTimer>
#include <QUrl>
#include <QVariantList>
#include <memory>

#ifdef NEKOWALL_CAJA
#include "CajaScan.h"
#endif

// «image://icono/<nombre>»: icono del tema del sistema (QIcon::fromTheme), con alternativas.
class ThemeIconProvider : public QQuickImageProvider
{
public:
    ThemeIconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) {}
    QPixmap requestPixmap(const QString &id, QSize *size, const QSize &requested) override;
};

// «image://etiqueta/<fila>/<seleccionado>/…»: el nombre de un icono dibujado con Pango (modo Caja).
class DesktopModel;
class CajaLabelProvider : public QQuickImageProvider
{
public:
    explicit CajaLabelProvider(DesktopModel *m) : QQuickImageProvider(QQuickImageProvider::Image), m_model(m) {}
    QImage requestImage(const QString &id, QSize *size, const QSize &requested) override;
private:
    DesktopModel *m_model;
};

class DesktopModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(QString folder READ folder CONSTANT)
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(int selectedCount READ selectedCount NOTIFY selectionChanged)
    Q_PROPERTY(int current READ current NOTIFY selectionChanged)
    // Rejilla (como libfm-qt)
    Q_PROPERTY(int cellWidth READ cellWidth NOTIFY geometryChanged)
    Q_PROPERTY(int cellHeight READ cellHeight NOTIFY geometryChanged)
    Q_PROPERTY(int marginWidth READ marginWidth NOTIFY geometryChanged)
    Q_PROPERTY(int marginHeight READ marginHeight NOTIFY geometryChanged)
    Q_PROPERTY(QRect workArea READ workArea NOTIFY geometryChanged)
    // Ajustes de pcmanfm-qt
    Q_PROPERTY(int iconSize READ iconSize NOTIFY settingsChanged)
    Q_PROPERTY(QFont font READ font NOTIFY settingsChanged)
    Q_PROPERTY(QColor fgColor READ fgColor NOTIFY settingsChanged)
    Q_PROPERTY(QColor shadowColor READ shadowColor NOTIFY settingsChanged)
    Q_PROPERTY(bool singleClick READ singleClick NOTIFY settingsChanged)
    Q_PROPERTY(bool hideItems READ hideItems NOTIFY settingsChanged)
    Q_PROPERTY(bool showHidden READ showHidden NOTIFY settingsChanged)
    Q_PROPERTY(QString sortColumn READ sortColumn NOTIFY settingsChanged)
    Q_PROPERTY(bool sortAscending READ sortAscending NOTIFY settingsChanged)
    Q_PROPERTY(bool folderFirst READ folderFirst NOTIFY settingsChanged)
    Q_PROPERTY(bool hiddenLast READ hiddenLast NOTIFY settingsChanged)
    Q_PROPERTY(bool allSticky READ allSticky NOTIFY settingsChanged)
    Q_PROPERTY(bool tooltips READ tooltips NOTIFY settingsChanged)
    Q_PROPERTY(bool confirmTrash READ confirmTrash NOTIFY settingsChanged)
    Q_PROPERTY(bool confirmDelete READ confirmDelete NOTIFY settingsChanged)
    Q_PROPERTY(bool useTrash READ useTrash NOTIFY settingsChanged)
    Q_PROPERTY(QStringList shortcuts READ shortcuts NOTIFY settingsChanged)
    Q_PROPERTY(QVariantMap prefs READ prefs NOTIFY settingsChanged)
    Q_PROPERTY(QString settingsFile READ settingsFile CONSTANT)
    // «pcmanfm» (Wayland, imita a pcmanfm-qt) o «caja» (MATE, imita al escritorio de Caja)
    Q_PROPERTY(QString flavor READ flavor CONSTANT)
    Q_PROPERTY(bool keepAligned READ keepAligned NOTIFY settingsChanged)
    Q_PROPERTY(bool lockPositions READ lockPositions NOTIFY settingsChanged)

public:
    enum Roles {
        NameRole = Qt::UserRole + 1, FileNameRole, PathRole, IconRole, ThumbRole, IsDirRole, IsLauncherRole,
        XRole, YRole, SelectedRole, StickyRole, CutRole, IsTrashRole, HiddenRole,
        // Geometría por icono (modo Caja: cada icono tiene su propio alto y ancho de nombre)
        BoxXRole, BoxYRole, BoxWRole, BoxHRole, IconXRole, IconYRole, IconPxRole,
        TextXRole, TextYRole, TextWRole, TextHRole, EditableHRole, AdditionalRole, SpecialRole
    };
    enum Flavor { Pcmanfm, Caja };
    // Un modelo por pantalla, como pcmanfm-qt 2.5 en Wayland (posiciones en desktop-items-<pantalla>.conf).
    explicit DesktopModel(const QString &screenName = {}, QObject *parent = nullptr, Flavor flavor = Pcmanfm);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    QString folder() const { return m_folder; }
    int selectedCount() const;
    int current() const { return m_current; }
    int cellWidth() const { return m_grid.width(); }
    int cellHeight() const { return m_grid.height(); }
    int marginWidth() const { return m_margins.width(); }
    int marginHeight() const { return m_margins.height(); }
    QRect workArea() const { return m_workArea; }
    int iconSize() const { return m_iconSize; }
    QFont font() const { return m_font; }
    QColor fgColor() const { return m_fg; }
    QColor shadowColor() const { return m_shadow; }
    bool singleClick() const { return m_singleClick; }
    bool hideItems() const { return m_hideItems; }
    bool showHidden() const { return m_showHidden; }
    QString sortColumn() const { return m_sortColumn; }
    bool sortAscending() const { return m_sortAscending; }
    bool folderFirst() const { return m_folderFirst; }
    bool hiddenLast() const { return m_hiddenLast; }
    bool allSticky() const { return m_allSticky; }
    bool tooltips() const { return !m_noTooltip; }
    bool confirmTrash() const { return m_confirmTrash; }
    bool confirmDelete() const { return m_confirmDelete; }
    bool useTrash() const { return m_useTrash; }
    QStringList shortcuts() const { return m_shortcuts; }
    QVariantMap prefs() const;
    QString settingsFile() const { return m_settingsFile; }
    QString flavor() const { return m_flavor == Caja ? QStringLiteral("caja") : QStringLiteral("pcmanfm"); }
    bool keepAligned() const { return m_keepAligned; }
    bool lockPositions() const { return m_lockPositions; }
    // Texto de un menú traducido como en Caja (su dominio de traducción), o el mismo texto.
    Q_INVOKABLE QString cajaText(const QString &msgid, const QString &domain = QStringLiteral("caja")) const;
    // Modo Caja: «Organizar el escritorio por nombre», «Mantener alineado».
    Q_INVOKABLE void organizeByName();
    Q_INVOKABLE void setKeepAligned(bool on);
    // Modo Caja: guardar en los metadatos de Caja dónde tiene Caja ahora cada icono (antes de reemplazarlo).
    Q_INVOKABLE int captureCajaPositions();
    void setSaveNewPositions(bool on) { m_saveNew = on; }   // falso solo para --capture-caja
    // Modo Caja: el nombre dibujado con Pango (lo pide el proveedor de imágenes «image://etiqueta/…»).
    QImage cajaLabelImage(int row, bool selected) const;

    // Tamaño de la capa (lo da QML).
    Q_INVOKABLE void setViewSize(int w, int h);

    // Selección
    Q_INVOKABLE void select(int row, int mode);          // 0 solo este · 1 alternar (Ctrl) · 2 rango (Mayús)
    Q_INVOKABLE void clearSelection();
    Q_INVOKABLE void selectAll();
    Q_INVOKABLE void invertSelection();
    Q_INVOKABLE void beginRubberBand(bool additive);
    Q_INVOKABLE void rubberBand(qreal x, qreal y, qreal w, qreal h);
    Q_INVOKABLE int navigate(int key, bool extend);      // flechas, Inicio, Fin, RePág, AvPág
    Q_INVOKABLE int rowAt(qreal x, qreal y) const;       // icono bajo un punto (-1 si ninguno)
    Q_INVOKABLE QPoint cellFor(qreal x, qreal y) const;  // celda de la rejilla para un punto

    // Abrir
    Q_INVOKABLE void open(int row);
    Q_INVOKABLE void openSelected();
    Q_INVOKABLE QVariantList appsFor(int row);          // [{id, name, icon, isDefault}]
    Q_INVOKABLE void openWith(int row, const QString &appId);
    Q_INVOKABLE void reveal(int row);
    Q_INVOKABLE void openFolder();
    Q_INVOKABLE void openTerminal();
    Q_INVOKABLE void openNekoWall();

    // Archivos
    Q_INVOKABLE void trashSelected();
    Q_INVOKABLE void deleteSelected();
    Q_INVOKABLE QString renameItem(int row, const QString &newName);   // "" si fue bien; si no, el error
    Q_INVOKABLE int createFolder();                      // fila nueva (para renombrarla) o -1
    Q_INVOKABLE int createFile();
    Q_INVOKABLE bool createLauncher(const QString &name, const QString &command, const QString &icon, bool terminal);
    Q_INVOKABLE void copySelected(bool cut);
    Q_INVOKABLE void copyPaths();
    Q_INVOKABLE bool canPaste() const;
    Q_INVOKABLE void paste(int intoRow = -1);
    Q_INVOKABLE void dropUrls(const QStringList &uris, qreal x, qreal y, bool copy);
    Q_INVOKABLE void moveSelection(qreal dx, qreal dy);  // arrastre: nuevas posiciones fijas
    Q_INVOKABLE void dropSelectionOn(int row);           // arrastre sobre una carpeta o la papelera
    Q_INVOKABLE bool isDropTarget(int row) const;        // carpeta o papelera no seleccionada
    Q_INVOKABLE bool isSticky(int row) const;
    Q_INVOKABLE void setStickySelected(bool on);
    Q_INVOKABLE QVariantMap properties(int row) const;
    Q_INVOKABLE int trashCount() const;
    Q_INVOKABLE void emptyTrash();

    // Ajustes (se guardan en el settings.conf de pcmanfm-qt)
    Q_INVOKABLE void setSort(const QString &column);
    Q_INVOKABLE void setSortAscending(bool on);
    Q_INVOKABLE void setFolderFirst(bool on);
    Q_INVOKABLE void setHiddenLast(bool on);
    Q_INVOKABLE void setShowHidden(bool on);
    Q_INVOKABLE void setHideItems(bool on);
    Q_INVOKABLE void savePrefs(const QVariantMap &p);
    Q_INVOKABLE QStringList fontFamilies() const;

public slots:
    void cajaSettingsChanged();          // GSettings de MATE cambió (modo Caja)

signals:
    void countChanged();
    void selectionChanged();
    void geometryChanged();
    void settingsChanged();
    void message(const QString &text);

private:
    struct Item {
        QString name, fileName, path, icon, thumb, exec, url, mime;
        bool dir = false, launcher = false, hidden = false, terminal = false, selected = false, cut = false;
        qint64 size = 0;
        qint64 mtime = 0;
        QPoint pos;
        // Modo Caja
        bool special = false, placed = false;
        double cx = 0, cy = 0;                     // posición en el lienzo de Caja (como icon->x/y)
        QString additional;                        // pies de icono
#ifdef NEKOWALL_CAJA
        CajaIconGeom geom;                         // medidas del nombre (para dibujarlo)
#endif
        QRect box, iconRect, textRect;             // en píxeles: recuadro (ventana) e icono/nombre (relativos al recuadro)
        int editableH = 0;
    };
    void reload();
    void loadSettings();
    void writeSetting(const QString &key, const QVariant &value);
    void loadPositions();
    void savePositions();
    void relayout();
    void computeGrid();
    void ensureShortcuts();
    void updateTrashShortcut();
    void sortItems(QList<Item> &items) const;
    void alignToGrid(QPoint &p) const;
    QRect itemRect(const Item &it) const;
    QList<int> selectedRows() const;
    void emitSelection();
    void launch(const QString &exec, const QString &file, bool terminal, const QString &what);
    void transfer(const QStringList &sources, const QString &destDir, bool move, const QPoint &dropAt);
    QString freeName(const QString &dir, const QString &name) const;
    static Item readLauncher(const QString &path);
    // Modo Caja (DesktopModelCaja.cpp)
    void cajaLoadSettings();
    void cajaReload();
    void cajaRelayout(bool firstLoad);
    void cajaMove(qreal dx, qreal dy);
    void cajaSavePosition(const Item &it);
    QRect cajaHitRect(const Item &it) const;

    QString m_screenName;
    QString m_folder, m_profileDir, m_settingsFile, m_positionsFile;
    QList<Item> m_items;
    QFileSystemWatcher m_watcher;
    QTimer m_reloadTimer;
    QSize m_view;
    QSize m_grid{80, 80};
    QSize m_margins{3, 1};
    QRect m_workArea;
    QStringList m_fileOrder;                // orden de desktop-items-*.conf al cargarlo (como lo inserta pcmanfm-qt)
    QHash<QString, QPoint> m_customStore;   // posiciones fijas (como customPosStorage_ de pcmanfm-qt)
    QHash<QString, QPoint> m_pending;       // archivos que van a aparecer en una posición (soltar/pegar)
    QList<bool> m_rubberBase;
    int m_current = -1, m_anchor = -1;

    // Ajustes
    int m_iconSize = 48;
    QSize m_baseMargins{3, 1};
    QMargins m_workMargins{12, 12, 12, 12};
    QFont m_font;
    QColor m_fg{Qt::white}, m_shadow{Qt::black};
    bool m_singleClick = false, m_hideItems = false, m_showHidden = false, m_sortAscending = true;
    bool m_folderFirst = true, m_hiddenLast = false, m_allSticky = false, m_noTooltip = false;
    bool m_confirmTrash = false, m_confirmDelete = true, m_useTrash = true, m_thumbnails = true;
    bool m_backupAsHidden = false, m_defaultFm = false;
    QString m_sortColumn = QStringLiteral("name");
    QStringList m_shortcuts;

    Flavor m_flavor = Pcmanfm;
    bool m_saveNew = true;
    bool m_keepAligned = true, m_lockPositions = false, m_cajaFirstLoad = true;
#ifdef NEKOWALL_CAJA
    CajaSettings m_caja;
    std::unique_ptr<CajaLayout> m_place, m_show;   // colocar (como Caja al arrancar) y mostrar (ajustes del escritorio)
    QPoint m_origin;                               // márgenes del área de trabajo (paneles)
    double m_ppu = 1.0;
#endif
};
