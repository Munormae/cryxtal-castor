use super::math::Vec3;
use super::overlay::OverlayPainter;
use super::ui::{Color32, Point2, Rect, Stroke, pos2, vec2};
use super::viewcube::ViewBasis;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisTarget {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

#[derive(Clone, Copy, Debug)]
pub struct AxisPick {
    pub target: AxisTarget,
    pub forward: Vec3,
}

const AXIS_COLOR_X: Color32 = Color32::from_rgb(250, 102, 104);
const AXIS_COLOR_Y: Color32 = Color32::from_rgb(17, 235, 107);
const AXIS_COLOR_Z: Color32 = Color32::from_rgb(102, 137, 239);
const AXIS_COLOR_NEG: Color32 = Color32::from_rgba_unmultiplied(198, 199, 194, 210);
const AXIS_LENGTH: f64 = 0.9;

const AXIS_TARGETS: [AxisTarget; 6] = [
    AxisTarget::PosX,
    AxisTarget::NegX,
    AxisTarget::PosY,
    AxisTarget::NegY,
    AxisTarget::PosZ,
    AxisTarget::NegZ,
];

pub fn rect(viewport: Rect) -> Rect {
    let size = (viewport.width().min(viewport.height()) * 0.22).clamp(70.0, 120.0);
    let padding = 12.0;
    Rect::from_min_size(
        pos2(viewport.right() - padding - size, viewport.top() + padding),
        vec2(size, size),
    )
}

pub fn draw<P: OverlayPainter>(
    painter: &mut P,
    viewport: Rect,
    basis: ViewBasis,
    hover: Option<AxisTarget>,
) {
    let rect = rect(viewport);
    let size = rect.width().min(rect.height());
    let center = rect.center();
    let radius = size * 0.5;
    let bg = Color32::from_rgba_unmultiplied(20, 22, 28, 200);
    let border = Color32::from_rgba_unmultiplied(90, 95, 100, 220);

    painter.circle_filled(center, radius, bg);
    painter.circle_stroke(center, radius, Stroke::new(1.0, border));

    let axis_scale = (size * 0.35) as f64;
    let head_radius = (size * 0.07).clamp(4.5, 8.5);
    let head_radius_hover = head_radius * 1.4;
    let line_width = (size * 0.03).clamp(1.6, 3.2);

    let mut axes = Vec::with_capacity(AXIS_TARGETS.len());
    for target in AXIS_TARGETS {
        let dir = axis_direction(target);
        let (pos, depth) = project_axis(dir, basis, center, axis_scale);
        axes.push(ProjectedAxis {
            target,
            pos,
            depth,
            color: axis_color(target),
        });
    }

    axes.sort_by(|a, b| {
        a.depth
            .partial_cmp(&b.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for axis in &axes {
        let mut stroke = Stroke::new(line_width, axis.color);
        if hover == Some(axis.target) {
            stroke = Stroke::new(line_width * 1.35, mix_color(axis.color, Color32::from_rgb(255, 255, 255), 0.25));
        }
        painter.line_segment(center, axis.pos, stroke);
    }

    for axis in &axes {
        let mut color = axis.color;
        let mut radius = head_radius;
        if hover == Some(axis.target) {
            color = mix_color(color, Color32::from_rgb(255, 255, 255), 0.3);
            radius = head_radius_hover;
        }
        painter.circle_filled(axis.pos, radius, color);
        painter.circle_stroke(
            axis.pos,
            radius,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(10, 10, 10, 180)),
        );
    }
}

pub fn pick_target(
    pos: Point2,
    viewport: Rect,
    basis: ViewBasis,
) -> Option<AxisPick> {
    let rect = rect(viewport);
    if !rect.contains(pos) {
        return None;
    }

    let size = rect.width().min(rect.height());
    let center = rect.center();
    if center.distance(pos) > size * 0.5 {
        return None;
    }

    let axis_scale = (size * 0.35) as f64;
    let head_radius = (size * 0.07).clamp(4.5, 8.5);
    let pick_radius = head_radius * 1.35;

    let mut best: Option<(AxisTarget, f64, f32)> = None;
    for target in AXIS_TARGETS {
        let dir = axis_direction(target);
        let (axis_pos, depth) = project_axis(dir, basis, center, axis_scale);
        let dist = pos.distance(axis_pos);
        if dist > pick_radius {
            continue;
        }

        match best {
            Some((_, best_depth, best_dist)) => {
                if depth > best_depth + 1.0e-6
                    || ((depth - best_depth).abs() <= 1.0e-6 && dist < best_dist)
                {
                    best = Some((target, depth, dist));
                }
            }
            None => best = Some((target, depth, dist)),
        }
    }

    best.map(|(target, _, _)| AxisPick {
        target,
        forward: axis_view_direction(target),
    })
}

fn axis_direction(target: AxisTarget) -> Vec3 {
    match target {
        AxisTarget::PosX => Vec3::new(1.0, 0.0, 0.0),
        AxisTarget::NegX => Vec3::new(-1.0, 0.0, 0.0),
        AxisTarget::PosY => Vec3::new(0.0, 1.0, 0.0),
        AxisTarget::NegY => Vec3::new(0.0, -1.0, 0.0),
        AxisTarget::PosZ => Vec3::new(0.0, 0.0, 1.0),
        AxisTarget::NegZ => Vec3::new(0.0, 0.0, -1.0),
    }
}

fn axis_view_direction(target: AxisTarget) -> Vec3 {
    let dir = axis_direction(target);
    Vec3::new(-dir.x, -dir.y, -dir.z)
}

fn axis_color(target: AxisTarget) -> Color32 {
    match target {
        AxisTarget::PosX => AXIS_COLOR_X,
        AxisTarget::NegX => AXIS_COLOR_NEG,
        AxisTarget::PosY => AXIS_COLOR_Y,
        AxisTarget::NegY => AXIS_COLOR_NEG,
        AxisTarget::PosZ => AXIS_COLOR_Z,
        AxisTarget::NegZ => AXIS_COLOR_NEG,
    }
}

fn project_axis(
    dir: Vec3,
    basis: ViewBasis,
    center: Point2,
    scale: f64,
) -> (Point2, f64) {
    let view = Vec3::new(
        dir.dot(basis.right),
        dir.dot(basis.up),
        dir.dot(basis.forward),
    ) * AXIS_LENGTH;
    let pos = pos2(
        center.x + (view.x * scale) as f32,
        center.y - (view.y * scale) as f32,
    );
    let depth = -view.z;
    (pos, depth)
}

fn mix_color(base: Color32, tint: Color32, factor: f32) -> Color32 {
    let [br, bg, bb, _] = base.to_array();
    let [tr, tg, tb, _] = tint.to_array();
    let mix = |b: u8, t: u8| -> u8 {
        let value = (b as f32) * (1.0 - factor) + (t as f32) * factor;
        value.clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(mix(br, tr), mix(bg, tg), mix(bb, tb))
}

struct ProjectedAxis {
    target: AxisTarget,
    pos: Point2,
    depth: f64,
    color: Color32,
}
