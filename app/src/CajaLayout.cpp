// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
#include "CajaLayout.h"

#include <cairo.h>
#include <cctype>
#include <climits>
#include <cmath>
#pragma push_macro("signals")
#undef signals
#include <pango/pangocairo.h>
#pragma pop_macro("signals")

// Constantes de caja-icon-container.c / caja-icon-canvas-item.c (Caja 1.28).
static constexpr int DESKTOP_PAD_HORIZONTAL = 10;
static constexpr int DESKTOP_PAD_VERTICAL = 10;
static constexpr int SNAP_SIZE_X = 78;
static constexpr int SNAP_SIZE_Y = 20;
static constexpr int MAX_TEXT_WIDTH_STANDARD = 135;
static constexpr int MAX_TEXT_WIDTH_TIGHTER = 80;
static constexpr int LABEL_OFFSET = 1;
static constexpr int TEXT_BACK_PADDING_X = 4;
static constexpr int TEXT_BACK_PADDING_Y = 1;

static double eelRound(double v) { return std::floor(v + 0.5); }
static double snapH(double (*f)(double), double x)
{
    return f((x - DESKTOP_PAD_HORIZONTAL) / SNAP_SIZE_X) * SNAP_SIZE_X + DESKTOP_PAD_HORIZONTAL;
}
static double snapV(double (*f)(double), double y)
{
    return f((y - DESKTOP_PAD_VERTICAL) / SNAP_SIZE_Y) * SNAP_SIZE_Y + DESKTOP_PAD_VERTICAL;
}
static double ceilD(double v) { return std::ceil(v); }
template<typename T> static T clampG(T x, T lo, T hi) { return x > hi ? hi : (x < lo ? lo : x); }   // CLAMP de GLib

static CajaRect unionR(const CajaRect &a, const CajaRect &b)
{
    auto empty = [](const CajaRect &r) { return r.x0 >= r.x1 || r.y0 >= r.y1; };
    if (empty(a)) return b;
    if (empty(b)) return a;
    return {std::min(a.x0, b.x0), std::min(a.y0, b.y0), std::max(a.x1, b.x1), std::max(a.y1, b.y1)};
}

struct CajaLayout::Impl {
    PangoContext *ctx = nullptr;
    PangoFontDescription *desc = nullptr;
};

CajaLayout::CajaLayout(const CajaFontOptions &opt)
    : d(new Impl)
{
    d->ctx = pango_font_map_create_context(pango_cairo_font_map_get_default());
    cairo_font_options_t *fo = cairo_font_options_create();
    if (opt.antialias == QStringLiteral("none"))
        cairo_font_options_set_antialias(fo, CAIRO_ANTIALIAS_NONE);
    else if (opt.antialias == QStringLiteral("grayscale"))
        cairo_font_options_set_antialias(fo, CAIRO_ANTIALIAS_GRAY);
    else
        cairo_font_options_set_antialias(fo, CAIRO_ANTIALIAS_SUBPIXEL);
    const QString h = opt.hinting;
    cairo_font_options_set_hint_style(fo, h == QStringLiteral("none") ? CAIRO_HINT_STYLE_NONE
                                        : h == QStringLiteral("slight") ? CAIRO_HINT_STYLE_SLIGHT
                                        : h == QStringLiteral("medium") ? CAIRO_HINT_STYLE_MEDIUM
                                                                        : CAIRO_HINT_STYLE_FULL);
    const QString o = opt.rgbaOrder;
    cairo_font_options_set_subpixel_order(fo, o == QStringLiteral("bgr") ? CAIRO_SUBPIXEL_ORDER_BGR
                                            : o == QStringLiteral("vrgb") ? CAIRO_SUBPIXEL_ORDER_VRGB
                                            : o == QStringLiteral("vbgr") ? CAIRO_SUBPIXEL_ORDER_VBGR
                                                                          : CAIRO_SUBPIXEL_ORDER_RGB);
    if (opt.hintMetrics == 0)
        cairo_font_options_set_hint_metrics(fo, CAIRO_HINT_METRICS_OFF);
    else if (opt.hintMetrics == 1)
        cairo_font_options_set_hint_metrics(fo, CAIRO_HINT_METRICS_ON);
    pango_cairo_context_set_font_options(d->ctx, fo);
    cairo_font_options_destroy(fo);
    pango_cairo_context_set_resolution(d->ctx, opt.dpi);
    if (opt.roundPositions >= 0)
        pango_context_set_round_glyph_positions(d->ctx, opt.roundPositions == 1);
    d->desc = pango_font_description_from_string(opt.font.toUtf8().constData());
}

CajaLayout::~CajaLayout()
{
    pango_font_description_free(d->desc);
    g_object_unref(d->ctx);
    delete d;
}

// layout_get_full_size de Caja.
static void fullSize(PangoLayout *layout, int *w, int *h, int *dx)
{
    PangoRectangle logical;
    pango_layout_get_extents(layout, nullptr, &logical);
    const int theWidth = (logical.width + PANGO_SCALE / 2) / PANGO_SCALE;
    const int total = (logical.x + logical.width + PANGO_SCALE / 2) / PANGO_SCALE;
    if (w) *w = theWidth;
    if (h) *h = (logical.height + PANGO_SCALE / 2) / PANGO_SCALE;
    if (dx) *dx = total - theWidth;
}

CajaIconGeom CajaLayout::measure(const QString &name, int iconW, int iconH, const QString &additional) const
{
    CajaIconGeom g;
    g.iconW = iconW;
    g.iconH = iconH;
    const double ppu = pixelsPerUnit;
    // + 1e-9: 135 × 32/48 debe dar 90 (como en Caja), no 89,999… (lo decide un píxel al cortar el nombre).
    const double maxTextWidth = (tighter ? MAX_TEXT_WIDTH_TIGHTER : MAX_TEXT_WIDTH_STANDARD) * ppu + 1e-9;

    // create_label_layout: espacio de ancho cero tras '_', '-' y '.' (salvo en versiones tipo 1.2.3).
    auto createLayout = [&](const QString &text) {
        const QByteArray utf8 = text.toUtf8();
        QByteArray z;
        for (int i = 0; i < utf8.size(); ++i) {
            const char c = utf8.at(i);
            z.append(c);
            auto digit = [&](int k) { return k < utf8.size() && std::isdigit(uchar(utf8.at(k))); };
            const bool zeroOrThree = !digit(i + 1) || (digit(i + 2) && digit(i + 3));
            if (c == '_' || c == '-' || (c == '.' && zeroOrThree))
                z.append("\xE2\x80\x8B");
        }
        PangoLayout *l = pango_layout_new(d->ctx);
        pango_layout_set_text(l, z.constData(), -1);
        pango_layout_set_auto_dir(l, FALSE);
        pango_layout_set_alignment(l, PANGO_ALIGN_CENTER);
        pango_layout_set_spacing(l, 0);
        pango_layout_set_wrap(l, PANGO_WRAP_WORD_CHAR);
        PangoAttrList *attrs = pango_attr_list_new();
        pango_attr_list_insert(attrs, pango_attr_insert_hyphens_new(FALSE));
        pango_layout_set_attributes(l, attrs);
        pango_attr_list_unref(attrs);
        pango_layout_set_font_description(l, d->desc);
        return l;
    };
    PangoLayout *layout = createLayout(name);

    auto prepareWidthOf = [&](PangoLayout *l) {
        pango_layout_set_width(l, int(std::floor(maxTextWidth)) * PANGO_SCALE);
        pango_layout_set_ellipsize(l, PANGO_ELLIPSIZE_END);
    };
    auto prepareWidth = [&] { prepareWidthOf(layout); };
    // Texto entero.
    prepareWidth();
    pango_layout_set_height(layout, G_MININT);
    int hEntire = 0;
    fullSize(layout, nullptr, &hEntire, nullptr);
    const int limit = ellipsisLimit > 0 ? ellipsisLimit : INT_MAX;
    g.lines = pango_layout_get_line_count(layout);
    int hLayout = 0;
    if (g.lines <= limit) {
        hLayout = hEntire;
    } else {
        PangoLayoutIter *it = pango_layout_get_iter(layout);
        for (int i = 0; i < limit; ++i) {
            PangoRectangle r;
            pango_layout_iter_get_line_extents(it, nullptr, &r);
            hLayout += (r.height + PANGO_SCALE / 2) / PANGO_SCALE;
            if (!pango_layout_iter_next_line(it))
                break;
            hLayout += pango_layout_get_spacing(layout);
        }
        pango_layout_iter_free(it);
    }
    // Lo que se dibuja (recortado a «limit» líneas con «…»).
    prepareWidth();
    pango_layout_set_height(layout, ellipsisLimit > 0 ? -ellipsisLimit : G_MININT);
    int w = 0, h = 0, dx = 0;
    fullSize(layout, &w, &h, &dx);
    g_object_unref(layout);
    // Pies de icono (additional_text): se dibujan debajo, con las mismas reglas que el nombre.
    int aw = 0, ah = 0, adx = 0;
    if (!additional.isEmpty()) {
        PangoLayout *al = createLayout(additional);
        prepareWidthOf(al);
        pango_layout_set_height(al, ellipsisLimit > 0 ? -ellipsisLimit : G_MININT);
        fullSize(al, &aw, &ah, &adx);
        g_object_unref(al);
    }
    g.editableHeight = h + TEXT_BACK_PADDING_Y * 2;
    if (w > aw) {
        g.textWidth = w;
        g.textDx = dx;
    } else {
        g.textWidth = aw;
        g.textDx = adx;
    }
    g.textWidth += TEXT_BACK_PADDING_X * 2;
    g.textHeight = h + ah + TEXT_BACK_PADDING_Y * 2;
    g.textHeightForLayout = hLayout + ah + TEXT_BACK_PADDING_Y * 2;
    g.textHeightForEntire = hEntire + ah + TEXT_BACK_PADDING_Y * 2;

    // caja_icon_canvas_item_ensure_bounds_up_to_date + compute_text_rectangle (nombre debajo del icono).
    CajaRect icon;
    icon.x1 = int(iconW / ppu);
    icon.y1 = int(iconH / ppu);
    auto textRect = [&](int height) {
        const double tw = g.textWidth / ppu;
        CajaRect t;
        t.x0 = (icon.x0 + icon.x1) / 2 - int(tw) / 2;
        t.y0 = icon.y1;
        t.x1 = int(t.x0 + tw);
        t.y1 = int(t.y0 + height / ppu + LABEL_OFFSET / ppu);
        return t;
    };
    g.text = textRect(g.textHeight);
    g.total = unionR(icon, g.text);
    g.totalForLayout = unionR(icon, textRect(g.textHeightForLayout));
    g.totalForEntire = unionR(icon, textRect(g.textHeightForEntire));
    return g;
}

QImage CajaLayout::renderLabel(const QString &name, const QString &additional, const CajaIconGeom &g,
                               const QColor &fg, const QColor &shadow) const
{
    const int w = std::max(1, g.textWidth), h = std::max(1, g.textHeight + 1);
    QImage img(w, h, QImage::Format_ARGB32_Premultiplied);
    img.fill(Qt::transparent);
    cairo_surface_t *surf = cairo_image_surface_create_for_data(img.bits(), CAIRO_FORMAT_ARGB32, w, h, int(img.bytesPerLine()));
    cairo_t *cr = cairo_create(surf);
    const double maxTextWidth = (tighter ? MAX_TEXT_WIDTH_TIGHTER : MAX_TEXT_WIDTH_STANDARD) * pixelsPerUnit + 1e-9;
    auto draw = [&](const QString &text, double y) {
        const QByteArray utf8 = text.toUtf8();
        QByteArray z;
        for (int i = 0; i < utf8.size(); ++i) {
            const char c = utf8.at(i);
            z.append(c);
            auto digit = [&](int k) { return k < utf8.size() && std::isdigit(uchar(utf8.at(k))); };
            const bool zeroOrThree = !digit(i + 1) || (digit(i + 2) && digit(i + 3));
            if (c == '_' || c == '-' || (c == '.' && zeroOrThree))
                z.append("\xE2\x80\x8B");
        }
        PangoLayout *l = pango_cairo_create_layout(cr);
        pango_layout_set_text(l, z.constData(), -1);
        pango_layout_set_auto_dir(l, FALSE);
        pango_layout_set_alignment(l, PANGO_ALIGN_CENTER);
        pango_layout_set_wrap(l, PANGO_WRAP_WORD_CHAR);
        PangoAttrList *attrs = pango_attr_list_new();
        pango_attr_list_insert(attrs, pango_attr_insert_hyphens_new(FALSE));
        pango_layout_set_attributes(l, attrs);
        pango_attr_list_unref(attrs);
        pango_layout_set_font_description(l, d->desc);
        pango_cairo_context_set_resolution(pango_layout_get_context(l), pango_cairo_context_get_resolution(d->ctx));
        pango_cairo_context_set_font_options(pango_layout_get_context(l), pango_cairo_context_get_font_options(d->ctx));
        pango_layout_context_changed(l);
        pango_layout_set_width(l, int(std::floor(maxTextWidth)) * PANGO_SCALE);
        pango_layout_set_ellipsize(l, PANGO_ELLIPSIZE_END);
        pango_layout_set_height(l, ellipsisLimit > 0 ? -ellipsisLimit : G_MININT);
        // Centrado sobre el recuadro del nombre (el ancho de Pango es el máximo, el recuadro el real).
        // draw_label_text: x = x0 + (ancho - max_text_width) / 2, división entera de C.
        const int x = (w - int(std::floor(maxTextWidth))) / 2;
        if (shadow.alpha() > 0) {
            cairo_set_source_rgba(cr, shadow.redF(), shadow.greenF(), shadow.blueF(), shadow.alphaF());
            cairo_move_to(cr, x + 1, y + 1);
            pango_cairo_show_layout(cr, l);
        }
        cairo_set_source_rgba(cr, fg.redF(), fg.greenF(), fg.blueF(), fg.alphaF());
        cairo_move_to(cr, x, y);
        pango_cairo_show_layout(cr, l);
        g_object_unref(l);
    };
    draw(name, TEXT_BACK_PADDING_Y);
    if (!additional.isEmpty())
        draw(additional, g.editableHeight - TEXT_BACK_PADDING_Y);
    cairo_destroy(cr);
    cairo_surface_destroy(surf);
    return img;
}

CajaRect CajaLayout::boundsForLayout(const CajaIcon &i) const
{
    const CajaRect &t = i.g.totalForLayout;
    return {int(i.x) + t.x0, int(i.y) + t.y0, int(i.x) + t.x1 + 1, int(i.y) + t.y1 + 1};
}

CajaRect CajaLayout::boundsForEntire(const CajaIcon &i) const
{
    const CajaRect &t = i.g.totalForEntire;
    return {int(i.x) + t.x0, int(i.y) + t.y0, int(i.x) + t.x1 + 1, int(i.y) + t.y1 + 1};
}

// icon_set_position_full en el escritorio: el icono entero queda dentro del área útil.
void CajaLayout::setPosition(CajaIcon &icon, double x, double y) const
{
    const double ppu = pixelsPerUnit;
    const int left = 0, top = 0;
    const int right = int(left + canvasWidth * ppu / ppu);
    const int bottom = int(top + canvasHeight * ppu / ppu);
    const CajaRect b = boundsForEntire(icon);
    const int itemW = b.x1 - b.x0, itemH = b.y1 - b.y0;
    const double heightAbove = icon.y - b.y0;
    const double widthLeft = icon.x - b.x0;
    const double minX = left + DESKTOP_PAD_HORIZONTAL + widthLeft;
    const double maxX = right - DESKTOP_PAD_HORIZONTAL - itemW + widthLeft;
    x = clampG(x, minX, maxX);
    const double minY = top + heightAbove + DESKTOP_PAD_VERTICAL;
    const double maxY = bottom - DESKTOP_PAD_VERTICAL - itemH + heightAbove;
    y = clampG(y, minY, maxY);
    icon.x = x;
    icon.y = y;
}

void CajaLayout::snapPosition(const CajaIcon &icon, int &x, int &y) const
{
    const int iconWidth = int(icon.g.iconW / pixelsPerUnit);
    const int iconHeight = int(icon.g.iconH / pixelsPerUnit);
    const int totalWidth = canvasWidth, totalHeight = canvasHeight;
    if (x + iconWidth / 2 < DESKTOP_PAD_HORIZONTAL + SNAP_SIZE_X)
        x = DESKTOP_PAD_HORIZONTAL + SNAP_SIZE_X - iconWidth / 2;
    if (x + iconWidth / 2 > totalWidth - (DESKTOP_PAD_HORIZONTAL + SNAP_SIZE_X))
        x = totalWidth - (DESKTOP_PAD_HORIZONTAL + SNAP_SIZE_X + (iconWidth / 2));
    if (y + iconHeight < DESKTOP_PAD_VERTICAL + SNAP_SIZE_Y)
        y = DESKTOP_PAD_VERTICAL + SNAP_SIZE_Y - iconHeight;
    if (y + iconHeight > totalHeight - (DESKTOP_PAD_VERTICAL + SNAP_SIZE_Y))
        y = totalHeight - (DESKTOP_PAD_VERTICAL + SNAP_SIZE_Y + (iconHeight / 2));
    const int centerX = x + iconWidth / 2;
    x = int(snapH(eelRound, centerX)) - (iconWidth / 2);
    int baseline = y + iconHeight;
    baseline = int(snapV(eelRound, baseline));
    y = baseline - iconHeight;
}

// PlacementGrid de Caja («no apretado»: floor en los dos extremos).
namespace {
struct Grid {
    int cols = 0, rows = 0;
    QList<char> cells;
    bool ok() const { return cols > 0 && rows > 0; }
    CajaRect toGrid(const CajaRect &c) const
    {
        CajaRect g;
        g.x0 = int(std::floor(double(c.x0 - DESKTOP_PAD_HORIZONTAL) / SNAP_SIZE_X));
        g.y0 = int(std::floor(double(c.y0 - DESKTOP_PAD_VERTICAL) / SNAP_SIZE_Y));
        g.x1 = int(std::floor(double(c.x1 - DESKTOP_PAD_HORIZONTAL) / SNAP_SIZE_X));
        g.y1 = int(std::floor(double(c.y1 - DESKTOP_PAD_VERTICAL) / SNAP_SIZE_Y));
        g.x0 = clampG(g.x0, 0, cols - 1);
        g.y0 = clampG(g.y0, 0, rows - 1);
        g.x1 = clampG(g.x1, g.x0, cols - 1);
        g.y1 = clampG(g.y1, g.y0, rows - 1);
        return g;
    }
    bool isFree(const CajaRect &g) const
    {
        for (int x = g.x0; x <= g.x1; ++x)
            for (int y = g.y0; y <= g.y1; ++y)
                if (cells.at(x * rows + y))
                    return false;
        return true;
    }
    void mark(const CajaRect &g)
    {
        for (int x = g.x0; x <= g.x1; ++x)
            for (int y = g.y0; y <= g.y1; ++y)
                cells[x * rows + y] = 1;
    }
};
}

static Grid makeGrid(int w, int h)
{
    Grid g;
    g.cols = w / SNAP_SIZE_X;
    g.rows = h / SNAP_SIZE_Y;
    if (g.ok())
        g.cells.fill(0, g.cols * g.rows);
    return g;
}

void CajaLayout::findEmptyLocation(const QList<CajaIcon> &placed, const CajaIcon &icon, int startX, int startY, int &x, int &y) const
{
    Grid grid = makeGrid(canvasWidth, canvasHeight);
    if (!grid.ok()) {
        x = startX;
        y = startY;
        return;
    }
    for (const CajaIcon &p : placed)
        grid.mark(grid.toGrid(boundsForLayout(p)));

    const CajaRect bl = boundsForLayout(icon), be = boundsForEntire(icon);
    const double iconWidth = bl.x1 - bl.x0, iconHeight = bl.y1 - bl.y0;
    const int heightForBoundCheck = be.y1 - be.y0;
    const double pixH = icon.g.iconH / pixelsPerUnit;
    snapPosition(icon, startX, startY);
    CajaRect pos;
    pos.x0 = startX;
    pos.y0 = startY;
    pos.x1 = int(pos.x0 + iconWidth);
    pos.y1 = int(pos.y0 + iconHeight);
    bool collision;
    do {
        collision = false;
        const CajaRect gp = grid.toGrid(pos);
        const bool newColumn = pos.y0 + heightForBoundCheck + DESKTOP_PAD_VERTICAL > canvasHeight;
        if (newColumn || !grid.isFree(gp)) {
            pos.y0 += SNAP_SIZE_Y;
            pos.y1 = int(pos.y0 + iconHeight);
            if (newColumn) {
                pos.y0 = int(DESKTOP_PAD_VERTICAL + SNAP_SIZE_Y - pixH);
                while (pos.y0 < DESKTOP_PAD_VERTICAL)
                    pos.y0 += SNAP_SIZE_Y;
                pos.y1 = int(pos.y0 + iconHeight);
                pos.x0 += SNAP_SIZE_X;
                pos.x1 = int(pos.x0 + iconWidth);
            }
            collision = true;
        }
    } while (collision && pos.x1 < canvasWidth);
    x = pos.x0;
    y = pos.y0;
}

// finish_adding_new_icons + lay_down_icons_vertical_desktop (orden ya dado por el que llama).
void CajaLayout::layout(QList<CajaIcon> &icons) const
{
    for (CajaIcon &i : icons)
        i.g = measure(i.name, i.iconW, i.iconH, i.additional);
    // Los que tienen posición guardada se colocan (con el recorte del escritorio).
    for (CajaIcon &i : icons)
        if (i.positioned) {
            const double sx = i.x, sy = i.y;
            i.x = 0;
            i.y = 0;
            setPosition(i, sx, sy);
        }
    QList<int> unplaced;
    for (int k = 0; k < icons.size(); ++k)
        if (!icons.at(k).positioned)
            unplaced << k;
    if (unplaced.isEmpty())
        return;
    const double ppu = pixelsPerUnit;

    if (unplaced.size() < icons.size() || alwaysFindEmpty) {
        // Hay iconos ya colocados: cada nuevo va al primer hueco libre empezando por la primera columna.
        QList<CajaIcon> placed;
        for (const CajaIcon &i : std::as_const(icons))
            if (i.positioned)
                placed << i;
        for (int k : std::as_const(unplaced)) {
            CajaIcon &i = icons[k];
            i.x = 0;
            i.y = 0;
            const double iw = i.iconW / ppu, ih = i.iconH / ppu;
            int x = int(DESKTOP_PAD_HORIZONTAL + (SNAP_SIZE_X / 2) - (iw / 2));
            int y = int(DESKTOP_PAD_VERTICAL + SNAP_SIZE_Y - ih);
            findEmptyLocation(placed, i, x, y, x, y);
            setPosition(i, x, y);
            i.positioned = true;
            placed << i;
        }
        return;
    }

    // Ninguno colocado: columnas de arriba abajo, ajustadas a la rejilla.
    const int height = canvasHeight;
    const bool shouldSnap = !(tighter && !keepAligned);
    int x = DESKTOP_PAD_HORIZONTAL;
    int start = 0;
    while (start < icons.size()) {
        int y = DESKTOP_PAD_VERTICAL;
        int maxWidth = 0;
        for (int p = start; p < icons.size(); ++p) {
            const CajaIcon &i = icons.at(p);
            const CajaRect bl = boundsForLayout(i), be = boundsForEntire(i);
            const int iconWidth = bl.x1 - bl.x0, iconHeight = bl.y1 - bl.y0;
            const int checkH = be.y1 - be.y0;
            if (shouldSnap) {
                int baseline = int(y + i.iconH / ppu);
                baseline = int(snapV(ceilD, baseline));
                y = int(baseline - i.iconH / ppu);
            }
            if (y != DESKTOP_PAD_VERTICAL && y + checkH > height)
                break;
            if (maxWidth < iconWidth)
                maxWidth = iconWidth;
            y += iconHeight + DESKTOP_PAD_VERTICAL;
        }
        y = DESKTOP_PAD_VERTICAL;
        int centerX = x + maxWidth / 2;
        int columnWidth = maxWidth;
        if (shouldSnap) {
            centerX = int(snapH(ceilD, centerX));
            columnWidth = (centerX - x) + (maxWidth / 2);
        }
        int p = start;
        for (; p < icons.size(); ++p) {
            CajaIcon &i = icons[p];
            const CajaRect bl = boundsForLayout(i), be = boundsForEntire(i);
            const int iconHeight = bl.y1 - bl.y0;
            const int checkH = be.y1 - be.y0;
            if (shouldSnap) {
                int baseline = int(y + i.iconH / ppu);
                baseline = int(snapV(ceilD, baseline));
                y = int(baseline - i.iconH / ppu);
            }
            if (y != DESKTOP_PAD_VERTICAL && y > height - checkH && p != start) {
                x += columnWidth + DESKTOP_PAD_HORIZONTAL;
                break;
            }
            setPosition(i, centerX - (i.iconW / ppu) / 2, y);
            i.positioned = true;
            y += iconHeight + DESKTOP_PAD_VERTICAL;
        }
        start = p;
    }
}
