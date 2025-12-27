use super::math::Vec3;
use super::overlay::OverlayPainter;
use super::pick::point_in_triangle;
use super::ui::{Align2, Color32, Point2, Rect, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewFace {
    Top,
    Bottom,
    Left,
    Right,
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewTarget {
    Face(ViewFace),
    Edge(usize),
    Corner(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct ViewPick {
    pub target: ViewTarget,
    pub normal: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct ViewBasis {
    pub right: Vec3,
    pub up: Vec3,
    pub forward: Vec3,
}

impl ViewBasis {
    pub fn new(right: Vec3, up: Vec3, forward: Vec3) -> Self {
        Self {
            right,
            up,
            forward,
        }
    }
}

const GIZMO_PICK_INSET: f64 = 0.85;
// Tuned to gizmo_cube.glb (after GIZMO_SCALE) so pick geometry matches rendering.
const GIZMO_PICK_RADIUS: f64 = 0.6706;
const FACE_SCALE: f64 = 0.7945;
const EDGE_SCALE: f64 = 0.8757;
const CORNER_SCALE: f64 = 0.7011;

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
    hover: Option<ViewTarget>,
) {
    let rect = rect(viewport);
    painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(20, 22, 28, 200));
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_gray(70)));

    let projected = project_cube(rect, basis);
    let faces = compute_faces(&projected, basis);
    let hover_face = match hover {
        Some(ViewTarget::Face(face)) => Some(face),
        _ => None,
    };

    for face in faces {
        let is_hover = hover_face == Some(face.face);
        let fill = if is_hover {
            blend_color(face.color, face_hover_tint(face.face), 0.65)
        } else {
            face.color
        };
        let stroke = if is_hover {
            Stroke::new(1.5, face_hover_tint(face.face))
        } else {
            Stroke::new(1.0, Color32::from_gray(30))
        };
        painter.polygon(face.points.to_vec(), fill, stroke);
        painter.text(
            face.center,
            Align2::CenterCenter,
            face.label.to_string(),
            10.0,
            Color32::from_gray(20),
        );
    }

    if let Some(ViewTarget::Edge(edge_idx)) = hover {
        if let Some((a, b)) = EDGE_DEFS.get(edge_idx) {
            let a = projected.points[*a];
            let b = projected.points[*b];
            painter.line_segment(a, b, Stroke::new(4.0, Color32::from_rgb(255, 225, 150)));
            painter.line_segment(a, b, Stroke::new(2.0, Color32::from_rgb(255, 170, 90)));
        }
    }

    if let Some(ViewTarget::Corner(corner_idx)) = hover {
        if let Some(pos) = projected.points.get(corner_idx) {
            painter.circle_filled(*pos, 4.8, Color32::from_rgb(255, 225, 150));
            painter.circle_stroke(*pos, 4.8, Stroke::new(1.2, Color32::from_rgb(255, 170, 90)));
        }
    }
}

pub fn pick_target(
    pos: Point2,
    viewport: Rect,
    basis: ViewBasis,
) -> Option<ViewPick> {
    let rect = rect(viewport);
    if !rect.contains(pos) {
        return None;
    }
    let projected = project_cube(rect, basis);
    let cube = cube_vertices();

    if let Some(corner_idx) = pick_corner(pos, rect, basis) {
        return Some(ViewPick {
            target: ViewTarget::Corner(corner_idx),
            normal: cube[corner_idx].normalized(),
        });
    }

    if let Some(edge_idx) = pick_edge(pos, rect, basis) {
        let (a, b) = EDGE_DEFS[edge_idx];
        let normal = (cube[a] + cube[b]) * 0.5;
        return Some(ViewPick {
            target: ViewTarget::Edge(edge_idx),
            normal: normal.normalized(),
        });
    }

    let faces = compute_faces(&projected, basis);
    if let Some(face) = pick_face_from_faces(pos, &faces) {
        return Some(ViewPick {
            target: ViewTarget::Face(face),
            normal: face_normal(face),
        });
    }

    None
}

pub fn view_direction_from_normal(normal: Vec3) -> Vec3 {
    Vec3::new(-normal.x, -normal.y, -normal.z)
}

fn compute_faces(projected: &ProjectedCube, basis: ViewBasis) -> Vec<ProjectedFace> {
    let mut projected_faces = Vec::new();
    for face in face_defs() {
        let facing = -face.normal.dot(basis.forward);
        if facing <= 0.0 {
            continue;
        }
        let mut points = [Point2::default(); 4];
        let mut depth = 0.0;
        for (i, idx) in face.indices.iter().enumerate() {
            let v = projected.view[*idx];
            depth += v.z;
            points[i] = projected.points[*idx];
        }
        let depth = -(depth / 4.0);
        let color = shade_color(face.base_color, facing);
        let center = points_center(points);
        projected_faces.push(ProjectedFace {
            face: face.face,
            points,
            depth,
            color,
            label: face.label,
            center,
        });
    }

    projected_faces.sort_by(|a, b| {
        a.depth
            .partial_cmp(&b.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    projected_faces
}

fn shade_color(base: Color32, facing: f64) -> Color32 {
    let level = 0.4 + 0.6 * facing.clamp(0.0, 1.0);
    let [r, g, b, _] = base.to_array();
    Color32::from_rgb(
        ((r as f64) * level).clamp(0.0, 255.0) as u8,
        ((g as f64) * level).clamp(0.0, 255.0) as u8,
        ((b as f64) * level).clamp(0.0, 255.0) as u8,
    )
}

fn blend_color(base: Color32, tint: Color32, factor: f32) -> Color32 {
    let [br, bg, bb, _] = base.to_array();
    let [tr, tg, tb, _] = tint.to_array();
    let mix = |b: u8, t: u8| -> u8 {
        let value = (b as f32) * (1.0 - factor) + (t as f32) * factor;
        value.clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(mix(br, tr), mix(bg, tg), mix(bb, tb))
}

fn points_center(points: [Point2; 4]) -> Point2 {
    let mut center = Point2::new(0.0, 0.0);
    for point in points {
        center.x += point.x;
        center.y += point.y;
    }
    pos2(center.x / 4.0, center.y / 4.0)
}

fn point_in_quad(p: Point2, quad: [Point2; 4]) -> bool {
    point_in_triangle(p, quad[0], quad[1], quad[2])
        || point_in_triangle(p, quad[0], quad[2], quad[3])
}

fn pick_face_from_faces(pos: Point2, faces: &[ProjectedFace]) -> Option<ViewFace> {
    let mut best: Option<(f64, ViewFace)> = None;
    for face in faces {
        if point_in_quad(pos, face.points) {
            match best {
                Some((depth, _)) if face.depth <= depth => {}
                _ => best = Some((face.depth, face.face)),
            }
        }
    }
    best.map(|(_, face)| face)
}

fn pick_corner(pos: Point2, rect: Rect, basis: ViewBasis) -> Option<usize> {
    let projected = project_scaled_cube(rect, basis, CORNER_SCALE);
    let size = rect.width().min(rect.height());
    let radius = (size * 0.1).clamp(7.0, 12.0);
    let mut best: Option<(usize, f32, f64)> = None;
    for (idx, point) in projected.points.iter().enumerate() {
        let dist = pos.distance(*point);
        if dist <= radius {
            let depth = -projected.view[idx].z;
            if depth <= 0.0 {
                continue;
            }
            match best {
                Some((_, best_dist, best_depth)) => {
                    if dist < best_dist - 0.1
                        || ((dist - best_dist).abs() <= 0.1 && depth > best_depth)
                    {
                        best = Some((idx, dist, depth));
                    }
                }
                None => best = Some((idx, dist, depth)),
            }
        }
    }
    best.map(|(idx, _, _)| idx)
}

fn pick_edge(pos: Point2, rect: Rect, basis: ViewBasis) -> Option<usize> {
    let projected = project_scaled_cube(rect, basis, EDGE_SCALE);
    let size = rect.width().min(rect.height());
    let threshold = (size * 0.06).clamp(6.0, 9.0);
    let mut best: Option<(usize, f32, f64)> = None;
    for (idx, (a_idx, b_idx)) in EDGE_DEFS.iter().enumerate() {
        let a = projected.points[*a_idx];
        let b = projected.points[*b_idx];
        let dist = point_to_segment_distance(pos, a, b);
        if dist <= threshold {
            let depth = -(projected.view[*a_idx].z + projected.view[*b_idx].z) * 0.5;
            if depth <= 0.0 {
                continue;
            }
            match best {
                Some((_, best_dist, best_depth)) => {
                    if dist < best_dist - 0.1
                        || ((dist - best_dist).abs() <= 0.1 && depth > best_depth)
                    {
                        best = Some((idx, dist, depth));
                    }
                }
                None => best = Some((idx, dist, depth)),
            }
        }
    }
    best.map(|(idx, _, _)| idx)
}

fn point_to_segment_distance(p: Point2, a: Point2, b: Point2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let denom = ab.dot(ab).max(1.0e-6);
    let t = (ap.dot(ab) / denom).clamp(0.0, 1.0);
    let proj = a + ab * t;
    p.distance(proj)
}

fn cube_vertices() -> [Vec3; 8] {
    [
        Vec3::new(-0.5, -0.5, -0.5),
        Vec3::new(0.5, -0.5, -0.5),
        Vec3::new(0.5, 0.5, -0.5),
        Vec3::new(-0.5, 0.5, -0.5),
        Vec3::new(-0.5, -0.5, 0.5),
        Vec3::new(0.5, -0.5, 0.5),
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(-0.5, 0.5, 0.5),
    ]
}

fn cube_vertices_scaled(scale: f64) -> [Vec3; 8] {
    let h = 0.5 * scale;
    [
        Vec3::new(-h, -h, -h),
        Vec3::new(h, -h, -h),
        Vec3::new(h, h, -h),
        Vec3::new(-h, h, -h),
        Vec3::new(-h, -h, h),
        Vec3::new(h, -h, h),
        Vec3::new(h, h, h),
        Vec3::new(-h, h, h),
    ]
}

fn gizmo_pick_scale(rect: Rect) -> f64 {
    let size = rect.width().min(rect.height()) as f64;
    size * 0.5 * GIZMO_PICK_INSET / GIZMO_PICK_RADIUS
}

fn project_scaled_cube(rect: Rect, basis: ViewBasis, scale: f64) -> ProjectedCube {
    let vertices = cube_vertices_scaled(scale);
    project_vertices(rect, basis, &vertices)
}

fn project_vertices(rect: Rect, basis: ViewBasis, vertices: &[Vec3; 8]) -> ProjectedCube {
    let mut view = [Vec3::ZERO; 8];
    let center = rect.center();
    let scale = gizmo_pick_scale(rect);
    let mut points = [Point2::default(); 8];
    for (idx, v) in vertices.iter().enumerate() {
        let x = v.dot(basis.right);
        let y = v.dot(basis.up);
        let z = v.dot(basis.forward);
        view[idx] = Vec3::new(x, y, z);
        points[idx] = pos2(
            center.x + (x * scale) as f32,
            center.y - (y * scale) as f32,
        );
    }
    ProjectedCube { view, points }
}

fn project_cube(rect: Rect, basis: ViewBasis) -> ProjectedCube {
    project_scaled_cube(rect, basis, FACE_SCALE)
}

const EDGE_DEFS: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

fn face_defs() -> [FaceDef; 6] {
    [
        FaceDef {
            face: ViewFace::Front,
            label: "Front",
            normal: Vec3::new(0.0, -1.0, 0.0),
            indices: [0, 1, 5, 4],
            base_color: Color32::from_rgb(226, 227, 222),
        },
        FaceDef {
            face: ViewFace::Back,
            label: "Back",
            normal: Vec3::new(0.0, 1.0, 0.0),
            indices: [3, 2, 6, 7],
            base_color: Color32::from_rgb(226, 227, 222),
        },
        FaceDef {
            face: ViewFace::Right,
            label: "Right",
            normal: Vec3::new(1.0, 0.0, 0.0),
            indices: [1, 2, 6, 5],
            base_color: Color32::from_rgb(226, 227, 222),
        },
        FaceDef {
            face: ViewFace::Left,
            label: "Left",
            normal: Vec3::new(-1.0, 0.0, 0.0),
            indices: [0, 3, 7, 4],
            base_color: Color32::from_rgb(226, 227, 222),
        },
        FaceDef {
            face: ViewFace::Top,
            label: "Top",
            normal: Vec3::new(0.0, 0.0, 1.0),
            indices: [4, 5, 6, 7],
            base_color: Color32::from_rgb(236, 238, 234),
        },
        FaceDef {
            face: ViewFace::Bottom,
            label: "Bottom",
            normal: Vec3::new(0.0, 0.0, -1.0),
            indices: [0, 1, 2, 3],
            base_color: Color32::from_rgb(212, 214, 209),
        },
    ]
}

fn face_normal(face: ViewFace) -> Vec3 {
    match face {
        ViewFace::Front => Vec3::new(0.0, -1.0, 0.0),
        ViewFace::Back => Vec3::new(0.0, 1.0, 0.0),
        ViewFace::Right => Vec3::new(1.0, 0.0, 0.0),
        ViewFace::Left => Vec3::new(-1.0, 0.0, 0.0),
        ViewFace::Top => Vec3::new(0.0, 0.0, 1.0),
        ViewFace::Bottom => Vec3::new(0.0, 0.0, -1.0),
    }
}

fn face_hover_tint(face: ViewFace) -> Color32 {
    match face {
        ViewFace::Front | ViewFace::Back => Color32::from_rgb(102, 137, 239),
        ViewFace::Right | ViewFace::Left => Color32::from_rgb(17, 235, 107),
        ViewFace::Top | ViewFace::Bottom => Color32::from_rgb(250, 102, 104),
    }
}

struct FaceDef {
    face: ViewFace,
    label: &'static str,
    normal: Vec3,
    indices: [usize; 4],
    base_color: Color32,
}

struct ProjectedFace {
    face: ViewFace,
    points: [Point2; 4],
    depth: f64,
    color: Color32,
    label: &'static str,
    center: Point2,
}

struct ProjectedCube {
    view: [Vec3; 8],
    points: [Point2; 8],
}
