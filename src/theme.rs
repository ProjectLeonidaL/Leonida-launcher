//! The launcher's look: the project logo's sunset (purple, pink, orange) over a dusk-blue
//! night, Bahnschrift for headings and Segoe UI for text, both from Windows' own fonts.

use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Mesh, Pos2, Rect,
    Response, RichText, Role, Sense, Stroke, TextStyle, Ui, Vec2, WidgetInfo, vec2,
};

pub const NIGHT: Color32 = Color32::from_rgb(0x14, 0x1b, 0x2d);
pub const DEEP: Color32 = Color32::from_rgb(0x0f, 0x14, 0x22);
pub const BAY: Color32 = Color32::from_rgb(0x1c, 0x25, 0x40);
pub const SHADE: Color32 = Color32::from_rgb(0x2a, 0x35, 0x58);
pub const FOAM: Color32 = Color32::from_rgb(0xea, 0xe6, 0xf5);
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa3, 0xc0);
pub const PURPLE: Color32 = Color32::from_rgb(0x8b, 0x5c, 0xf6);
pub const PINK: Color32 = Color32::from_rgb(0xff, 0x3d, 0x9a);
pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x8a, 0x3d);
pub const SEA: Color32 = Color32::from_rgb(0x4a, 0xd6, 0x9e);

const SIGN: &str = "sign";
const SECTION: &str = "section";

fn sign() -> FontFamily {
    FontFamily::Name(SIGN.into())
}

/// Bahnschrift and Segoe UI from the Windows font folder; egui's own fonts stay behind them for
/// characters they lack, and stand in for them when they are missing.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let folder = PathBuf::from(env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into()));
    let mut load = |name: &str, file: &str| {
        let bytes = fs::read(folder.join("Fonts").join(file)).ok()?;
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_owned(bytes)));
        Some(name.to_string())
    };
    let text = load("segoe", "segoeui.ttf");
    let heading = load("bahnschrift", "bahnschrift.ttf");
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    if let Some(text) = text {
        proportional.insert(0, text);
    }
    let mut signs: Vec<String> = heading.into_iter().collect();
    signs.extend(proportional.iter().cloned());
    fonts.families.insert(sign(), signs);
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(30.0, sign())),
            (TextStyle::Name(SECTION.into()), FontId::new(19.0, sign())),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 7.0);
        style.spacing.button_padding = vec2(12.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.combo_width = 150.0;

        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.panel_fill = NIGHT;
        visuals.window_fill = BAY;
        visuals.extreme_bg_color = DEEP;
        visuals.faint_bg_color = Color32::from_rgb(0x18, 0x20, 0x37);
        visuals.code_bg_color = BAY;
        visuals.warn_fg_color = ORANGE;
        visuals.hyperlink_color = PINK;
        visuals.selection.bg_fill = PURPLE.gamma_multiply(0.6);
        visuals.selection.stroke = Stroke::new(1.0, FOAM);
        visuals.window_corner_radius = CornerRadius::same(8);
        visuals.menu_corner_radius = CornerRadius::same(6);
        visuals.window_stroke = Stroke::new(1.0, SHADE);

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, FOAM);
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, SHADE);
        for (state, fill, edge) in [
            (&mut widgets.inactive, BAY, SHADE),
            (&mut widgets.hovered, SHADE, PURPLE),
            (&mut widgets.active, PURPLE.gamma_multiply(0.7), PURPLE),
            (&mut widgets.open, SHADE, PURPLE),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, edge);
            state.fg_stroke = Stroke::new(1.5, FOAM);
            state.corner_radius = CornerRadius::same(6);
            state.expansion = 0.0;
        }
    });
}

/// A section title, with a marker for whether this step is done when it is a step.
pub fn section(ui: &mut Ui, title: &str, ready: Option<bool>) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        section_title(ui, title);
        if let Some(ready) = ready {
            let (color, word) = if ready {
                (SEA, "ready")
            } else {
                (ORANGE, "needed")
            };
            ui.add_space(4.0);
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, color);
            ui.label(RichText::new(word).small().color(color));
        }
    });
}

pub fn section_title(ui: &mut Ui, title: &str) {
    let style = TextStyle::Name(SECTION.into());
    ui.label(RichText::new(title).text_style(style).color(FOAM));
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(MUTED));
}

/// The Play button: the logo's sunset as a band, the one loud thing in the window.
pub fn play_button(ui: &mut Ui, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(168.0, 42.0), sense);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, "Play"));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (mut left, mut right) = if enabled {
        (PINK, ORANGE)
    } else {
        (SHADE, SHADE)
    };
    if enabled && response.is_pointer_button_down_on() {
        left = left.gamma_multiply(0.85);
        right = right.gamma_multiply(0.85);
    } else if enabled && response.hovered() {
        left = left.lerp_to_gamma(Color32::WHITE, 0.12);
        right = right.lerp_to_gamma(Color32::WHITE, 0.12);
    }
    ui.painter().add(gradient(rect, left, right));
    let text = if enabled { NIGHT } else { MUTED };
    let font = FontId::new(21.0, sign());
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Play",
        font,
        text,
    );
    if response.has_focus() {
        let ring = rect.expand(3.0);
        ui.painter()
            .rect_stroke(ring, 2.0, Stroke::new(2.0, FOAM), egui::StrokeKind::Outside);
    }
    response
}

/// A rectangle shaded from `left` to `right`.
fn gradient(rect: Rect, left: Color32, right: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let corners = [
        (rect.left_top(), left),
        (rect.right_top(), right),
        (rect.right_bottom(), right),
        (rect.left_bottom(), left),
    ];
    for (pos, color) in corners {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh
}

/// The window icon: the logo's sunset, top to bottom.
pub fn icon() -> egui::IconData {
    const SIZE: u32 = 64;
    let stops = [(0.0, PURPLE), (0.55, PINK), (1.0, ORANGE)];
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        let t = y as f32 / (SIZE - 1) as f32;
        let i = usize::from(t > stops[1].0);
        let ((from, a), (to, b)) = (stops[i], stops[i + 1]);
        let color = a.lerp_to_gamma(b, (t - from) / (to - from));
        for x in 0..SIZE {
            let inside = rounded_square(Pos2::new(x as f32 + 0.5, y as f32 + 0.5), SIZE as f32);
            let alpha = if inside { 255 } else { 0 };
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
