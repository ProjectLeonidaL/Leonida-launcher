//! The launcher's look: dusk on the Gulf coast. Flamingo pink and sunset orange over a violet
//! night, teal water, Bahnschrift for signs and Segoe UI for text, all from Windows' own fonts.

use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs};

use eframe::egui::epaint::text::{FontTweak, TextWrapping, VariationCoords};
use eframe::egui::epaint::{PathShape, PathStroke, RoundedRect, tessellator};
use eframe::egui::style::ScrollStyle;
use eframe::egui::text::LayoutJob;
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame,
    Galley, Layout, Margin, Mesh, Pos2, Rect, Response, RichText, Role, Sense, Shadow, Shape,
    Stroke, StrokeKind, TextFormat, TextStyle, Ui, WidgetInfo, pos2, vec2,
};

pub const NIGHT: Color32 = Color32::from_rgb(0x0F, 0x0D, 0x22);
pub const NIGHT_TOP: Color32 = Color32::from_rgb(0x17, 0x13, 0x2F);
pub const PLATE: Color32 = Color32::from_rgb(0x1A, 0x17, 0x34);
pub const PLATE_ALT: Color32 = Color32::from_rgb(0x1F, 0x1B, 0x3D);
pub const INPUT: Color32 = Color32::from_rgb(0x24, 0x1F, 0x45);
pub const INSET: Color32 = Color32::from_rgb(0x0B, 0x0A, 0x1A);
pub const LINE: Color32 = Color32::from_rgb(0x3A, 0x31, 0x62);
pub const FLAMINGO: Color32 = Color32::from_rgb(0xFF, 0x3D, 0x8B);
pub const SUNSET: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x3D);
pub const GOLD: Color32 = Color32::from_rgb(0xFF, 0xD2, 0x7A);
pub const TEAL: Color32 = Color32::from_rgb(0x1F, 0xC8, 0xC1);
pub const SAND: Color32 = Color32::from_rgb(0xF4, 0xE6, 0xD0);
pub const DRIFTWOOD: Color32 = Color32::from_rgb(0x9C, 0x95, 0xB8);
pub const SILHOUETTE: Color32 = Color32::from_rgb(0x0A, 0x07, 0x18);

pub const SIGN: &str = "sign";
const PLAY: &str = "play";
const STRONG: &str = "strong";
const SECTION: &str = "section";

pub fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

/// `color`, which must be opaque, at `alpha`.
pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn add_font(
    fonts: &mut FontDefinitions,
    name: &str,
    bytes: Option<Vec<u8>>,
    coords: VariationCoords,
) -> Option<String> {
    let tweak = FontTweak {
        coords,
        ..Default::default()
    };
    let data = FontData::from_owned(bytes?).tweak(tweak);
    fonts.font_data.insert(name.into(), Arc::new(data));
    Some(name.to_string())
}

/// Segoe UI, Consolas and Bahnschrift from the Windows font folder; egui's own fonts stay behind
/// them for characters they lack, and stand in for them when they are missing.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let folder = PathBuf::from(env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into()));
    let read = |file: &str| fs::read(folder.join("Fonts").join(file)).ok();
    let plain = VariationCoords::default;
    let segoe = add_font(&mut fonts, "segoe", read("segoeui.ttf"), plain());
    let semi = add_font(&mut fonts, "segoe-semi", read("seguisb.ttf"), plain());
    let bold = add_font(&mut fonts, "segoe-bold", read("segoeuib.ttf"), plain());
    let consolas = add_font(&mut fonts, "consolas", read("consola.ttf"), plain());
    let bahnschrift = read("bahnschrift.ttf");
    let sign = VariationCoords::new([(b"wght", 600.0)]);
    let bahn = add_font(&mut fonts, "bahn", bahnschrift.clone(), sign);
    let play = VariationCoords::new([(b"wght", 700.0), (b"wdth", 87.5)]);
    let bahn_bold = add_font(&mut fonts, "bahn-bold", bahnschrift, play);

    let defaults = |family: &FontFamily| fonts.families.get(family).cloned().unwrap_or_default();
    let proportional = defaults(&FontFamily::Proportional);
    let monospace = defaults(&FontFamily::Monospace);
    let stack = |first: &[&Option<String>], rest: &[String]| -> Vec<String> {
        let first = first.iter().filter_map(|name| (*name).clone());
        first.chain(rest.iter().cloned()).collect()
    };
    let families = [
        (FontFamily::Proportional, stack(&[&segoe], &proportional)),
        (FontFamily::Monospace, stack(&[&consolas], &monospace)),
        (family(SIGN), stack(&[&bahn, &segoe], &proportional)),
        (family(PLAY), stack(&[&bahn_bold, &bold], &proportional)),
        (family(STRONG), stack(&[&semi, &segoe], &proportional)),
    ];
    fonts.families.extend(families);
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.all_styles_mut(|style| {
        let proportional = |size| FontId::new(size, FontFamily::Proportional);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(36.0, family(SIGN))),
            (section(), FontId::new(19.0, family(SIGN))),
            (strong(), FontId::new(15.0, family(STRONG))),
            (TextStyle::Body, proportional(14.5)),
            (TextStyle::Button, proportional(14.5)),
            (TextStyle::Small, proportional(12.5)),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(14.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.combo_width = 130.0;
        style.spacing.scroll = ScrollStyle {
            bar_width: 6.0,
            ..ScrollStyle::floating()
        };

        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.panel_fill = NIGHT;
        visuals.window_fill = PLATE;
        visuals.extreme_bg_color = INSET;
        visuals.faint_bg_color = PLATE_ALT;
        visuals.code_bg_color = INPUT;
        visuals.warn_fg_color = SUNSET;
        visuals.error_fg_color = SUNSET;
        visuals.hyperlink_color = FLAMINGO;
        visuals.selection.bg_fill = PLATE.lerp_to_gamma(FLAMINGO, 0.45);
        visuals.selection.stroke = Stroke::new(1.0, SAND);
        visuals.window_corner_radius = CornerRadius::same(10);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.window_stroke = Stroke::new(1.0, LINE);
        visuals.window_shadow = Shadow::NONE;
        visuals.popup_shadow = Shadow::NONE;

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, SAND);
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        widgets.noninteractive.bg_fill = PLATE;
        widgets.noninteractive.weak_bg_fill = PLATE;
        for (state, fill, edge, expansion) in [
            (&mut widgets.inactive, INPUT, LINE, 0.0),
            (
                &mut widgets.hovered,
                INPUT.lerp_to_gamma(SAND, 0.06),
                FLAMINGO,
                1.0,
            ),
            (
                &mut widgets.active,
                PLATE.lerp_to_gamma(FLAMINGO, 0.30),
                FLAMINGO,
                1.0,
            ),
            (&mut widgets.open, PLATE, FLAMINGO, 0.0),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, edge);
            state.fg_stroke = Stroke::new(1.5, SAND);
            state.corner_radius = CornerRadius::same(8);
            state.expansion = expansion;
        }
    });
}

fn section() -> TextStyle {
    TextStyle::Name(SECTION.into())
}

pub fn strong() -> TextStyle {
    TextStyle::Name(STRONG.into())
}

/// A rectangle shaded top to bottom through `stops` (position 0 to 1, colour).
pub fn vgradient(rect: Rect, stops: &[(f32, Color32)]) -> Mesh {
    let mut mesh = Mesh::default();
    for (row, &(t, color)) in (0u32..).zip(stops) {
        let y = egui::lerp(rect.top()..=rect.bottom(), t);
        mesh.colored_vertex(pos2(rect.left(), y), color);
        mesh.colored_vertex(pos2(rect.right(), y), color);
        if row > 0 {
            let a = 2 * row - 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
    }
    mesh
}

/// The ring that custom widgets draw around themselves while they have keyboard focus.
fn focus_ring(ui: &Ui, response: &Response, rect: Rect, corner: f32) {
    if response.has_focus() {
        let stroke = Stroke::new(2.0, SAND);
        let ring = rect.expand(3.0);
        ui.painter()
            .rect_stroke(ring, corner + 3.0, stroke, StrokeKind::Outside);
    }
}

fn check_mark(painter: &egui::Painter, left: Pos2, size: f32, stroke: Stroke) {
    let a = left;
    let b = left + vec2(size * 0.35, size * 0.35);
    let c = left + vec2(size, -size * 0.4);
    painter.line_segment([a, b], stroke);
    painter.line_segment([b, c], stroke);
}

/// `text` on one line, cut with an ellipsis at `width`.
fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

/// A card for one part of the window, with a 3 px colour strip along its top edge.
pub fn plate(ui: &mut Ui, strip: Color32, flash: bool, add: impl FnOnce(&mut Ui)) -> Rect {
    let stroke = if flash {
        Stroke::new(2.0, SUNSET)
    } else {
        Stroke::new(1.0, LINE)
    };
    let margin = Margin {
        left: 16,
        right: 16,
        top: 18,
        bottom: 14,
    };
    let rect = Frame::new()
        .fill(PLATE)
        .stroke(stroke)
        .corner_radius(12)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        })
        .response
        .rect;
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), 3.0));
    let clip = top.intersect(ui.clip_rect());
    ui.painter()
        .with_clip_rect(clip)
        .rect_filled(rect, 12, strip);
    rect
}

/// A plate's title row: an optional numbered step badge, the title, then `right` from the right.
pub fn plate_header(
    ui: &mut Ui,
    step: Option<(u8, bool)>,
    title: &str,
    right: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        if let Some((number, ready)) = step {
            step_badge(ui, number, ready);
            ui.add_space(2.0);
        }
        ui.label(RichText::new(title).text_style(section()).color(SAND));
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
}

fn step_badge(ui: &mut Ui, number: u8, ready: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    let painter = ui.painter();
    let digit = if ready {
        painter.circle_filled(rect.center(), 10.0, TEAL);
        NIGHT
    } else {
        painter.circle_stroke(rect.center(), 9.25, Stroke::new(1.5, SUNSET));
        SUNSET
    };
    let font = FontId::new(13.0, family(SIGN));
    let center = rect.center() + vec2(0.0, 0.5);
    painter.text(center, egui::Align2::CENTER_CENTER, number, font, digit);
}

/// A 16 px round marker with one character in it, before a warning or a note.
pub fn marker(ui: &mut Ui, symbol: &str, fill: Color32, text: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 8.0, fill);
    let font = FontId::new(12.0, family(PLAY));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        symbol,
        font,
        text,
    );
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(DRIFTWOOD));
}

/// A group's caption inside a plate.
pub fn caption(text: &str) -> RichText {
    RichText::new(text)
        .font(FontId::new(13.5, family(STRONG)))
        .color(DRIFTWOOD)
}

/// A small rounded tag: `edge` for its outline and tint, `text` for its words.
pub fn tag(ui: &mut Ui, text: &str, edge: Color32, color: Color32) -> Response {
    let font = FontId::new(12.5, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.into(), font, color);
    let size = galley.size() + vec2(16.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let fill = PLATE.lerp_to_gamma(edge, 0.14);
        let painter = ui.painter();
        let stroke = Stroke::new(1.0, edge);
        painter.rect(rect, rect.height() / 2.0, fill, stroke, StrokeKind::Inside);
        painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    }
    response
}

/// A tag in one colour, for Ready, Needed and counts.
pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    tag(ui, text, color, color)
}

/// A command to type, in a dark well.
pub fn code_chip(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(INSET)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().color(SAND));
        });
}

/// The button that finishes a step that is still missing.
pub fn primary(text: &str) -> egui::Button<'static> {
    let text = RichText::new(text).color(NIGHT).text_style(strong());
    egui::Button::new(text)
        .fill(TEAL)
        .stroke(Stroke::NONE)
        .corner_radius(8)
}

/// A rounded outline button, for the second action of the run bar.
pub fn outline_pill(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    // egui fades disabled widgets by half, which leaves teal on night nearly invisible.
    let color = if enabled { TEAL } else { DRIFTWOOD };
    let button = egui::Button::new(RichText::new(text).color(color))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.5, color))
        .corner_radius(18)
        .min_size(vec2(0.0, 36.0));
    ui.add_enabled(enabled, button)
}

/// A rounded on/off chip, the way profiles are ticked. Flips `on` when clicked.
pub fn chip(ui: &mut Ui, name: &str, on: &mut bool) -> Response {
    let font = FontId::new(14.5, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(name.into(), font, SAND);
    let check = if *on { 16.0 } else { 0.0 };
    let size = vec2(galley.size().x + 24.0 + check, 30.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let on = *on;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, name));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (fill, stroke) = if on {
        let fill = PLATE.lerp_to_gamma(FLAMINGO, 0.22);
        (fill, Stroke::new(1.5, FLAMINGO))
    } else if response.hovered() {
        (INPUT, Stroke::new(1.0, FLAMINGO))
    } else {
        (INPUT, Stroke::new(1.0, LINE))
    };
    let painter = ui.painter();
    painter.rect(rect, 15, fill, stroke, StrokeKind::Inside);
    let mut x = rect.left() + 12.0;
    if on {
        let left = pos2(x, rect.center().y);
        check_mark(painter, left, 10.0, Stroke::new(2.0, FLAMINGO));
        x += check;
    }
    let top = rect.center().y - galley.size().y / 2.0;
    painter.galley(pos2(x, top), galley, SAND);
    focus_ring(ui, &response, rect, 15.0);
    response
}

/// One game the search found: its name and folder, ticked when it is the one in use.
pub fn found_row(ui: &mut Ui, name: &str, path: &str, chosen: bool) -> Response {
    let size = vec2(ui.available_width(), 44.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, chosen, name));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let fill = if response.hovered() { PLATE_ALT } else { PLATE };
    let stroke = if chosen {
        Stroke::new(1.5, TEAL)
    } else if response.hovered() {
        Stroke::new(1.0, FLAMINGO)
    } else {
        Stroke::new(1.0, LINE)
    };
    ui.painter().rect(rect, 8, fill, stroke, StrokeKind::Inside);
    let width = rect.width() - 24.0 - 28.0;
    let strong = FontId::new(15.0, family(STRONG));
    let small = FontId::new(12.5, FontFamily::Proportional);
    let name = one_line(ui, name, strong, SAND, width);
    let path = one_line(ui, path, small, DRIFTWOOD, width);
    let left = rect.left() + 12.0;
    let top = rect.center().y - (name.size().y + path.size().y) / 2.0;
    let below = top + name.size().y;
    ui.painter().galley(pos2(left, top), name, SAND);
    ui.painter().galley(pos2(left, below), path, DRIFTWOOD);
    if chosen {
        let at = pos2(rect.right() - 26.0, rect.center().y);
        check_mark(ui.painter(), at, 12.0, Stroke::new(2.0, TEAL));
    }
    focus_ring(ui, &response, rect, 8.0);
    response
}

/// The 2 px line along the top of the run bar, flamingo through sunset to teal.
pub fn neon_line(painter: &egui::Painter, rect: Rect) {
    let y = rect.top() + 1.0;
    let points = vec![pos2(rect.left(), y), pos2(rect.right(), y)];
    let stroke = PathStroke::new_uv(2.0, |r, p| {
        let t = ((p.x - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0);
        if t < 0.5 {
            FLAMINGO.lerp_to_gamma(SUNSET, t * 2.0)
        } else {
            SUNSET.lerp_to_gamma(TEAL, t * 2.0 - 1.0)
        }
    });
    painter.add(PathShape::line(points, stroke));
}

/// The Play button: the sunset as a lit pill when everything is ready, a quiet outline before.
pub fn play_button(ui: &mut Ui, ready: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(184.0, 48.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ready, "Play"));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let pressed = ready && response.is_pointer_button_down_on();
    let hovered = response.hovered();
    let rect = if pressed {
        rect.translate(vec2(0.0, 1.0))
    } else {
        rect
    };
    let label = if ready {
        for (grow, rest, hover) in [(10.0, 10, 16), (6.0, 22, 34), (3.0, 40, 60)] {
            let alpha = if hovered { hover } else { rest };
            let glow = with_alpha(FLAMINGO, alpha);
            painter.rect_filled(rect.expand(grow), 24.0 + grow, glow);
        }
        let (mut left, mut right) = (FLAMINGO, SUNSET);
        if pressed {
            left = left.gamma_multiply(0.85);
            right = right.gamma_multiply(0.85);
        } else if hovered {
            left = left.lerp_to_gamma(Color32::WHITE, 0.1);
            right = right.lerp_to_gamma(Color32::WHITE, 0.1);
        }
        let shade = move |x: f32, from: f32, width: f32| {
            left.lerp_to_gamma(right, ((x - from) / width).clamp(0.0, 1.0))
        };
        let mut outline = Vec::new();
        tessellator::path::rounded_rectangle(&mut outline, RoundedRect::new(rect, 24.0));
        let mut body = Mesh::default();
        let (from, width) = (rect.left(), rect.width());
        body.colored_vertex(rect.center(), shade(rect.center().x, from, width));
        for point in &outline {
            body.colored_vertex(*point, shade(point.x, from, width));
        }
        let count = u32::try_from(outline.len()).unwrap_or(0);
        for i in 0..count {
            body.add_triangle(0, 1 + i, 1 + (i + 1) % count);
        }
        painter.add(body);
        let edge = PathStroke::new_uv(1.0, move |r, p| shade(p.x, r.left(), r.width()));
        painter.add(PathShape::closed_line(outline, edge));
        NIGHT
    } else {
        let stroke = Stroke::new(1.5, DRIFTWOOD);
        painter.rect(rect, 24, PLATE, stroke, StrokeKind::Inside);
        DRIFTWOOD
    };
    let galley = painter.layout_no_wrap("Play".into(), FontId::new(21.0, family(PLAY)), label);
    let (triangle, gap) = (10.0, 9.0);
    let left = rect.center().x - (triangle + gap + galley.size().x) / 2.0;
    let middle = rect.center().y;
    let points = vec![
        pos2(left, middle - 6.0),
        pos2(left + triangle, middle),
        pos2(left, middle + 6.0),
    ];
    painter.add(Shape::convex_polygon(points, label, Stroke::NONE));
    let top = middle - galley.size().y / 2.0;
    painter.galley(pos2(left + triangle + gap, top), galley, label);
    focus_ring(ui, &response, rect, 24.0);
    response
}

/// The window icon: the sun half sunk in the gulf, a causeway across the water. No letters.
pub fn icon() -> egui::IconData {
    const SIZE: u32 = 64;
    let sky = [
        (0.0, Color32::from_rgb(0x1E, 0x12, 0x40)),
        (26.0, Color32::from_rgb(0xB0, 0x2F, 0x7E)),
        (40.0, Color32::from_rgb(0xFF, 0x9A, 0x4D)),
    ];
    let sea_top = Color32::from_rgb(0x1B, 0x6F, 0x7A);
    let bars = [(43.0, 11.0), (47.0, 8.0), (52.0, 5.0)];
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = pos2(x as f32 + 0.5, y as f32 + 0.5);
            let color = if p.y < 40.0 {
                let i = usize::from(p.y > sky[1].0);
                let ((from, a), (to, b)) = (sky[i], sky[i + 1]);
                let base = a.lerp_to_gamma(b, (p.y - from) / (to - from));
                if p.distance(pos2(40.0, 40.0)) <= 15.0 {
                    GOLD.lerp_to_gamma(FLAMINGO, ((p.y - 25.0) / 15.0).clamp(0.0, 1.0))
                } else {
                    base
                }
            } else if (55.0..57.0).contains(&p.y) {
                SILHOUETTE
            } else if bars
                .iter()
                .any(|&(row, half)| (row..row + 2.0).contains(&p.y) && (p.x - 40.0).abs() <= half)
            {
                SUNSET
            } else {
                sea_top.lerp_to_gamma(NIGHT, (p.y - 40.0) / 23.0)
            };
            let alpha = if rounded_square(p, SIZE as f32) {
                255
            } else {
                0
            };
            rgba.extend_from_slice(&[color.r(), color.g(), color.b(), alpha]);
        }
    }
    egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

fn rounded_square(p: Pos2, size: f32) -> bool {
    let (margin, radius) = (size * 0.04, size * 0.2);
    let low = margin + radius;
    let high = size - margin - radius;
    let nearest = Pos2::new(p.x.clamp(low, high), p.y.clamp(low, high));
    p.distance(nearest) <= radius
}

/// The title as painted over the sky: Bahnschrift, a little letter spacing.
pub fn title_job(text: &str, size: f32, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let format = TextFormat {
        font_id: FontId::new(size, family(SIGN)),
        extra_letter_spacing: 0.5,
        color,
        ..Default::default()
    };
    job.append(text, 0.0, format);
    job
}
