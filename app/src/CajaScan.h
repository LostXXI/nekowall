// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// El escritorio de Caja (MATE) tal como lo ve Caja: iconos especiales (Equipo, Carpeta personal, Red,
// Papelera) con sus nombres traducidos por Caja, los archivos de la carpeta Escritorio, su orden
// (fm-icon-container.c) y sus posiciones guardadas (metadatos GVfs «caja-icon-position» y, para los
// especiales, ~/.config/caja/desktop-metadata).
#pragma once

#include "CajaLayout.h"

#include <QList>
#include <QString>
#include <QStringList>

struct CajaSettings {
    CajaFontOptions font;
    QString iconTheme;                                    // org.mate.interface icon-theme
    QString interfaceFont = QStringLiteral("Sans 10");   // org.mate.interface font-name: con ella COLOCA Caja al arrancar
    int placeEllipsisLimit = 3;   // org.mate.caja.icon-view text-ellipsis-limit (para su zoom): el que usa al COLOCAR
    int ellipsisLimit = 3;
    int zoomSize = 48;                 // org.mate.caja.icon-view default-zoom-level → tamaño de icono
    bool computerVisible = true, homeVisible = true, trashVisible = true, networkVisible = false;
    bool volumesVisible = true;
    bool directoriesFirst = true;
    bool singleClick = false;
    bool showHidden = false;
    bool confirmTrash = true;
    QString thumbnails = QStringLiteral("local-only");
    QString showIconText = QStringLiteral("local-only");   // org.mate.caja.preferences show-icon-text
    bool keepAligned = true;
    QStringList captions{QStringLiteral("none"), QStringLiteral("size"), QStringLiteral("date_modified")};  // icon-view captions
    QString dateFormat = QStringLiteral("locale");
    bool iecUnits = false;
    // Pies de icono que se muestran con un tamaño de icono (pieces_by_level de fm-icon-container.c).
    static int captionPieces(int zoomSize);
    static CajaSettings load();        // GSettings de MATE (si no existen los esquemas, valores por defecto)
};

struct CajaEntry {
    QString key;          // especiales: computer/home/trash/network; archivos: nombre del archivo
    QString path;         // archivo (vacío en los especiales)
    QString uri;          // a abrir (computer:///, trash:///, ~)
    QString display;      // nombre que muestra Caja
    QString iconName;     // icono del tema (varios nombres separados por «|», en el orden de GIO)
    QString thumb;        // miniatura (imágenes), si Caja la mostraría
    bool special = false, dir = false, launcher = false, hidden = false;
    int category = 4;     // SORT_COMPUTER_LINK 0, HOME 1, NETWORK 2, MOUNT 3, OTHER 4, TRASH 5
    QByteArray collation; // g_utf8_collate_key_for_filename del nombre
    bool sortLast = false;
    bool positioned = false;
    int x = 0, y = 0;
    QStringList captionTexts;   // uno por hueco de «captions» (vacío = none); especiales y lanzadores: ninguno
    QString additionalFor(int pieces) const;
};

namespace CajaScan {
QString desktopDir();
QList<CajaEntry> entries(const CajaSettings &s);   // ya en el orden de Caja
void sortEntries(QList<CajaEntry> &list, bool directoriesFirst);
bool readPosition(const CajaEntry &e, int &x, int &y);
bool writePosition(const CajaEntry &e, int x, int y);
bool clearPosition(const CajaEntry &e);
QString trName(const char *msgid);                // traducción del dominio «caja»
double xsettingsDpi();                             // Xft/DPI de XSETTINGS (la resolución exacta que usa GTK), 0 si no hay
}
