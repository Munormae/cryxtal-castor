use super::axis_gizmo::{draw as draw_axis_gizmo, pick_target as pick_axis_target};
use super::input::ViewerInput;
use super::math::{Vec3, rotate_around_axis};
use super::mesh::ViewerMesh;
use super::overlay::OverlayPainter;
use super::pivot::PivotState;
use super::ui::{Align2, Color32, Point2, Rect, Stroke, Vec2, pos2, vec2};
use super::viewcube::{ViewBasis, draw as draw_viewcube, pick_target as pick_viewcube_target, view_direction_from_normal};
use cryxtal_topology::Point3;

#[derive(Clone, Copy, Debug)]
struct CameraBasis {
    pos: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
}

#[derive(Clone, Copy, Debug)]
struct ViewTransition {
    from_forward: Vec3,
    from_up: Vec3,
    to_forward: Vec3,
    to_up: Vec3,
    elapsed: f64,
    duration: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Skeleton,
    LayerOpaque,
    LayerTransparent,
    Material,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoMode {
    Cube,
    Axis,
}

impl Default for GizmoMode {
    fn default() -> Self {
        Self::Cube
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapKind {
    Vertex,
    EdgeMidpoint,
    FaceCenter,
}

#[derive(Clone, Copy, Debug)]
struct SnapHit {
    kind: SnapKind,
    world: Vec3,
    screen: Point2,
    distance: f32,
    depth: f64,
}

#[derive(Clone, Copy, Debug)]
struct SnapCache {
    pos: Point2,
    rect: Rect,
    camera_pos: Vec3,
    camera_target: Vec3,
    camera_up: Vec3,
    hit: Option<SnapHit>,
}

const GIZMO_DRAG_THRESHOLD: f32 = 2.0;
const GIZMO_DRAG_SPEED: f64 = 0.015;

#[derive(Clone, Debug)]
pub struct ViewerState {
    target: Vec3,
    camera_pos: Vec3,
    camera_up: Vec3,
    pivot: PivotState,
    fov_deg: f64,
    view_transition: Option<ViewTransition>,
    snap_cache: Option<SnapCache>,
    gizmo_mode: GizmoMode,
    gizmo_drag_active: bool,
    gizmo_drag_pos: Option<Point2>,
    gizmo_dragged: bool,
}

impl Default for ViewerState {
    fn default() -> Self {
        let yaw: f64 = 0.6;
        let pitch: f64 = 0.35;
        let distance: f64 = 500.0;
        let forward =
            Vec3::new(yaw.cos() * pitch.cos(), yaw.sin() * pitch.cos(), pitch.sin()).normalized();
        let target = Vec3::ZERO;
        let camera_pos = target - forward * distance;
        let camera_up = Self::default_up(forward);
        Self {
            target,
            camera_pos,
            camera_up,
            pivot: PivotState::default(),
            fov_deg: 60.0,
            view_transition: None,
            snap_cache: None,
            gizmo_mode: GizmoMode::Cube,
            gizmo_drag_active: false,
            gizmo_drag_pos: None,
            gizmo_dragged: false,
        }
    }
}

impl ViewerState {
    pub fn reset_view(&mut self) {
        let gizmo_mode = self.gizmo_mode;
        *self = Self::default();
        self.gizmo_mode = gizmo_mode;
    }

    pub fn gizmo_mode(&self) -> GizmoMode {
        self.gizmo_mode
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        if self.gizmo_mode != mode {
            self.gizmo_mode = mode;
            self.gizmo_drag_active = false;
            self.gizmo_drag_pos = None;
            self.gizmo_dragged = false;
        }
    }

    pub fn fit_bounds(&mut self, bounds: (Vec3, Vec3)) {
        let center = (bounds.0 + bounds.1) * 0.5;
        let size = bounds.1 - bounds.0;
        let radius = size.max_component().max(1.0) * 0.5;
        self.target = center;
        self.pivot.set_position(center);
        let forward = self.forward();
        self.camera_pos = self.target - forward * (radius * 3.0).max(10.0);
        self.camera_up = Self::default_up(forward);
    }

    pub fn update(&mut self, dt: f64) -> bool {
        self.update_view_transition(dt);
        self.view_transition.is_some()
    }

    pub fn invalidate_snap_cache(&mut self) {
        self.snap_cache = None;
    }

    pub fn handle_input(&mut self, input: &ViewerInput, meshes: &[ViewerMesh]) -> bool {
        let basis = self.camera_basis();
        let ctrl = input.modifiers.ctrl;

        let gizmo_rect = self.gizmo_rect(input.rect);
        let pointer_pos = input.pointer_pos;

        if self.gizmo_drag_active {
            if !input.primary_down {
                if !self.gizmo_dragged {
                    if let Some(pos) = pointer_pos {
                        if gizmo_rect.contains(pos) {
                            let gizmo_basis = self.gizmo_basis(&basis);
                            match self.gizmo_mode {
                                GizmoMode::Cube => {
                                    if let Some(pick) =
                                        pick_viewcube_target(pos, input.rect, gizmo_basis)
                                    {
                                        let forward = view_direction_from_normal(pick.normal);
                                        self.begin_view_transition(forward);
                                    }
                                }
                                GizmoMode::Axis => {
                                    if let Some(pick) =
                                        pick_axis_target(pos, input.rect, gizmo_basis)
                                    {
                                        self.begin_view_transition(pick.forward);
                                    }
                                }
                            }
                        }
                    }
                }
                self.gizmo_drag_active = false;
                self.gizmo_drag_pos = None;
                self.gizmo_dragged = false;
                return true;
            }

            if let Some(pos) = pointer_pos {
                if let Some(last) = self.gizmo_drag_pos {
                    let delta = pos - last;
                    if delta.length() > 0.0 {
                        if delta.length() > GIZMO_DRAG_THRESHOLD {
                            self.gizmo_dragged = true;
                        }
                        let yaw_delta = -(delta.x as f64) * GIZMO_DRAG_SPEED;
                        let pitch_delta = -(delta.y as f64) * GIZMO_DRAG_SPEED;
                        self.orbit_pivot(yaw_delta, pitch_delta);
                    }
                }
                self.gizmo_drag_pos = Some(pos);
            }
            return true;
        }

        if input.primary_down {
            if let Some(pos) = pointer_pos {
                if gizmo_rect.contains(pos) {
                    self.gizmo_drag_active = true;
                    self.gizmo_drag_pos = Some(pos);
                    self.gizmo_dragged = false;
                    return true;
                }
            }
        }

        let delta = input.pointer_delta;
        let dragging = delta.x.abs() > 0.0 || delta.y.abs() > 0.0;

        if input.middle_down && ctrl && dragging {
            self.cancel_view_transition();
            let yaw_delta = -(delta.x as f64) * 0.01;
            let pitch_delta = -(delta.y as f64) * 0.01;
            self.orbit_pivot(yaw_delta, pitch_delta);
        } else if (input.middle_down && dragging) || input.secondary_down {
            if dragging {
                self.cancel_view_transition();
                let scale = self.distance_internal() * 0.002;
                let delta_world =
                    -basis.right * (delta.x as f64 * scale)
                        + basis.up * (delta.y as f64 * scale);
                self.target = self.target + delta_world;
                self.camera_pos = self.camera_pos + delta_world;
            }
        }

        if input.hovered {
            let scroll = input.scroll_delta;
            if scroll != 0.0 {
                self.cancel_view_transition();
                let zoom = (-scroll as f64 * 0.01).exp();
                let distance = self.distance_internal().clamp(1.0, 1.0e7);
                let forward = self.forward();
                let basis = self.camera_basis();
                let scale = self.view_scale(input.rect);
                let cursor = input.pointer_pos;
                let before = cursor.and_then(|pos| {
                    screen_point_on_plane(
                        pos,
                        input.rect,
                        &basis,
                        scale,
                        self.camera_pos,
                        self.target,
                        forward,
                    )
                });
                let new_distance = (distance * zoom).clamp(1.0, 1.0e7);
                self.camera_pos = self.target - forward * new_distance;
                if let (Some(pos), Some(before)) = (cursor, before) {
                    if let Some(after) = screen_point_on_plane(
                        pos,
                        input.rect,
                        &basis,
                        scale,
                        self.camera_pos,
                        self.target,
                        forward,
                    ) {
                        let delta = before - after;
                        self.target = self.target + delta;
                        self.camera_pos = self.camera_pos + delta;
                    }
                }
            }
        }

        if input.key_v_pressed {
            self.pivot.arm_pick();
        }
        let v_active = self.pivot.is_pick_active(input.key_v_down);
        if v_active && input.primary_clicked {
            if let Some(pos) = input.pointer_pos {
                if input.rect.contains(pos) {
                    if let Some(pivot) = self.pick_point(pos, input.rect, meshes, v_active) {
                        self.pivot.set_position(pivot);
                        self.pivot.disarm_pick();
                    }
                }
            }
            return true;
        }
        false
    }

    pub fn project_point(&self, point: Vec3, rect: Rect) -> Option<Point2> {
        let basis = self.camera_basis();
        let scale = self.view_scale(rect);
        self.project(point, rect, &basis, scale).map(|(pos, _)| pos)
    }

    pub fn project_point3(&self, point: Point3, rect: Rect) -> Option<Point2> {
        self.project_point(Vec3::from(point), rect)
    }

    pub fn paint_overlay<P: OverlayPainter>(
        &mut self,
        painter: &mut P,
        rect: Rect,
        meshes: &[ViewerMesh],
        selected: Option<usize>,
        view_mode: ViewMode,
        snap_active: bool,
        pointer_pos: Option<Point2>,
        draw_gizmo: bool,
    ) {
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(60)));

        let basis = self.camera_basis();
        let scale = self.view_scale(rect);
        for (idx, mesh) in meshes.iter().enumerate() {
            if Some(idx) == selected {
                self.draw_selection_handles(painter, rect, &basis, scale, mesh);
            }
        }

        self.pivot
            .draw(painter, |point| self.project(point, rect, &basis, scale));

        let gizmo_basis = self.gizmo_basis(&basis);
        if draw_gizmo {
            match self.gizmo_mode {
                GizmoMode::Cube => {
                    let hover_target = pointer_pos
                        .and_then(|pos| pick_viewcube_target(pos, rect, gizmo_basis))
                        .map(|pick| pick.target);
                    draw_viewcube(painter, rect, gizmo_basis, hover_target);
                }
                GizmoMode::Axis => {
                    let hover_target = pointer_pos
                        .and_then(|pos| pick_axis_target(pos, rect, gizmo_basis))
                        .map(|pick| pick.target);
                    draw_axis_gizmo(painter, rect, gizmo_basis, hover_target);
                }
            }
        }

        let gizmo_rect = self.gizmo_rect(rect);
        if let Some(pos) = pointer_pos {
            if rect.contains(pos) {
                let snap = if snap_active && !gizmo_rect.contains(pos) {
                    self.cached_snap(pos, rect, &basis, scale, meshes)
                } else {
                    None
                };
                if let Some(hit) = snap {
                    self.draw_snap_indicator(painter, hit);
                }
                self.draw_cursor(painter, pos, snap.is_some());
                if let Some(hit) = snap {
                    self.draw_snap_label(painter, hit);
                }
            }
        }

        let mut hint = format!(
            "Left click/drag: select | Ctrl+middle-drag: rotate | Middle-drag/Right-drag: pan | Wheel: zoom | V: pick pivot | Ctrl+1..4: {}",
            view_mode_label(view_mode)
        );
        if view_mode == ViewMode::Material {
            hint.push_str(" (n/a)");
        }
        hint.push_str(" | Esc: cancel tool");
        painter.text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            Align2::LeftTop,
            hint,
            12.0,
            Color32::from_gray(120),
        );
    }

    pub fn distance(&self) -> f64 {
        self.distance_internal()
    }

    pub fn pivot_position(&self) -> Vec3 {
        self.pivot.position()
    }

    pub fn is_pivot_pick_active(&self, key_v_down: bool) -> bool {
        self.pivot.is_pick_active(key_v_down)
    }

    pub fn camera_position(&self) -> Vec3 {
        self.camera_pos
    }

    pub fn camera_target(&self) -> Vec3 {
        self.target
    }

    pub fn camera_up(&self) -> Vec3 {
        self.camera_up
    }

    pub fn fov_deg(&self) -> f64 {
        self.fov_deg
    }

    fn camera_basis(&self) -> CameraBasis {
        let forward = self.forward();
        let mut right = forward.cross(self.camera_up);
        if right.length() <= 1.0e-6 {
            let up = Self::default_up(forward);
            right = forward.cross(up);
        }
        right = right.normalized();
        let up = right.cross(forward).normalized();
        let pos = self.camera_pos;
        CameraBasis {
            pos,
            right,
            up,
            forward,
        }
    }

    fn project(
        &self,
        point: Vec3,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
    ) -> Option<(Point2, f64)> {
        let rel = point - basis.pos;
        let camera = Vec3::new(
            rel.dot(basis.right),
            rel.dot(basis.up),
            rel.dot(basis.forward),
        );
        project_camera(camera, rect.center(), scale, 1.0e-4)
    }

    fn draw_selection_handles(
        &self,
        painter: &mut impl OverlayPainter,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
        mesh: &ViewerMesh,
    ) {
        let size = 6.0;
        let fill = Color32::from_rgba_unmultiplied(255, 230, 140, 40);
        let stroke = Stroke::new(1.4, Color32::from_rgb(255, 210, 90));

        if !mesh.edges.is_empty() {
            let mut used = vec![false; mesh.positions.len()];
            for edge in &mesh.edges {
                if let Some(slot) = used.get_mut(edge[0]) {
                    *slot = true;
                }
                if let Some(slot) = used.get_mut(edge[1]) {
                    *slot = true;
                }
            }

            for (idx, point) in mesh.positions.iter().enumerate() {
                if !used[idx] {
                    continue;
                }
                let Some((pos, _)) = self.project(*point, rect, basis, scale) else {
                    continue;
                };
                let handle_rect = Rect::from_center_size(pos, vec2(size, size));
                painter.rect_filled(handle_rect, 1.0, fill);
                painter.rect_stroke(handle_rect, 1.0, stroke);
            }
            return;
        }

        if let Some(bounds) = mesh.bounds {
            let (min, max) = bounds;
            let corners = [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(min.x, max.y, max.z),
            ];
            for corner in corners {
                let Some((pos, _)) = self.project(corner, rect, basis, scale) else {
                    continue;
                };
                let handle_rect = Rect::from_center_size(pos, vec2(size, size));
                painter.rect_filled(handle_rect, 1.0, fill);
                painter.rect_stroke(handle_rect, 1.0, stroke);
            }
        }
    }

    fn bounds_screen_rect(
        &self,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
        bounds: (Vec3, Vec3),
    ) -> Option<(Rect, f64)> {
        let (min, max) = bounds;
        let corners = [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ];

        let mut any = false;
        let mut min_screen = Point2::new(f32::INFINITY, f32::INFINITY);
        let mut max_screen = Point2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut min_depth = f64::INFINITY;

        for corner in corners {
            if let Some((pos, depth)) = self.project(corner, rect, basis, scale) {
                any = true;
                min_screen = Point2::new(min_screen.x.min(pos.x), min_screen.y.min(pos.y));
                max_screen = Point2::new(max_screen.x.max(pos.x), max_screen.y.max(pos.y));
                if depth < min_depth {
                    min_depth = depth;
                }
            }
        }

        if !any {
            return None;
        }

        Some((Rect { min: min_screen, max: max_screen }, min_depth))
    }

    pub fn pick_element(
        &self,
        pos: Point2,
        rect: Rect,
        meshes: &[ViewerMesh],
    ) -> Option<(usize, Vec3)> {
        self.pick_mesh_point(pos, rect, meshes)
            .map(|(idx, _, point)| (idx, point))
    }

    pub fn pick_element_rect(
        &self,
        rect: Rect,
        selection: Rect,
        meshes: &[ViewerMesh],
    ) -> Option<usize> {
        if selection.width() <= 0.0 || selection.height() <= 0.0 {
            return None;
        }

        let basis = self.camera_basis();
        let scale = self.view_scale(rect);
        let mut best: Option<(usize, f64)> = None;

        for (idx, mesh) in meshes.iter().enumerate() {
            let Some(bounds) = mesh.bounds else {
                continue;
            };
            let Some((screen_rect, depth)) =
                self.bounds_screen_rect(rect, &basis, scale, bounds)
            else {
                continue;
            };

            if selection.intersects(screen_rect) {
                match best {
                    Some((_, best_depth)) if depth >= best_depth => {}
                    _ => best = Some((idx, depth)),
                }
            }
        }

        best.map(|(idx, _)| idx)
    }

    pub fn pick_point(
        &self,
        pos: Point2,
        rect: Rect,
        meshes: &[ViewerMesh],
        snap_active: bool,
    ) -> Option<Vec3> {
        let basis = self.camera_basis();
        let scale = self.view_scale(rect);

        if snap_active {
            if let Some(snap) = self.pick_snap(pos, rect, &basis, scale, meshes) {
                return Some(snap.world);
            }
        }

        if let Some((_, _, point)) = self.pick_mesh_point(pos, rect, meshes) {
            return Some(point);
        }

        self.pick_on_plane(pos, rect, &basis, scale, self.pivot.position().z)
    }

    fn pick_on_plane(
        &self,
        pos: Point2,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
        plane_z: f64,
    ) -> Option<Vec3> {
        let center = rect.center();
        let dx = (pos.x - center.x) as f64 / scale;
        let dy = (center.y - pos.y) as f64 / scale;
        let origin = basis.pos + basis.right * dx + basis.up * dy;
        let dir = basis.forward;
        if dir.z.abs() <= 1.0e-6 {
            return None;
        }
        let t = (plane_z - origin.z) / dir.z;
        if t <= 0.0 {
            return None;
        }
        Some(origin + dir * t)
    }

    fn pick_mesh_point(
        &self,
        pos: Point2,
        rect: Rect,
        meshes: &[ViewerMesh],
    ) -> Option<(usize, f64, Vec3)> {
        let basis = self.camera_basis();
        let scale = self.view_scale(rect);
        let (origin, dir) = self.screen_ray(pos, rect, &basis, scale)?;
        let mut best: Option<(usize, f64, Vec3)> = None;

        for (mesh_idx, mesh) in meshes.iter().enumerate() {
            if let Some((t, point)) = mesh.ray_pick(origin, dir) {
                match best {
                    Some((_, best_t, _)) if t >= best_t => {}
                    _ => best = Some((mesh_idx, t, point)),
                }
            }
        }

        best
    }

    fn pick_snap(
        &self,
        pos: Point2,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
        meshes: &[ViewerMesh],
    ) -> Option<SnapHit> {
        if !rect.contains(pos) {
            return None;
        }

        let mut best: Option<SnapHit> = None;
        let mut consider = |kind: SnapKind, world: Vec3, screen: Point2, depth: f64| {
            let radius = snap_radius(kind);
            let distance = pos.distance(screen);
            if distance > radius {
                return;
            }

            let candidate = SnapHit {
                kind,
                world,
                screen,
                distance,
                depth,
            };

            match best {
                None => best = Some(candidate),
                Some(current) => {
                    if candidate.distance < current.distance - 0.1 {
                        best = Some(candidate);
                    } else if (candidate.distance - current.distance).abs() <= 0.1 {
                        let candidate_priority = snap_priority(candidate.kind);
                        let current_priority = snap_priority(current.kind);
                        if candidate_priority < current_priority
                            || (candidate_priority == current_priority && candidate.depth < current.depth)
                        {
                            best = Some(candidate);
                        }
                    }
                }
            }
        };

        let pad = 10.0;
        for mesh in meshes {
            if let Some(bounds) = mesh.bounds {
                if let Some((screen_rect, _)) = self.bounds_screen_rect(rect, basis, scale, bounds)
                {
                    let expanded = Rect {
                        min: Point2::new(screen_rect.min.x - pad, screen_rect.min.y - pad),
                        max: Point2::new(screen_rect.max.x + pad, screen_rect.max.y + pad),
                    };
                    if !expanded.contains(pos) {
                        continue;
                    }
                }
            }
            for point in &mesh.positions {
                if let Some((screen, depth)) = self.project(*point, rect, basis, scale) {
                    consider(SnapKind::Vertex, *point, screen, depth);
                }
            }

            for edge in &mesh.edges {
                let a = mesh.positions[edge[0]];
                let b = mesh.positions[edge[1]];
                let mid = (a + b) * 0.5;
                if let Some((screen, depth)) = self.project(mid, rect, basis, scale) {
                    consider(SnapKind::EdgeMidpoint, mid, screen, depth);
                }
            }

            for tri in &mesh.tri_faces {
                let p0 = mesh.positions[tri[0]];
                let p1 = mesh.positions[tri[1]];
                let p2 = mesh.positions[tri[2]];
                let center = (p0 + p1 + p2) * (1.0 / 3.0);
                if let Some((screen, depth)) = self.project(center, rect, basis, scale) {
                    consider(SnapKind::FaceCenter, center, screen, depth);
                }
            }
        }

        best
    }

    fn cached_snap(
        &mut self,
        pos: Point2,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
        meshes: &[ViewerMesh],
    ) -> Option<SnapHit> {
        if let Some(cache) = self.snap_cache {
            if cache.pos == pos
                && cache.rect == rect
                && same_vec3(cache.camera_pos, self.camera_pos)
                && same_vec3(cache.camera_target, self.target)
                && same_vec3(cache.camera_up, self.camera_up)
            {
                return cache.hit;
            }
        }

        let hit = self.pick_snap(pos, rect, basis, scale, meshes);
        self.snap_cache = Some(SnapCache {
            pos,
            rect,
            camera_pos: self.camera_pos,
            camera_target: self.target,
            camera_up: self.camera_up,
            hit,
        });
        hit
    }

    fn view_scale(&self, rect: Rect) -> f64 {
        let view_size = rect.width().min(rect.height()) as f64;
        let fov = self.fov_deg.to_radians();
        let persp = view_size / (2.0 * (fov * 0.5).tan());
        (persp / self.distance_internal().max(1.0)).max(1.0e-6)
    }

    fn orbit_pivot(&mut self, yaw_delta: f64, pitch_delta: f64) {
        let pivot = self.pivot.position();
        let mut pos = self.camera_pos;
        let mut target = self.target;
        let world_up = Vec3::new(0.0, 0.0, 1.0);

        if yaw_delta != 0.0 {
            pos = rotate_around_axis(pos, pivot, world_up, yaw_delta);
            target = rotate_around_axis(target, pivot, world_up, yaw_delta);
            self.camera_up = rotate_around_axis(self.camera_up, Vec3::ZERO, world_up, yaw_delta)
                .normalized();
        }
        self.camera_pos = pos;
        self.target = target;

        if pitch_delta != 0.0 {
            let basis = self.camera_basis();
            pos = rotate_around_axis(self.camera_pos, pivot, basis.right, pitch_delta);
            target = rotate_around_axis(self.target, pivot, basis.right, pitch_delta);
            self.camera_up =
                rotate_around_axis(self.camera_up, Vec3::ZERO, basis.right, pitch_delta)
                    .normalized();
            self.camera_pos = pos;
            self.target = target;
        }

        let forward = self.forward();
        let mut up = self.camera_up - forward * self.camera_up.dot(forward);
        if up.length() <= 1.0e-6 {
            up = Self::default_up(forward);
        }
        self.camera_up = up.normalized();
    }

    fn begin_view_transition(&mut self, forward: Vec3) {
        let to_forward = forward.normalized();
        if to_forward.length() <= 1.0e-6 {
            return;
        }
        let from_forward = self.forward();
        if (from_forward - to_forward).length() <= 1.0e-3 {
            self.set_view(to_forward);
            self.view_transition = None;
            return;
        }
        let from_up = self.camera_up.normalized();
        let to_up = Self::default_up(to_forward);
        self.view_transition = Some(ViewTransition {
            from_forward,
            from_up,
            to_forward,
            to_up,
            elapsed: 0.0,
            duration: 0.35,
        });
    }

    fn update_view_transition(&mut self, dt: f64) {
        let Some(transition) = self.view_transition else {
            return;
        };
        let elapsed = transition.elapsed + dt.max(0.0);
        let t = (elapsed / transition.duration).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        let forward =
            (transition.from_forward * (1.0 - smooth) + transition.to_forward * smooth)
                .normalized();
        let mut up = (transition.from_up * (1.0 - smooth) + transition.to_up * smooth).normalized();
        up = (up - forward * up.dot(forward)).normalized();
        let distance = self.distance_internal().max(1.0e-6);
        self.camera_pos = self.target - forward * distance;
        self.camera_up = up;
        if t >= 1.0 {
            self.set_view(transition.to_forward);
            self.view_transition = None;
        } else {
            self.view_transition = Some(ViewTransition { elapsed, ..transition });
        }
    }

    fn cancel_view_transition(&mut self) {
        self.view_transition = None;
    }

    pub fn cancel_interaction(&mut self) {
        self.cancel_view_transition();
        self.pivot.disarm_pick();
    }

    fn set_view(&mut self, forward: Vec3) {
        let direction = forward.normalized();
        let distance = self.distance_internal().max(1.0);
        self.camera_pos = self.target - direction * distance;
        self.camera_up = Self::default_up(direction);
    }

    fn forward(&self) -> Vec3 {
        let dir = self.target - self.camera_pos;
        if dir.length() <= f64::EPSILON {
            Vec3::new(0.0, 0.0, 1.0)
        } else {
            dir.normalized()
        }
    }

    fn distance_internal(&self) -> f64 {
        (self.target - self.camera_pos).length()
    }

    fn gizmo_basis(&self, basis: &CameraBasis) -> ViewBasis {
        ViewBasis::new(basis.right, basis.up, basis.forward)
    }

    pub fn gizmo_rect(&self, rect: Rect) -> Rect {
        match self.gizmo_mode {
            GizmoMode::Cube => super::viewcube::rect(rect),
            GizmoMode::Axis => super::axis_gizmo::rect(rect),
        }
    }

    fn draw_snap_indicator(&self, painter: &mut impl OverlayPainter, snap: SnapHit) {
        let center = snap.screen;
        let size = 22.0;
        let fill = Color32::from_rgba_unmultiplied(255, 210, 90, 90);
        let stroke = Stroke::new(2.2, Color32::from_rgb(255, 200, 90));
        let outline = Stroke::new(4.2, Color32::from_rgba_unmultiplied(15, 12, 8, 140));

        match snap.kind {
            SnapKind::Vertex => {
                let rect = Rect::from_center_size(center, vec2(size, size));
                painter.rect_stroke(rect, 1.0, outline);
                painter.rect_filled(rect, 1.0, fill);
                painter.rect_stroke(rect, 1.0, stroke);
            }
            SnapKind::EdgeMidpoint => {
                let r = size * 0.6;
                let points = vec![
                    center + Vec2::new(0.0, -r),
                    center + Vec2::new(r, 0.0),
                    center + Vec2::new(0.0, r),
                    center + Vec2::new(-r, 0.0),
                ];
                painter.polygon(
                    points.clone(),
                    Color32::from_rgba_unmultiplied(0, 0, 0, 0),
                    outline,
                );
                painter.polygon(points, fill, stroke);
            }
            SnapKind::FaceCenter => {
                let r = size * 0.7;
                let points = vec![
                    center + Vec2::new(0.0, -r),
                    center + Vec2::new(r * 0.866, r * 0.5),
                    center + Vec2::new(-r * 0.866, r * 0.5),
                ];
                painter.polygon(
                    points.clone(),
                    Color32::from_rgba_unmultiplied(0, 0, 0, 0),
                    outline,
                );
                painter.polygon(points, fill, stroke);
            }
        }
    }

    fn draw_cursor(&self, painter: &mut impl OverlayPainter, center: Point2, snapped: bool) {
        let len = if snapped { 39.0 } else { 27.0 };
        let box_size = if snapped { 9.0 } else { 6.75 };
        let half_box = box_size * 0.5;
        let shadow = if snapped {
            Color32::from_rgba_unmultiplied(35, 24, 12, 160)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 140)
        };
        let main = if snapped {
            Color32::from_rgb(255, 200, 90)
        } else {
            Color32::from_gray(220)
        };
        let shadow_stroke = Stroke::new(2.2, shadow);
        let stroke = Stroke::new(1.0, main);
        let segments = [
            (
                center + Vec2::new(-len, 0.0),
                center + Vec2::new(-half_box, 0.0),
            ),
            (
                center + Vec2::new(half_box, 0.0),
                center + Vec2::new(len, 0.0),
            ),
            (
                center + Vec2::new(0.0, -len),
                center + Vec2::new(0.0, -half_box),
            ),
            (
                center + Vec2::new(0.0, half_box),
                center + Vec2::new(0.0, len),
            ),
        ];
        for (start, end) in segments {
            painter.line_segment(start, end, shadow_stroke);
        }
        for (start, end) in segments {
            painter.line_segment(start, end, stroke);
        }
        let box_rect = Rect::from_center_size(center, vec2(box_size, box_size));
        painter.rect_stroke(
            box_rect,
            0.6,
            Stroke::new(1.6, Color32::from_rgba_unmultiplied(10, 8, 6, 170)),
        );
        painter.rect_stroke(box_rect, 0.6, Stroke::new(0.9, main));
    }

    fn draw_snap_label(&self, painter: &mut impl OverlayPainter, snap: SnapHit) {
        let label = snap_label(snap.kind);
        let offset = Vec2::new(12.0, -12.0);
        painter.text(
            snap.screen + offset,
            Align2::LeftTop,
            label.to_string(),
            12.0,
            Color32::from_rgb(255, 220, 170),
        );
    }

    fn screen_ray(
        &self,
        pos: Point2,
        rect: Rect,
        basis: &CameraBasis,
        scale: f64,
    ) -> Option<(Vec3, Vec3)> {
        if !rect.contains(pos) {
            return None;
        }
        let center = rect.center();
        let dx = (pos.x - center.x) as f64 / scale;
        let dy = (center.y - pos.y) as f64 / scale;
        let origin = basis.pos + basis.right * dx + basis.up * dy;
        let dir = basis.forward.normalized();
        Some((origin, dir))
    }

    fn default_up(forward: Vec3) -> Vec3 {
        let mut up = Vec3::new(0.0, 0.0, 1.0);
        let mut right = forward.cross(up);
        if right.length() <= 1.0e-6 {
            up = Vec3::new(0.0, 1.0, 0.0);
            right = forward.cross(up);
        }
        let right = right.normalized();
        right.cross(forward).normalized()
    }
}

fn snap_radius(kind: SnapKind) -> f32 {
    match kind {
        SnapKind::Vertex => 7.0,
        SnapKind::EdgeMidpoint => 7.0,
        SnapKind::FaceCenter => 9.0,
    }
}

fn same_vec3(a: Vec3, b: Vec3) -> bool {
    a.x == b.x && a.y == b.y && a.z == b.z
}

fn snap_priority(kind: SnapKind) -> u8 {
    match kind {
        SnapKind::Vertex => 0,
        SnapKind::EdgeMidpoint => 1,
        SnapKind::FaceCenter => 2,
    }
}

fn snap_label(kind: SnapKind) -> &'static str {
    match kind {
        SnapKind::Vertex => "Vertex",
        SnapKind::EdgeMidpoint => "Edge midpoint",
        SnapKind::FaceCenter => "Face center",
    }
}

fn view_mode_label(mode: ViewMode) -> &'static str {
    match mode {
        ViewMode::Skeleton => "Skeleton",
        ViewMode::LayerOpaque => "Layer Opaque",
        ViewMode::LayerTransparent => "Layer Transparent",
        ViewMode::Material => "Material",
    }
}

fn project_camera(
    camera: Vec3,
    center: Point2,
    scale: f64,
    near: f64,
) -> Option<(Point2, f64)> {
    if camera.z <= near {
        return None;
    }
    let sx = center.x + (camera.x * scale) as f32;
    let sy = center.y - (camera.y * scale) as f32;
    Some((pos2(sx, sy), camera.z))
}

fn screen_point_on_plane(
    pos: Point2,
    rect: Rect,
    basis: &CameraBasis,
    scale: f64,
    camera_pos: Vec3,
    plane_point: Vec3,
    plane_normal: Vec3,
) -> Option<Vec3> {
    if !rect.contains(pos) {
        return None;
    }
    let center = rect.center();
    let dx = (pos.x - center.x) as f64 / scale;
    let dy = (center.y - pos.y) as f64 / scale;
    let origin = camera_pos + basis.right * dx + basis.up * dy;
    let dir = basis.forward;
    let denom = dir.dot(plane_normal);
    if denom.abs() <= 1.0e-9 {
        return None;
    }
    let t = (plane_point - origin).dot(plane_normal) / denom;
    if t <= 0.0 {
        return None;
    }
    Some(origin + dir * t)
}
