//! The header: dusk over a gulf, painted from the header's rect alone. Fixed tables and a fixed
//! seed, no clock, so it looks the same on every frame and never asks for a repaint itself.

use std::f32::consts::{PI, TAU};

use eframe::egui::epaint::{PathShape, PathStroke};
use eframe::egui::{Color32, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use eframe::egui::{lerp, pos2, vec2};

use crate::theme::{
    FLAMINGO, GOLD, NIGHT, NIGHT_TOP, SAND, SILHOUETTE, SUNSET, TEAL, title_job, vgradient,
    with_alpha,
};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const SKY: [(f32, Color32); 5] = [
    (0.00, rgb(0x1E1240)),
    (0.50, rgb(0x4A1E66)),
    (0.80, rgb(0xB02F7E)),
    (0.93, rgb(0xFF4F7E)),
    (1.00, rgb(0xFF9A4D)),
];
const SEA: [(f32, Color32); 3] = [(0.00, rgb(0x1B6F7A)), (0.30, rgb(0x173E5E)), (1.00, NIGHT)];
const HAZE: Color32 = rgb(0x2A1650);
const TAIL_LIGHTS: Color32 = rgb(0xFF4D5E);

/// Width and height of the skyline's blocks, left to right, before scaling.
const BUILDINGS: [(f32, f32); 12] = [
    (10.0, 12.0),
    (14.0, 20.0),
    (9.0, 9.0),
    (16.0, 30.0),
    (12.0, 17.0),
    (8.0, 11.0),
    (18.0, 26.0),
    (11.0, 14.0),
    (15.0, 34.0),
    (9.0, 10.0),
    (13.0, 19.0),
    (10.0, 8.0),
];

/// How the sun's reflection breaks up, row by row.
const RIPPLES: [f32; 8] = [1.0, 0.82, 0.95, 0.7, 0.88, 0.6, 0.75, 0.5];

/// Frond angles from straight up in degrees (clockwise positive) and their relative lengths.
const FRONDS: [(f32, f32); 9] = [
    (-150.0, 0.85),
    (-115.0, 1.0),
    (-82.0, 0.95),
    (-50.0, 0.8),
    (-18.0, 0.7),
    (16.0, 0.75),
    (48.0, 0.9),
    (82.0, 1.0),
    (118.0, 0.88),
];

/// The header's frame of reference: its rect, the horizon and the pixel scale.
struct Stage {
    rect: Rect,
    horizon: f32,
    scale: f32,
    compact: bool,
}

impl Stage {
    fn x(&self, fraction: f32) -> f32 {
        self.rect.left() + fraction * self.rect.width()
    }

    /// The causeway's deck top at `x`: it climbs gently towards the right.
    fn deck(&self, x: f32) -> f32 {
        let h = self.rect.height();
        let along = (x - self.rect.left()) / (0.88 * self.rect.width());
        self.horizon + 0.17 * h - 0.06 * h * along
    }
}

pub fn scene(painter: &Painter, rect: Rect, ready: bool) {
    let stage = Stage {
        rect,
        horizon: rect.top() + 0.62 * rect.height(),
        scale: rect.height() / 187.0,
        compact: rect.height() < 140.0,
    };
    let sky = Rect::from_min_max(rect.min, pos2(rect.right(), stage.horizon));
    let sea = Rect::from_min_max(pos2(rect.left(), stage.horizon), rect.max);
    painter.add(vgradient(sky, &SKY));
    let sun = sun(
        &painter.with_clip_rect(sky.intersect(painter.clip_rect())),
        &stage,
    );
    skyline(painter, &stage);
    painter.add(vgradient(sea, &SEA));
    reflection(painter, &stage, sun);
    sparkles(painter, &stage, sun);
    causeway(painter, &stage, ready);
    let s = stage.scale;
    let h = rect.height();
    let base = rect.bottom() + 8.0;
    if !stage.compact && rect.width() >= 640.0 {
        palm(
            painter,
            pos2(rect.right() - 150.0 * s, base),
            0.80 * h,
            -0.25,
            s,
        );
    }
    palm(
        painter,
        pos2(rect.right() - 64.0 * s, base),
        1.08 * h,
        -0.6,
        s,
    );
    title(painter, &stage);
    wave(painter, rect);
}

/// The sun half sunk in the sea: a soft glow, then a disc from gold on top to pink at the
/// horizon. Returns its centre and radius.
fn sun(painter: &Painter, stage: &Stage) -> (Pos2, f32) {
    let h = stage.rect.height();
    let center = pos2(stage.x(0.68), stage.horizon + 0.02 * h);
    let r = 0.25 * h;
    painter.add(radial(center, 2.4 * r, with_alpha(SUNSET, 85)));
    let shade =
        move |y: f32| GOLD.lerp_to_gamma(FLAMINGO, ((y - (center.y - r)) / r).clamp(0.0, 1.0));
    let rim: Vec<Pos2> = (0..56)
        .map(|i| center + r * Vec2::angled(i as f32 / 56.0 * TAU))
        .collect();
    painter.add(fan(center, &rim, shade));
    let edge = PathStroke::new_uv(1.0, move |_, p| shade(p.y));
    painter.add(PathShape::closed_line(rim, edge));
    (center, r)
}

/// A convex fan from `center` through `rim`, each vertex coloured by its height.
fn fan(center: Pos2, rim: &[Pos2], shade: impl Fn(f32) -> Color32) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, shade(center.y));
    for p in rim {
        mesh.colored_vertex(*p, shade(p.y));
    }
    let count = u32::try_from(rim.len()).unwrap_or(0);
    for i in 0..count {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % count);
    }
    mesh
}

/// A soft round glow, `inner` at the centre fading out at `r`.
fn radial(center: Pos2, r: f32, inner: Color32) -> Mesh {
    const SEGMENTS: u32 = 40;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, inner);
    for (radius, color) in [
        (0.4 * r, inner.gamma_multiply(0.4)),
        (r, Color32::TRANSPARENT),
    ] {
        for i in 0..SEGMENTS {
            let angle = i as f32 / SEGMENTS as f32 * TAU;
            mesh.colored_vertex(center + radius * Vec2::angled(angle), color);
        }
    }
    for i in 0..SEGMENTS {
        let next = (i + 1) % SEGMENTS;
        mesh.add_triangle(0, 1 + i, 1 + next);
        let (a, b) = (1 + i, 1 + next);
        let (c, d) = (a + SEGMENTS, b + SEGMENTS);
        mesh.add_triangle(a, c, d);
        mesh.add_triangle(a, d, b);
    }
    mesh
}

/// A low, distant skyline on the horizon with a few lit windows. No landmark.
fn skyline(painter: &Painter, stage: &Stage) {
    let s = stage.scale;
    let start = stage.x(if stage.compact { 0.44 } else { 0.40 });
    let end = stage.x(0.58);
    let fill = SILHOUETTE.lerp_to_gamma(HAZE, 0.35);
    let window = with_alpha(GOLD, 150);
    let mut shapes = Vec::new();
    let mut x = start;
    for (i, &(width, height)) in BUILDINGS.iter().enumerate() {
        let (width, height) = (width * s, height * s);
        if x + width > end {
            break;
        }
        let block = Rect::from_min_max(
            pos2(x, stage.horizon - height),
            pos2(x + width, stage.horizon),
        );
        shapes.push(Shape::rect_filled(block, 0, fill));
        if !stage.compact {
            let rows = ((height - 4.0) / 4.0).max(0.0) as usize;
            let cols = ((width - 2.0) / 4.0).max(0.0) as usize;
            for row in 0..rows {
                for col in 0..cols {
                    if (i * 7 + row * 3 + col) % 5 == 0 {
                        let at = pos2(
                            x + 1.5 + col as f32 * 4.0,
                            block.top() + 2.0 + row as f32 * 4.0,
                        );
                        let lit = Rect::from_min_size(at, vec2(1.5, 1.5));
                        shapes.push(Shape::rect_filled(lit, 0, window));
                    }
                }
            }
        }
        x += width + 2.0 * s;
    }
    painter.extend(shapes);
}

/// The sun's broken reflection, narrowing as it comes towards the viewer.
fn reflection(painter: &Painter, stage: &Stage, (center, r): (Pos2, f32)) {
    let s = stage.scale;
    for (k, ripple) in RIPPLES.iter().enumerate() {
        let kf = k as f32;
        let y = stage.horizon + 3.0 * s + 1.5 * kf * kf * s;
        let half = 0.85 * r * (1.0 - kf / 10.0) * ripple;
        let color = SUNSET.lerp_to_gamma(FLAMINGO, kf / 7.0);
        let alpha = 200 - 20 * u8::try_from(k).unwrap_or(7);
        let bar = Rect::from_center_size(pos2(center.x, y), vec2(2.0 * half, 2.0 * s));
        painter.rect_filled(bar, 1, with_alpha(color, alpha));
    }
}

/// Short glints on the water, kept clear of the reflection.
fn sparkles(painter: &Painter, stage: &Stage, (center, r): (Pos2, f32)) {
    let (rect, s) = (stage.rect, stage.scale);
    let mut random = lcg(0x5EA5);
    let depth = (rect.bottom() - stage.horizon - 20.0 * s).max(0.0);
    for _ in 0..26 {
        let x = rect.left() + random() * rect.width();
        let y = stage.horizon + 6.0 * s + random().powf(1.6) * depth;
        let length = 5.0 + 9.0 * random();
        let alpha = (60.0 + 60.0 * random()) as u8;
        if (x - center.x).abs() < 1.2 * r {
            continue;
        }
        let stroke = Stroke::new(1.0, with_alpha(TEAL, alpha));
        painter.line_segment([pos2(x, y), pos2(x + length, y)], stroke);
    }
}

/// The causeway across the water: deck, pillars, lamps, and when everything is ready, its
/// lamps glow and the traffic's light trails run along it.
fn causeway(painter: &Painter, stage: &Stage, ready: bool) {
    let (rect, s, h) = (stage.rect, stage.scale, stage.rect.height());
    let (left, right) = (rect.left(), rect.right());
    let thick = 4.0 * s;
    let deck = vec![
        pos2(left, stage.deck(left)),
        pos2(right, stage.deck(right)),
        pos2(right, stage.deck(right) + thick),
        pos2(left, stage.deck(left) + thick),
    ];
    let mut shapes = vec![Shape::convex_polygon(deck, SILHOUETTE, Stroke::NONE)];
    let mut x = left + 10.0 * s;
    while x < right {
        let top = stage.deck(x) + thick;
        let pillar = Rect::from_min_size(pos2(x, top - 0.5), vec2(2.0 * s, 0.06 * h));
        shapes.push(Shape::rect_filled(pillar, 0, SILHOUETTE));
        x += 26.0 * s;
    }
    let mut x = left + 22.0 * s;
    while x < right {
        let top = stage.deck(x);
        let tip = pos2(x, top - 9.0 * s);
        shapes.push(Shape::line_segment(
            [pos2(x, top), tip],
            Stroke::new(1.0, SILHOUETTE),
        ));
        if ready {
            shapes.push(Shape::circle_filled(tip, 6.0 * s, with_alpha(GOLD, 40)));
            shapes.push(Shape::circle_filled(tip, 1.6 * s, GOLD));
        } else {
            shapes.push(Shape::circle_filled(tip, 1.6 * s, with_alpha(GOLD, 60)));
        }
        x += 44.0 * s;
    }
    if ready {
        for (offset, color) in [(-2.0 * s, SAND), (-4.0 * s, TAIL_LIGHTS)] {
            const SEGMENTS: usize = 30;
            for i in 0..SEGMENTS {
                let (t0, t1) = (i as f32 / SEGMENTS as f32, (i + 1) as f32 / SEGMENTS as f32);
                let (x0, x1) = (lerp(left..=right, t0), lerp(left..=right, t1));
                let middle = (t0 + t1) / 2.0;
                let alpha = (220.0 * (PI * middle).sin()) as u8;
                let a = pos2(x0, stage.deck(x0) + offset);
                let b = pos2(x1, stage.deck(x1) + offset);
                shapes.push(Shape::line_segment(
                    [a, b],
                    Stroke::new(1.5 * s, with_alpha(color, alpha)),
                ));
            }
        }
    }
    painter.extend(shapes);
}

/// A palm in silhouette. `lean` bends the trunk, negative to the left.
fn palm(painter: &Painter, base: Pos2, height: f32, lean: f32, s: f32) {
    let crown = base + vec2(0.35 * lean * height, -height);
    let bend = base + vec2(0.2 * lean * height, -0.5 * height);
    let trunk = bezier(base, bend, crown, 16);
    let widths: Vec<f32> = (0..trunk.len())
        .map(|i| lerp(0.040 * height..=0.018 * height, i as f32 / 15.0))
        .collect();
    let (mesh, outline) = strip(&trunk, &widths, SILHOUETTE);
    let mut shapes = vec![
        Shape::mesh(mesh),
        PathShape::closed_line(outline, Stroke::new(1.0, SILHOUETTE)).into(),
    ];
    let rib = Stroke::new(2.6 * s, SILHOUETTE);
    let leaflet = Stroke::new(1.5 * s, SILHOUETTE);
    for (angle, length) in FRONDS {
        let a = angle.to_radians();
        let f = 0.42 * height * length;
        let dir = vec2(a.sin(), -a.cos());
        let mid = crown + dir * 0.55 * f + vec2(0.0, -0.15 * f);
        let tip = crown + dir * f + vec2(0.0, 0.45 * f * a.sin().abs());
        let rachis = bezier(crown, mid, tip, 28);
        let last = rachis.len() - 1;
        for i in 3..last {
            let t = i as f32 / last as f32;
            let tangent = (rachis[i + 1] - rachis[i - 1]).normalized();
            let normal = tangent.rot90();
            // Short near the crown so neighbouring fronds don't merge into one dark knot.
            let len = 0.19 * f * (PI * t).sin().powf(1.2) * (0.4 + 0.6 * t) + 1.0;
            for side in [-1.0, 1.0] {
                let d = (side * normal + vec2(0.0, 0.4) + 0.4 * tangent).normalized();
                shapes.push(Shape::line_segment(
                    [rachis[i], rachis[i] + d * len],
                    leaflet,
                ));
            }
        }
        shapes.push(PathShape::line(rachis, rib).into());
    }
    let nut = 0.022 * height;
    for offset in [vec2(-3.0, 4.0), vec2(3.0, 5.0), vec2(0.0, 8.0)] {
        shapes.push(Shape::circle_filled(crown + offset * s, nut, SILHOUETTE));
    }
    painter.extend(shapes);
}

fn title(painter: &Painter, stage: &Stage) {
    let rect = stage.rect;
    let size = if stage.compact { 30.0 } else { 36.0 };
    let at = pos2(rect.left() + 28.0, rect.top() + 0.13 * rect.height());
    let shadow = with_alpha(NIGHT, 150);
    let galley = painter.layout_job(title_job("Leonida Launcher", size, SAND));
    let below = at.y + galley.size().y + 8.0;
    painter.galley_with_override_text_color(at + vec2(0.0, 2.0), galley.clone(), shadow);
    painter.galley(at, galley, SAND);
    if stage.compact {
        return;
    }
    let font = FontId::new(14.5, FontFamily::Proportional);
    let text = "Pick your game, choose switches, play.";
    let color = SAND.gamma_multiply(0.9);
    let galley = painter.layout_no_wrap(text.into(), font, color);
    let at = pos2(at.x, below);
    painter.galley_with_override_text_color(at + vec2(0.0, 2.0), galley.clone(), shadow);
    painter.galley(at, galley, color);
}

/// The bottom edge as a gentle wave in the board's colour, with a line of foam on it.
fn wave(painter: &Painter, rect: Rect) {
    let mut points = Vec::new();
    let mut x = rect.left();
    loop {
        let u = x - rect.left();
        let y = rect.bottom() - 12.0 + 4.0 * (u / 46.0).sin() + 2.0 * (u / 17.0 + 1.3).sin();
        points.push(pos2(x, y));
        if x >= rect.right() {
            break;
        }
        x = (x + 6.0).min(rect.right());
    }
    let mut fill = Mesh::default();
    for (i, p) in (0u32..).zip(&points) {
        fill.colored_vertex(*p, NIGHT_TOP);
        fill.colored_vertex(pos2(p.x, rect.bottom()), NIGHT_TOP);
        if i > 0 {
            let a = 2 * i - 2;
            fill.add_triangle(a, a + 2, a + 1);
            fill.add_triangle(a + 1, a + 2, a + 3);
        }
    }
    painter.add(fill);
    let lower: Vec<Pos2> = points.iter().map(|p| *p + vec2(0.0, 4.0)).collect();
    painter.add(PathShape::line(
        points,
        Stroke::new(1.5, with_alpha(TEAL, 160)),
    ));
    painter.add(PathShape::line(
        lower,
        Stroke::new(1.0, with_alpha(TEAL, 60)),
    ));
}

/// `n` points along the quadratic Bezier curve from `p0` to `p2`, both ends included.
fn bezier(p0: Pos2, p1: Pos2, p2: Pos2, n: usize) -> Vec<Pos2> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            let u = 1.0 - t;
            (u * u * p0.to_vec2() + 2.0 * u * t * p1.to_vec2() + t * t * p2.to_vec2()).to_pos2()
        })
        .collect()
}

/// A ribbon along `points`, `half_widths[i]` either side of point i. Also returns its outline
/// (left side out, right side back) for an anti-aliased edge.
fn strip(points: &[Pos2], half_widths: &[f32], color: Color32) -> (Mesh, Vec<Pos2>) {
    let mut mesh = Mesh::default();
    let mut left = Vec::with_capacity(points.len());
    let mut right = Vec::with_capacity(points.len());
    let last = points.len().saturating_sub(1);
    for (i, p) in points.iter().enumerate() {
        let ahead = points[(i + 1).min(last)];
        let behind = points[i.saturating_sub(1)];
        let normal = (ahead - behind).normalized().rot90();
        left.push(*p - normal * half_widths[i]);
        right.push(*p + normal * half_widths[i]);
    }
    for (i, (l, r)) in (0u32..).zip(left.iter().zip(&right)) {
        mesh.colored_vertex(*l, color);
        mesh.colored_vertex(*r, color);
        if i > 0 {
            let a = 2 * i - 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
    }
    left.extend(right.into_iter().rev());
    (mesh, left)
}

/// A small fixed-seed random sequence in 0..1, so the scene is the same every frame.
fn lcg(seed: u32) -> impl FnMut() -> f32 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 8) as f32 / (1u32 << 24) as f32
    }
}
