use super::math::Vec3;
use super::ui::Point2;

pub fn point_in_triangle(p: Point2, a: Point2, b: Point2, c: Point2) -> bool {
    let ab = b - a;
    let ap = p - a;
    let bc = c - b;
    let bp = p - b;
    let ca = a - c;
    let cp = p - c;

    let d1 = ab.x * ap.y - ab.y * ap.x;
    let d2 = bc.x * bp.y - bc.y * bp.x;
    let d3 = ca.x * cp.y - ca.y * cp.x;

    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

pub fn ray_intersect_triangle(
    origin: Vec3,
    dir: Vec3,
    a: Vec3,
    b: Vec3,
    c: Vec3,
) -> Option<f64> {
    let eps = 1.0e-9;
    let edge1 = b - a;
    let edge2 = c - a;
    let pvec = dir.cross(edge2);
    let det = edge1.dot(pvec);
    if det.abs() < eps {
        return None;
    }
    let inv_det = 1.0 / det;
    let tvec = origin - a;
    let u = tvec.dot(pvec) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qvec = tvec.cross(edge1);
    let v = dir.dot(qvec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = edge2.dot(qvec) * inv_det;
    if t > eps {
        Some(t)
    } else {
        None
    }
}
