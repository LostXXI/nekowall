// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Colocación de iconos del escritorio idéntica a la de Caja (MATE): traducción de
// caja-icon-container.c / caja-icon-canvas-item.c (Caja 1.28). Los nombres se miden con Pango, como
// Caja, para que la altura de cada icono (y por tanto cada columna) salga igual.
#pragma once

#include <QColor>
#include <QImage>
#include <QList>
#include <QString>

struct CajaFontOptions {
    QString font = QStringLiteral("Sans 10");   // org.mate.caja.desktop font
    double dpi = 96;                             // Xft.dpi
    QString antialias = QStringLiteral("rgba");  // org.mate.font-rendering antialiasing
    QString hinting = QStringLiteral("slight");  // org.mate.font-rendering hinting
    QString rgbaOrder = QStringLiteral("rgb");
    int hintMetrics = -1;                        // -1 por defecto de cairo, 0 apagado, 1 encendido (calibración)
    int roundPositions = -1;                     // -1 por defecto de Pango, 0/1 (calibración)
};

// Rectángulo entero como EelIRect de Caja (x1/y1 excluidos).
struct CajaRect {
    int x0 = 0, y0 = 0, x1 = 0, y1 = 0;
};

struct CajaIconGeom {
    // Medidas del nombre (como measure_label_text, ya con los márgenes de Caja).
    int textWidth = 0, textHeight = 0, textHeightForLayout = 0, textHeightForEntire = 0;
    int textDx = 0;
    int editableHeight = 0;   // alto del nombre solo (el resto son pies de icono)
    int lines = 0;
    // Recuadros relativos al origen del icono (como bounds_cache_*).
    CajaRect total, totalForLayout, totalForEntire;
    CajaRect text;                                 // recuadro del nombre (para dibujar)
    int iconW = 48, iconH = 48;
};

struct CajaIcon {
    QString name;          // nombre mostrado
    QString additional;    // pies de icono (tamaño, fecha…) separados por «\n»; vacío si no hay
    int iconW = 48, iconH = 48;
    bool positioned = false;
    double x = 0, y = 0;   // coordenadas del lienzo (como icon->x / icon->y)
    CajaIconGeom g;
};

class CajaLayout
{
public:
    explicit CajaLayout(const CajaFontOptions &opt);
    ~CajaLayout();
    CajaLayout(const CajaLayout &) = delete;
    CajaLayout &operator=(const CajaLayout &) = delete;

    int ellipsisLimit = 3;        // org.mate.caja.desktop text-ellipsis-limit
    double pixelsPerUnit = 1.0;   // tamaño del zoom / 48
    bool tighter = false, keepAligned = true;
    int canvasWidth = 0, canvasHeight = 0;   // pantalla menos los márgenes del área de trabajo
    // Comprobado contra Caja 1.28: al arrancar añade los iconos en varias tandas, así que en la práctica coloca
    // TODOS con find_empty_location (primer hueco libre desde la primera columna), no por columnas.
    bool alwaysFindEmpty = true;

    CajaIconGeom measure(const QString &name, int iconW, int iconH, const QString &additional = {}) const;
    // Coloca los iconos (en el orden dado) como finish_adding_new_icons + lay_down_icons_vertical_desktop.
    void layout(QList<CajaIcon> &icons) const;
    // Aplica una posición con el recorte de icon_set_position_full (escritorio).
    void setPosition(CajaIcon &icon, double x, double y) const;
    // Para arrastrar con «mantener alineado»: snap_position.
    void snapPosition(const CajaIcon &icon, int &x, int &y) const;
    // Primer hueco libre a partir de (x, y), como find_empty_location (grid «no apretado»).
    void findEmptyLocation(const QList<CajaIcon> &placed, const CajaIcon &icon, int startX, int startY, int &x, int &y) const;
    // El nombre (y los pies de icono) dibujados con Pango, como Caja: mismos cortes, «…» y suavizado.
    QImage renderLabel(const QString &name, const QString &additional, const CajaIconGeom &g,
                       const QColor &fg, const QColor &shadow) const;
    CajaRect boundsForLayout(const CajaIcon &icon) const;
    CajaRect boundsForEntire(const CajaIcon &icon) const;

private:
    struct Impl;
    Impl *d;
};
