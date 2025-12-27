use crate::viewer::ViewerMesh;

const REBAR_EDGE_ANGLE_DEG: f64 = 30.0;

pub(super) fn tune_rebar_wireframe(mesh: &mut ViewerMesh) {
    mesh.edges = mesh.edges_with_angle_threshold(REBAR_EDGE_ANGLE_DEG);
}
