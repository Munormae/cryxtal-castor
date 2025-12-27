use cryxtal_bim::BimElement;

use crate::elements::opening_outline_points;
use crate::viewer::{Color32, OverlayPainter, Rect, Stroke, ViewerMesh, ViewerState};

pub(super) fn paint_hover_outline(
    viewer: &ViewerState,
    painter: &mut impl OverlayPainter,
    rect: Rect,
    meshes: &[ViewerMesh],
    elements: &[BimElement],
    hovered: Option<usize>,
    selected: Option<usize>,
    visibility: &[bool],
) {
    let mut indices = Vec::new();
    if let Some(idx) = hovered {
        indices.push(idx);
    }
    if let Some(idx) = selected {
        if !indices.contains(&idx) {
            indices.push(idx);
        }
    }

    for idx in indices {
        let visible = visibility.get(idx).copied().unwrap_or(true);
        if visible {
            continue;
        }
        let Some(mesh) = meshes.get(idx) else {
            continue;
        };

        let is_selected = Some(idx) == selected;
        let (main, outline) = if is_selected {
            (
                Color32::from_rgba_unmultiplied(255, 210, 90, 180),
                Color32::from_rgba_unmultiplied(10, 8, 6, 140),
            )
        } else {
            (
                Color32::from_rgba_unmultiplied(70, 230, 255, 150),
                Color32::from_rgba_unmultiplied(10, 8, 6, 120),
            )
        };
        let outer = Stroke::new(3.4, outline);
        let inner = Stroke::new(2.2, main);

        let element = elements.get(idx);
        let mut handled = false;
        if let Some(opening) = element {
            if let Some(points) = opening_outline_points(opening, elements) {
                handled = draw_opening_outline(viewer, painter, rect, &points, outer, inner);
            }
        }
        if !handled {
            draw_mesh_edges(viewer, painter, rect, mesh, outer, inner);
        }
    }
}

fn draw_opening_outline(
    viewer: &ViewerState,
    painter: &mut impl OverlayPainter,
    rect: Rect,
    points: &[cryxtal_topology::Point3; 4],
    outer: Stroke,
    inner: Stroke,
) -> bool {
    let mut screen_points = Vec::with_capacity(points.len());
    for point in points.iter().copied() {
        let Some(screen) = viewer.project_point3(point, rect) else {
            return false;
        };
        screen_points.push(screen);
    }

    let transparent = Color32::from_rgba_unmultiplied(0, 0, 0, 0);
    painter.polygon(screen_points.clone(), transparent, outer);
    painter.polygon(screen_points, transparent, inner);
    true
}

fn draw_mesh_edges(
    viewer: &ViewerState,
    painter: &mut impl OverlayPainter,
    rect: Rect,
    mesh: &ViewerMesh,
    outer: Stroke,
    inner: Stroke,
) {
    for edge in &mesh.edges {
        let (Some(a), Some(b)) = (mesh.positions.get(edge[0]), mesh.positions.get(edge[1])) else {
            continue;
        };
        let Some(start) = viewer.project_point(*a, rect) else {
            continue;
        };
        let Some(end) = viewer.project_point(*b, rect) else {
            continue;
        };
        painter.line_segment(start, end, outer);
        painter.line_segment(start, end, inner);
    }
}
