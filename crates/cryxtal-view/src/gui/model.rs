use cryxtal_topology::Point3;

pub struct ModelInfo {
    pub label: String,
    pub elements: usize,
    pub vertices: usize,
    pub faces: usize,
    pub bounds: Option<(Point3, Point3)>,
}

impl Default for ModelInfo {
    fn default() -> Self {
        Self {
            label: "None".to_string(),
            elements: 0,
            vertices: 0,
            faces: 0,
            bounds: None,
        }
    }
}

pub fn mesh_bounds(points: &[Point3]) -> Option<(Point3, Point3)> {
    let mut iter = points.iter();
    let first = iter.next()?;
    let mut min = *first;
    let mut max = *first;
    for p in iter {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        min.z = min.z.min(p.z);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
        max.z = max.z.max(p.z);
    }
    Some((min, max))
}

pub fn format_point(point: &Point3) -> String {
    format!("{:.3}, {:.3}, {:.3}", point.x, point.y, point.z)
}

pub fn merge_bounds(
    a: Option<(Point3, Point3)>,
    b: Option<(Point3, Point3)>,
) -> Option<(Point3, Point3)> {
    match (a, b) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some((min_a, max_a)), Some((min_b, max_b))) => Some((
            Point3::new(
                min_a.x.min(min_b.x),
                min_a.y.min(min_b.y),
                min_a.z.min(min_b.z),
            ),
            Point3::new(
                max_a.x.max(max_b.x),
                max_a.y.max(max_b.y),
                max_a.z.max(max_b.z),
            ),
        )),
    }
}
