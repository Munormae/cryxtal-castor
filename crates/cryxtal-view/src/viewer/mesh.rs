use std::cmp::Ordering;
use std::collections::HashMap;
use truck_polymesh::PolygonMesh;

use super::math::Vec3;
use super::pick::ray_intersect_triangle;

const BVH_LEAF_SIZE: usize = 8;

#[derive(Clone, Debug)]
pub struct ViewerMesh {
    pub positions: Vec<Vec3>,
    pub tri_faces: Vec<[usize; 3]>,
    pub edges: Vec<[usize; 2]>,
    pub edge_info: Vec<EdgeInfo>,
    pub bounds: Option<(Vec3, Vec3)>,
    bvh_nodes: Vec<BvhNode>,
    bvh_indices: Vec<usize>,
}

impl ViewerMesh {
    pub fn from_mesh(mesh: &PolygonMesh) -> Self {
        let positions: Vec<Vec3> = mesh.positions().iter().copied().map(Vec3::from).collect();
        let bounds = compute_bounds(&positions);

        let mut tri_faces = Vec::new();
        tri_faces.extend(mesh.tri_faces().iter().map(|tri| [tri[0].pos, tri[1].pos, tri[2].pos]));
        for quad in mesh.quad_faces() {
            tri_faces.push([quad[0].pos, quad[1].pos, quad[2].pos]);
            tri_faces.push([quad[0].pos, quad[2].pos, quad[3].pos]);
        }
        for face in mesh.faces().other_faces() {
            if face.len() < 3 {
                continue;
            }
            for idx in 1..(face.len() - 1) {
                tri_faces.push([face[0].pos, face[idx].pos, face[idx + 1].pos]);
            }
        }

        orient_triangles_outward(&positions, tri_faces.as_mut_slice());

        let (edges, edge_info) = build_feature_edges(&positions, &tri_faces);
        let (bvh_nodes, bvh_indices) = build_bvh(&positions, &tri_faces);

        Self {
            positions,
            tri_faces,
            edges,
            edge_info,
            bounds,
            bvh_nodes,
            bvh_indices,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty() || self.tri_faces.is_empty()
    }

    pub fn edges_with_angle_threshold(&self, angle_deg: f64) -> Vec<[usize; 2]> {
        let cos_threshold = angle_deg.to_radians().cos();
        let mut edges = Vec::new();
        for info in &self.edge_info {
            let feature = if info.count == 1 || info.count > 2 {
                true
            } else {
                info.normal0.dot(info.normal1) < cos_threshold
            };
            if feature {
                edges.push([info.a, info.b]);
            }
        }
        edges
    }

    pub fn ray_pick(&self, origin: Vec3, dir: Vec3) -> Option<(f64, Vec3)> {
        if self.tri_faces.is_empty() {
            return None;
        }
        if self.bvh_nodes.is_empty() {
            return self.ray_pick_linear(origin, dir);
        }

        let mut best_t = f64::INFINITY;
        let mut best_point = None;
        let mut stack = Vec::new();
        stack.push(0usize);

        while let Some(node_idx) = stack.pop() {
            let node = &self.bvh_nodes[node_idx];
            if ray_aabb_interval(origin, dir, node.bounds, best_t).is_none() {
                continue;
            }

            if node.count > 0 {
                let start = node.start;
                let end = start + node.count;
                for &tri_idx in &self.bvh_indices[start..end] {
                    let tri = self.tri_faces[tri_idx];
                    let p0 = self.positions[tri[0]];
                    let p1 = self.positions[tri[1]];
                    let p2 = self.positions[tri[2]];
                    if let Some(t) = ray_intersect_triangle(origin, dir, p0, p1, p2) {
                        if t < best_t {
                            best_t = t;
                            best_point = Some(origin + dir * t);
                        }
                    }
                }
                continue;
            }

            let left = node.left;
            let right = node.right;
            let left_hit = left.and_then(|idx| {
                ray_aabb_interval(origin, dir, self.bvh_nodes[idx].bounds, best_t)
                    .map(|(tmin, _)| (idx, tmin))
            });
            let right_hit = right.and_then(|idx| {
                ray_aabb_interval(origin, dir, self.bvh_nodes[idx].bounds, best_t)
                    .map(|(tmin, _)| (idx, tmin))
            });

            match (left_hit, right_hit) {
                (Some((left_idx, left_t)), Some((right_idx, right_t))) => {
                    if left_t <= right_t {
                        stack.push(right_idx);
                        stack.push(left_idx);
                    } else {
                        stack.push(left_idx);
                        stack.push(right_idx);
                    }
                }
                (Some((left_idx, _)), None) => stack.push(left_idx),
                (None, Some((right_idx, _))) => stack.push(right_idx),
                (None, None) => {}
            }
        }

        best_point.map(|point| (best_t, point))
    }

    pub fn merge(meshes: &[ViewerMesh]) -> Option<Self> {
        let mut positions = Vec::new();
        let mut tri_faces = Vec::new();
        let mut edges = Vec::new();
        let mut edge_info = Vec::new();
        let mut bounds: Option<(Vec3, Vec3)> = None;

        for mesh in meshes {
            if mesh.is_empty() {
                continue;
            }

            let offset = positions.len();
            positions.extend(mesh.positions.iter().copied());
            tri_faces.extend(mesh.tri_faces.iter().map(|tri| {
                [tri[0] + offset, tri[1] + offset, tri[2] + offset]
            }));
            edges.extend(
                mesh.edges
                    .iter()
                    .map(|edge| [edge[0] + offset, edge[1] + offset]),
            );
            edge_info.extend(mesh.edge_info.iter().map(|edge| EdgeInfo {
                a: edge.a + offset,
                b: edge.b + offset,
                normal0: edge.normal0,
                normal1: edge.normal1,
                count: edge.count,
                feature: edge.feature,
            }));

            bounds = match (bounds, mesh.bounds) {
                (None, some) => some,
                (Some(acc), None) => Some(acc),
                (Some((min, max)), Some((other_min, other_max))) => {
                    Some((min.min(other_min), max.max(other_max)))
                }
            };
        }

        if positions.is_empty() || tri_faces.is_empty() {
            None
        } else {
            Some(Self {
                positions,
                tri_faces,
                edges,
                edge_info,
                bounds,
                bvh_nodes: Vec::new(),
                bvh_indices: Vec::new(),
            })
        }
    }

    fn ray_pick_linear(&self, origin: Vec3, dir: Vec3) -> Option<(f64, Vec3)> {
        let mut best_t = f64::INFINITY;
        let mut best_point = None;

        for tri in &self.tri_faces {
            let p0 = self.positions[tri[0]];
            let p1 = self.positions[tri[1]];
            let p2 = self.positions[tri[2]];
            if let Some(t) = ray_intersect_triangle(origin, dir, p0, p1, p2) {
                if t < best_t {
                    best_t = t;
                    best_point = Some(origin + dir * t);
                }
            }
        }

        best_point.map(|point| (best_t, point))
    }
}

#[derive(Clone, Copy, Debug)]
struct BvhNode {
    bounds: (Vec3, Vec3),
    left: Option<usize>,
    right: Option<usize>,
    start: usize,
    count: usize,
}

fn build_feature_edges(
    positions: &[Vec3],
    tri_faces: &[[usize; 3]],
) -> (Vec<[usize; 2]>, Vec<EdgeInfo>) {
    let mut edge_map: HashMap<(usize, usize), EdgeEntry> = HashMap::new();
    let cos_threshold = (8.0_f64.to_radians()).cos();
    let mesh_center = average_point(positions);

    for tri in tri_faces {
        let p0 = positions[tri[0]];
        let p1 = positions[tri[1]];
        let p2 = positions[tri[2]];
        let mut normal = (p1 - p0).cross(p2 - p0);
        let len = normal.length();
        if len <= 1.0e-8 {
            continue;
        }
        let tri_center = (p0 + p1 + p2) * (1.0 / 3.0);
        if normal.dot(tri_center - mesh_center) < 0.0 {
            normal = -normal;
        }
        let normal = normal / len;

        for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let (min, max) = if a <= b { (a, b) } else { (b, a) };
            match edge_map.get_mut(&(min, max)) {
                Some(entry) => {
                    if entry.count == u8::MAX {
                        entry.keep = true;
                    } else {
                        entry.count += 1;
                    }
                    if entry.count == 2 {
                        entry.normal1 = normal;
                        let dot = entry.normal0.dot(normal);
                        entry.keep = dot < cos_threshold;
                    } else {
                        entry.keep = true;
                    }
                }
                None => {
                    edge_map.insert(
                        (min, max),
                        EdgeEntry {
                            normal0: normal,
                            normal1: Vec3::ZERO,
                            count: 1,
                            keep: true,
                        },
                    );
                }
            }
        }
    }

    let mut edge_info = Vec::new();
    let mut edges = Vec::new();
    for ((a, b), entry) in edge_map {
        let feature = entry.count == 1 || entry.keep || entry.count > 2;
        edge_info.push(EdgeInfo {
            a,
            b,
            normal0: entry.normal0,
            normal1: entry.normal1,
            count: entry.count,
            feature,
        });
        if feature {
            edges.push([a, b]);
        }
    }
    (edges, edge_info)
}

fn build_bvh(
    positions: &[Vec3],
    tri_faces: &[[usize; 3]],
) -> (Vec<BvhNode>, Vec<usize>) {
    if tri_faces.is_empty() || positions.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut tri_bounds = Vec::with_capacity(tri_faces.len());
    let mut centroids = Vec::with_capacity(tri_faces.len());
    for tri in tri_faces {
        let p0 = positions[tri[0]];
        let p1 = positions[tri[1]];
        let p2 = positions[tri[2]];
        let min = p0.min(p1).min(p2);
        let max = p0.max(p1).max(p2);
        tri_bounds.push((min, max));
        centroids.push((p0 + p1 + p2) * (1.0 / 3.0));
    }

    let mut indices: Vec<usize> = (0..tri_faces.len()).collect();
    let mut nodes = Vec::new();
    let mut out_indices = Vec::with_capacity(tri_faces.len());
    build_bvh_node(
        &mut indices,
        &tri_bounds,
        &centroids,
        &mut nodes,
        &mut out_indices,
    );
    (nodes, out_indices)
}

fn build_bvh_node(
    indices: &mut [usize],
    tri_bounds: &[(Vec3, Vec3)],
    centroids: &[Vec3],
    nodes: &mut Vec<BvhNode>,
    out_indices: &mut Vec<usize>,
) -> usize {
    let node_index = nodes.len();
    let bounds = bounds_for_indices(indices, tri_bounds);
    nodes.push(BvhNode {
        bounds,
        left: None,
        right: None,
        start: 0,
        count: 0,
    });

    if indices.len() <= BVH_LEAF_SIZE {
        let start = out_indices.len();
        let len = indices.len();
        out_indices.extend_from_slice(&indices[..len]);
        nodes[node_index].start = start;
        nodes[node_index].count = len;
        return node_index;
    }

    let (cmin, cmax) = centroid_bounds(indices, centroids);
    let extent = cmax - cmin;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    indices.sort_unstable_by(|a, b| {
        axis_value(centroids[*a], axis)
            .partial_cmp(&axis_value(centroids[*b], axis))
            .unwrap_or(Ordering::Equal)
    });
    let mid = indices.len() / 2;
    let (left, right) = indices.split_at_mut(mid);
    let left_idx = build_bvh_node(left, tri_bounds, centroids, nodes, out_indices);
    let right_idx = build_bvh_node(right, tri_bounds, centroids, nodes, out_indices);
    nodes[node_index].left = Some(left_idx);
    nodes[node_index].right = Some(right_idx);
    node_index
}

fn bounds_for_indices(indices: &[usize], tri_bounds: &[(Vec3, Vec3)]) -> (Vec3, Vec3) {
    let (mut min, mut max) = tri_bounds[indices[0]];
    for &idx in &indices[1..] {
        let (bmin, bmax) = tri_bounds[idx];
        min = min.min(bmin);
        max = max.max(bmax);
    }
    (min, max)
}

fn centroid_bounds(indices: &[usize], centroids: &[Vec3]) -> (Vec3, Vec3) {
    let mut min = centroids[indices[0]];
    let mut max = min;
    for &idx in &indices[1..] {
        let c = centroids[idx];
        min = min.min(c);
        max = max.max(c);
    }
    (min, max)
}

fn axis_value(value: Vec3, axis: usize) -> f64 {
    match axis {
        0 => value.x,
        1 => value.y,
        _ => value.z,
    }
}

fn ray_aabb_interval(
    origin: Vec3,
    dir: Vec3,
    bounds: (Vec3, Vec3),
    max_t: f64,
) -> Option<(f64, f64)> {
    let (min, max) = bounds;
    let mut tmin: f64 = 0.0;
    let mut tmax: f64 = max_t;

    let mut check_axis = |origin: f64, dir: f64, min: f64, max: f64| -> bool {
        if dir.abs() <= 1.0e-9 {
            return origin >= min && origin <= max;
        }
        let inv = 1.0 / dir;
        let t1 = (min - origin) * inv;
        let t2 = (max - origin) * inv;
        let axis_min = t1.min(t2);
        let axis_max = t1.max(t2);
        tmin = tmin.max(axis_min);
        tmax = tmax.min(axis_max);
        tmax >= tmin
    };

    if !check_axis(origin.x, dir.x, min.x, max.x) {
        return None;
    }
    if !check_axis(origin.y, dir.y, min.y, max.y) {
        return None;
    }
    if !check_axis(origin.z, dir.z, min.z, max.z) {
        return None;
    }
    if tmax < 0.0 {
        return None;
    }
    Some((tmin, tmax))
}

struct EdgeEntry {
    normal0: Vec3,
    normal1: Vec3,
    count: u8,
    keep: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct EdgeInfo {
    pub a: usize,
    pub b: usize,
    pub normal0: Vec3,
    pub normal1: Vec3,
    pub count: u8,
    pub feature: bool,
}

fn compute_bounds(points: &[Vec3]) -> Option<(Vec3, Vec3)> {
    let mut iter = points.iter().copied();
    let first = iter.next()?;
    let mut min = first;
    let mut max = first;
    for p in iter {
        min = min.min(p);
        max = max.max(p);
    }
    Some((min, max))
}

fn average_point(points: &[Vec3]) -> Vec3 {
    if points.is_empty() {
        return Vec3::ZERO;
    }
    let mut sum = Vec3::ZERO;
    for p in points {
        sum = sum + *p;
    }
    sum / (points.len() as f64)
}

fn orient_triangles_outward(positions: &[Vec3], tri_faces: &mut [[usize; 3]]) {
    if positions.is_empty() {
        return;
    }
    let center = average_point(positions);
    for tri in tri_faces {
        let p0 = positions[tri[0]];
        let p1 = positions[tri[1]];
        let p2 = positions[tri[2]];
        let normal = (p1 - p0).cross(p2 - p0);
        let tri_center = (p0 + p1 + p2) * (1.0 / 3.0);
        if normal.dot(tri_center - center) < 0.0 {
            tri.swap(1, 2);
        }
    }
}
