use std::collections::HashMap;
use std::sync::Arc;

use cgmath::Quaternion;
use image::{DynamicImage, Rgba, RgbaImage};
use serde::Deserialize;
use truck_base::cgmath64::{Matrix4, Point3, SquareMatrix, Vector2, Vector3, Vector4};
use truck_base::newton::Jacobian;
use truck_platform::{
    BackendBufferConfig, Camera, DeviceHandler, Light, LightType, ProjectionMethod,
    RenderTextureConfig, Scene, SceneDescriptor, StudioConfig,
};
use truck_polymesh::{Faces, PolygonMesh, StandardAttributes, StandardVertex, Transformed};
use truck_rendimpl::{
    CreatorCreator, InstanceCreator, Material, PolygonInstance, PolygonState, WireFrameInstance,
    WireFrameState,
};

use super::math::Vec3;
use super::ui::{Point2, Rect, Color32};
use super::viewcube::{ViewFace, ViewTarget, ViewBasis, pick_target};
use super::{GizmoMode, ViewerState};

const GIZMO_GLB: &[u8] = include_bytes!("../../assets/gizmo_cube/gizmo_cube.glb");
const LABELS_LIGHT: &[u8] = include_bytes!("../../assets/gizmo_cube/labels_light.png");
const LABELS_LIGHT_HOVER: &[u8] =
    include_bytes!("../../assets/gizmo_cube/labels_light_hover.png");
const LABELS_DARK: &[u8] = include_bytes!("../../assets/gizmo_cube/labels_dark.png");
const LABELS_DARK_HOVER: &[u8] = include_bytes!("../../assets/gizmo_cube/labels_dark_hover.png");

const GIZMO_SCALE: f64 = 1.05;
const GIZMO_CAMERA_DISTANCE: f64 = 2.2;
const GIZMO_SCREEN_SIZE_FALLBACK: f64 = 1.25;
const GIZMO_CIRCLE_INSET: f64 = 0.85;

const EDGE_DARK: Color32 = Color32::from_rgb(0x36, 0x38, 0x37);
const EDGE_DARK_HOVER: Color32 = Color32::from_rgb(0xE2, 0xE3, 0xDE);
const EDGE_LIGHT: Color32 = Color32::from_rgb(0xE2, 0xE3, 0xDE);
const EDGE_LIGHT_HOVER: Color32 = Color32::from_rgb(0x99, 0x99, 0x99);
const EDGE_LINE_COLOR: Color32 = Color32::from_rgb(0x99, 0x99, 0x99);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CubePartKind {
    Face(ViewFace),
    Edge,
    Corner,
}

struct CubePart {
    name: String,
    kind: CubePartKind,
    target: Option<ViewTarget>,
    instance: PolygonInstance,
}

pub struct GizmoRenderer {
    scene: Scene,
    device: wgpu::Device,
    target: RenderTarget,
    target_revision: u64,
    creator: InstanceCreator,
    face_tex_placeholder: Arc<wgpu::Texture>,
    cube_parts: Vec<CubePart>,
    edge_lines: Option<WireFrameInstance>,
    cube_radius: f64,
    current_dark: bool,
    current_hover: Option<ViewTarget>,
    materials_dirty: bool,
    face_tex_light: Option<Arc<wgpu::Texture>>,
    face_tex_light_hover: Option<Arc<wgpu::Texture>>,
    face_tex_dark: Option<Arc<wgpu::Texture>>,
    face_tex_dark_hover: Option<Arc<wgpu::Texture>>,
}

impl GizmoRenderer {
    pub fn new(adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let initial_size = [1, 1];
        let scene_desc = SceneDescriptor {
            studio: StudioConfig {
                background: wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
                camera: Camera::default(),
                lights: vec![
                    Light {
                        position: Point3::new(2.5, 0.25, 1.0),
                        color: Vector3::new(1.0, 1.0, 1.0),
                        light_type: LightType::Point,
                    },
                    Light {
                        position: Point3::new(0.0, 0.0, 0.0),
                        color: Vector3::new(1.0, 1.0, 1.0),
                        light_type: LightType::Uniform,
                    },
                ],
            },
            backend_buffer: BackendBufferConfig {
                depth_test: true,
                sample_count: 1,
            },
            render_texture: RenderTextureConfig {
                canvas_size: (initial_size[0], initial_size[1]),
                format: wgpu::TextureFormat::Rgba8Unorm,
            },
        };
        let handler = DeviceHandler::new(adapter, device.clone(), queue);
        let scene = Scene::new(handler, &scene_desc);
        let target = RenderTarget::new(&device, initial_size);
        let creator = scene.instance_creator();
        let face_tex_placeholder = creator.create_texture(&placeholder_image());
        let (cube_parts, edge_lines, cube_radius) =
            build_cube_parts(&creator, &face_tex_placeholder);

        let mut renderer = Self {
            scene,
            device,
            target,
            target_revision: 0,
            creator,
            face_tex_placeholder,
            cube_parts,
            edge_lines,
            cube_radius,
            current_dark: false,
            current_hover: None,
            materials_dirty: true,
            face_tex_light: None,
            face_tex_light_hover: None,
            face_tex_dark: None,
            face_tex_dark_hover: None,
        };
        renderer.add_objects_to_scene();
        renderer
    }

    pub fn render(
        &mut self,
        rect: Rect,
        scale_factor: f32,
        viewer: &ViewerState,
        pointer_pos: Option<Point2>,
        dark_mode: bool,
    ) -> bool {
        if viewer.gizmo_mode() != GizmoMode::Cube {
            return false;
        }

        let gizmo_rect = viewer.gizmo_rect(rect);
        let size = pixel_size(gizmo_rect, scale_factor);
        if size[0] == 0 || size[1] == 0 {
            return false;
        }

        self.ensure_target(size);
        self.update_camera(viewer);

        let basis = view_basis(viewer);
        let hover = pointer_pos
            .and_then(|pos| pick_target(pos, rect, basis))
            .map(|pick| pick.target);

        if self.materials_dirty || self.current_dark != dark_mode || self.current_hover != hover {
            self.current_dark = dark_mode;
            self.current_hover = hover;
            self.update_materials();
            self.materials_dirty = false;
        }

        self.scene.render(&self.target.view);
        true
    }

    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.target.view
    }

    pub fn target_revision(&self) -> u64 {
        self.target_revision
    }

    fn ensure_target(&mut self, size: [u32; 2]) {
        if self.target.size != size {
            self.target = RenderTarget::new(&self.device, size);
            self.target_revision = self.target_revision.wrapping_add(1);
        }
        let current = self.scene.descriptor().render_texture.canvas_size;
        if current != (size[0], size[1]) {
            let mut desc = self.scene.descriptor_mut();
            desc.render_texture.canvas_size = (size[0], size[1]);
        }
    }

    fn update_camera(&mut self, viewer: &ViewerState) {
        let forward = (viewer.camera_target() - viewer.camera_position()).normalized();
        let eye = Vec3::new(-forward.x, -forward.y, -forward.z) * GIZMO_CAMERA_DISTANCE;
        let target = Vec3::ZERO;
        let up = viewer.camera_up();

        let eye = to_point(eye);
        let target = to_point(target);
        let up = to_vector(up);
        let matrix = Matrix4::look_at_rh(eye, target, up);
        let matrix = matrix.invert().unwrap_or_else(Matrix4::identity);
        let screen_size = gizmo_screen_size(self.cube_radius);
        let camera = Camera {
            matrix,
            method: ProjectionMethod::parallel(screen_size),
            near_clip: 0.1,
            far_clip: 10.0,
        };
        let studio = self.scene.studio_config_mut();
        studio.camera = camera;
        if let Some(light) = studio.lights.first_mut() {
            light.position = eye;
            light.light_type = LightType::Point;
        }
    }

    fn add_objects_to_scene(&mut self) {
        for part in &self.cube_parts {
            self.scene.add_object(&part.instance);
        }
        if let Some(lines) = &self.edge_lines {
            self.scene.add_object(lines);
        }
    }

    fn update_materials(&mut self) {
        let dark = self.current_dark;
        let hover = self.current_hover;
        let hover_is_face = matches!(hover, Some(ViewTarget::Face(_)));
        let creator = self.creator.clone();
        let (face_tex, face_tex_hover) = if dark {
            let base = ensure_face_texture(&creator, &mut self.face_tex_dark, LABELS_DARK);
            let hover_tex = if hover_is_face {
                Some(ensure_face_texture(
                    &creator,
                    &mut self.face_tex_dark_hover,
                    LABELS_DARK_HOVER,
                ))
            } else {
                None
            };
            (base, hover_tex)
        } else {
            let base = ensure_face_texture(&creator, &mut self.face_tex_light, LABELS_LIGHT);
            let hover_tex = if hover_is_face {
                Some(ensure_face_texture(
                    &creator,
                    &mut self.face_tex_light_hover,
                    LABELS_LIGHT_HOVER,
                ))
            } else {
                None
            };
            (base, hover_tex)
        };

        let edge_base = if dark { EDGE_DARK } else { EDGE_LIGHT };
        let edge_hover = if dark { EDGE_DARK_HOVER } else { EDGE_LIGHT_HOVER };
        let edge_material = gizmo_material(edge_base);
        let edge_hover_material = gizmo_material(edge_hover);

        for part in &mut self.cube_parts {
            let is_hover = is_part_hovered(part, hover);
            match part.kind {
                CubePartKind::Face(_) => {
                    let texture = if is_hover {
                        face_tex_hover.clone().unwrap_or_else(|| face_tex.clone())
                    } else {
                        face_tex.clone()
                    };
                    part.instance.instance_state_mut().material = gizmo_face_material();
                    part.instance.instance_state_mut().texture = Some(texture);
                    self.scene.update_bind_group(&part.instance);
                }
                CubePartKind::Edge | CubePartKind::Corner => {
                    let material = if is_hover { edge_hover_material } else { edge_material };
                    part.instance.instance_state_mut().material = material;
                    part.instance.instance_state_mut().texture = None;
                    self.scene.update_bind_group(&part.instance);
                }
            }
        }

        if let Some(lines) = &mut self.edge_lines {
            lines.instance_state_mut().color = color_to_vec4(EDGE_LINE_COLOR, 1.0);
            self.scene.update_bind_group(lines);
        }
    }

}

fn ensure_face_texture(
    creator: &InstanceCreator,
    slot: &mut Option<Arc<wgpu::Texture>>,
    bytes: &[u8],
) -> Arc<wgpu::Texture> {
    if slot.is_none() {
        let texture = creator.create_texture(&load_image(bytes));
        *slot = Some(texture);
    }
    slot.as_ref().expect("gizmo texture missing").clone()
}

struct RenderTarget {
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl RenderTarget {
    fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gizmo_cube"),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { size, texture, view }
    }
}

fn load_image(bytes: &[u8]) -> DynamicImage {
    image::load_from_memory(bytes).expect("gizmo texture decode failed")
}

fn placeholder_image() -> DynamicImage {
    let image = RgbaImage::from_pixel(1, 1, Rgba([255, 255, 255, 255]));
    DynamicImage::ImageRgba8(image)
}

fn build_cube_parts(
    creator: &InstanceCreator,
    placeholder_tex: &Arc<wgpu::Texture>,
) -> (Vec<CubePart>, Option<WireFrameInstance>, f64) {
    let glb = parse_glb(GIZMO_GLB);
    let mut parts = Vec::new();
    let mut line_segments: Vec<(Point3, Point3)> = Vec::new();
    let mut max_radius: f64 = 0.0;

    for node in &glb.nodes {
        let Some(mesh_index) = node.mesh else {
            continue;
        };
        let name = node.name.clone().unwrap_or_default();
        if !(name.starts_with("face_") || name.starts_with("edge_") || name.starts_with("corner_"))
        {
            continue;
        }

        let mesh = build_mesh_for_node(&glb, mesh_index, node);
        let mut part_kind = part_kind_from_name(&name);
        let target = view_target_from_mesh(&mesh, part_kind);
        if let Some(ViewTarget::Face(face)) = target {
            part_kind = CubePartKind::Face(face);
        }
        max_radius = max_radius.max(mesh_radius(&mesh));
        let state = match part_kind {
            CubePartKind::Face(_) => PolygonState {
                matrix: Matrix4::identity(),
                material: gizmo_face_material(),
                texture: Some(placeholder_tex.clone()),
                backface_culling: true,
            },
            CubePartKind::Edge | CubePartKind::Corner => PolygonState {
                matrix: Matrix4::identity(),
                material: gizmo_material(EDGE_LIGHT),
                texture: None,
                backface_culling: true,
            },
        };

        if matches!(part_kind, CubePartKind::Edge | CubePartKind::Corner) {
            collect_boundary_segments(&mesh, &mut line_segments);
        }

        let instance = creator.create_instance(&mesh, &state);
        parts.push(CubePart {
            name,
            kind: part_kind,
            target,
            instance,
        });
    }

    let edge_lines = if line_segments.is_empty() {
        None
    } else {
        let state = WireFrameState {
            matrix: Matrix4::identity(),
            color: color_to_vec4(EDGE_LINE_COLOR, 1.0),
        };
        Some(creator.create_instance(&line_segments, &state))
    };

    (parts, edge_lines, max_radius)
}

fn part_kind_from_name(name: &str) -> CubePartKind {
    if name.starts_with("edge_") {
        return CubePartKind::Edge;
    }
    if name.starts_with("corner_") {
        return CubePartKind::Corner;
    }
    if let Some(face) = face_from_name(name) {
        return CubePartKind::Face(face);
    }
    CubePartKind::Edge
}

fn face_from_name(name: &str) -> Option<ViewFace> {
    let suffix = name.strip_prefix("face_")?;
    match suffix {
        "front" => Some(ViewFace::Front),
        "back" => Some(ViewFace::Back),
        "left" => Some(ViewFace::Left),
        "right" => Some(ViewFace::Right),
        "top" => Some(ViewFace::Top),
        "bottom" => Some(ViewFace::Bottom),
        _ => None,
    }
}

fn is_part_hovered(part: &CubePart, hover: Option<ViewTarget>) -> bool {
    if let Some(target) = part.target {
        return Some(target) == hover;
    }
    let Some(hover) = hover else {
        return false;
    };
    match hover {
        ViewTarget::Face(face) => part.kind == CubePartKind::Face(face),
        ViewTarget::Edge(idx) => {
            if part.kind != CubePartKind::Edge {
                return false;
            }
            edge_name_for_index(idx).map_or(false, |name| name == part.name)
        }
        ViewTarget::Corner(idx) => {
            if part.kind != CubePartKind::Corner {
                return false;
            }
            corner_name_for_index(idx).map_or(false, |name| name == part.name)
        }
    }
}

fn view_target_from_mesh(mesh: &PolygonMesh, kind: CubePartKind) -> Option<ViewTarget> {
    let center = mesh_center(mesh)?;
    let direction = center.normalized();
    if direction.length() <= 1.0e-6 {
        return None;
    }
    Some(match kind {
        CubePartKind::Face(_) => ViewTarget::Face(closest_face(direction)),
        CubePartKind::Edge => ViewTarget::Edge(closest_edge_index(direction)),
        CubePartKind::Corner => ViewTarget::Corner(closest_corner_index(direction)),
    })
}

fn mesh_center(mesh: &PolygonMesh) -> Option<Vec3> {
    let positions = mesh.positions();
    if positions.is_empty() {
        return None;
    }
    let mut sum = Vec3::ZERO;
    for pos in positions {
        sum = sum + Vec3::new(pos.x, pos.y, pos.z);
    }
    Some(sum / positions.len() as f64)
}

fn closest_face(direction: Vec3) -> ViewFace {
    let candidates = [
        (ViewFace::Front, Vec3::new(0.0, -1.0, 0.0)),
        (ViewFace::Back, Vec3::new(0.0, 1.0, 0.0)),
        (ViewFace::Right, Vec3::new(1.0, 0.0, 0.0)),
        (ViewFace::Left, Vec3::new(-1.0, 0.0, 0.0)),
        (ViewFace::Top, Vec3::new(0.0, 0.0, 1.0)),
        (ViewFace::Bottom, Vec3::new(0.0, 0.0, -1.0)),
    ];
    let mut best = ViewFace::Front;
    let mut best_dot = f64::NEG_INFINITY;
    for (face, normal) in candidates {
        let dot = direction.dot(normal);
        if dot > best_dot {
            best_dot = dot;
            best = face;
        }
    }
    best
}

fn closest_corner_index(direction: Vec3) -> usize {
    let vertices = cube_vertices();
    let mut best = 0;
    let mut best_dot = f64::NEG_INFINITY;
    for (idx, vertex) in vertices.iter().enumerate() {
        let dot = direction.dot(vertex.normalized());
        if dot > best_dot {
            best_dot = dot;
            best = idx;
        }
    }
    best
}

fn closest_edge_index(direction: Vec3) -> usize {
    let vertices = cube_vertices();
    let mut best = 0;
    let mut best_dot = f64::NEG_INFINITY;
    for (idx, (a, b)) in EDGE_DEFS.iter().enumerate() {
        let mid = (vertices[*a] + vertices[*b]) * 0.5;
        let dot = direction.dot(mid.normalized());
        if dot > best_dot {
            best_dot = dot;
            best = idx;
        }
    }
    best
}

fn gizmo_face_material() -> Material {
    Material {
        albedo: Vector3::new(1.0, 1.0, 1.0).extend(1.0),
        roughness: 0.6,
        reflectance: 0.0,
        ambient_ratio: 0.25,
        background_ratio: 0.0,
        alpha_blend: false,
    }
}

fn gizmo_material(color: Color32) -> Material {
    Material {
        albedo: color_to_vec4(color, 1.0),
        roughness: 0.6,
        reflectance: 0.0,
        ambient_ratio: 0.25,
        background_ratio: 0.0,
        alpha_blend: false,
    }
}

fn collect_boundary_segments(mesh: &PolygonMesh, segments: &mut Vec<(Point3, Point3)>) {
    let positions = mesh.positions();
    let mut edge_count: HashMap<(usize, usize), u32> = HashMap::new();
    for tri in mesh.faces().triangle_iter() {
        let indices = [tri[0].pos, tri[1].pos, tri[2].pos];
        add_edge(&mut edge_count, indices[0], indices[1]);
        add_edge(&mut edge_count, indices[1], indices[2]);
        add_edge(&mut edge_count, indices[2], indices[0]);
    }
    for ((a, b), count) in edge_count {
        if count == 1 {
            if let (Some(pa), Some(pb)) = (positions.get(a), positions.get(b)) {
                segments.push((*pa, *pb));
            }
        }
    }
}

fn add_edge(edge_count: &mut HashMap<(usize, usize), u32>, a: usize, b: usize) {
    let key = if a < b { (a, b) } else { (b, a) };
    let entry = edge_count.entry(key).or_insert(0);
    *entry += 1;
}

fn parse_glb(bytes: &[u8]) -> GltfRoot {
    let (json_bytes, bin_bytes) = split_glb(bytes);
    let mut root: GltfRoot =
        serde_json::from_slice(&json_bytes).expect("gizmo glb json parse failed");
    root.bin = bin_bytes;
    root
}

fn split_glb(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if bytes.len() < 20 {
        panic!("gizmo glb data too small");
    }
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json_start = 20;
    let json_end = json_start + json_len;
    let json_bytes = bytes[json_start..json_end].to_vec();
    let bin_header = json_end;
    let bin_len = u32::from_le_bytes(bytes[bin_header..bin_header + 4].try_into().unwrap()) as usize;
    let bin_start = bin_header + 8;
    let bin_end = bin_start + bin_len;
    let bin_bytes = bytes[bin_start..bin_end].to_vec();
    (json_bytes, bin_bytes)
}

fn build_mesh_for_node(glb: &GltfRoot, mesh_index: usize, node: &GltfNode) -> PolygonMesh {
    let mesh = glb.meshes.get(mesh_index).expect("gizmo mesh missing");
    let primitive = mesh.primitives.get(0).expect("gizmo primitive missing");
    let pos_acc = primitive.attributes.position;
    let nor_acc = primitive.attributes.normal;
    let uv_acc = primitive.attributes.texcoord_0;
    let idx_acc = primitive.indices;

    let positions = read_accessor_vec3(glb, pos_acc);
    let normals = read_accessor_vec3(glb, nor_acc);
    let uvs = read_accessor_vec2(glb, uv_acc);
    let indices = read_accessor_indices(glb, idx_acc);

    let attrs = StandardAttributes {
        positions: positions
            .iter()
            .map(|p| Point3::new(p[0] as f64, p[1] as f64, p[2] as f64))
            .collect(),
        uv_coords: uvs
            .iter()
            .map(|uv| Vector2::new(uv[0] as f64, uv[1] as f64))
            .collect(),
        normals: normals
            .iter()
            .map(|n| Vector3::new(n[0] as f64, n[1] as f64, n[2] as f64))
            .collect(),
    };

    let tri_faces: Vec<[StandardVertex; 3]> = indices
        .chunks(3)
        .filter_map(|chunk| {
            if chunk.len() != 3 {
                return None;
            }
            let a = chunk[0] as usize;
            let b = chunk[1] as usize;
            let c = chunk[2] as usize;
            Some([
                StandardVertex {
                    pos: a,
                    uv: Some(a),
                    nor: Some(a),
                },
                StandardVertex {
                    pos: b,
                    uv: Some(b),
                    nor: Some(b),
                },
                StandardVertex {
                    pos: c,
                    uv: Some(c),
                    nor: Some(c),
                },
            ])
        })
        .collect();

    let faces = Faces::from_tri_and_quad_faces(tri_faces, Vec::new());
    let mesh = PolygonMesh::new(attrs, faces);

    let matrix = axis_swap_matrix() * node_transform(node) * Matrix4::from_scale(GIZMO_SCALE);
    mesh.transformed(matrix)
}

fn node_transform(node: &GltfNode) -> Matrix4 {
    if let Some(matrix) = node.matrix {
        return matrix_from_gltf(matrix);
    }
    let translation = node
        .translation
        .unwrap_or([0.0, 0.0, 0.0])
        .map(|v| v as f64);
    let rotation = node.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let scale = node.scale.unwrap_or([1.0, 1.0, 1.0]).map(|v| v as f64);

    let trans = Matrix4::from_translation(Vector3::new(
        translation[0],
        translation[1],
        translation[2],
    ));
    let rot = Matrix4::from(Quaternion::new(
        rotation[3] as f64,
        rotation[0] as f64,
        rotation[1] as f64,
        rotation[2] as f64,
    ));
    let scale = Matrix4::from_nonuniform_scale(scale[0], scale[1], scale[2]);
    trans * rot * scale
}

fn matrix_from_gltf(matrix: [f32; 16]) -> Matrix4 {
    Matrix4::new(
        matrix[0] as f64,
        matrix[1] as f64,
        matrix[2] as f64,
        matrix[3] as f64,
        matrix[4] as f64,
        matrix[5] as f64,
        matrix[6] as f64,
        matrix[7] as f64,
        matrix[8] as f64,
        matrix[9] as f64,
        matrix[10] as f64,
        matrix[11] as f64,
        matrix[12] as f64,
        matrix[13] as f64,
        matrix[14] as f64,
        matrix[15] as f64,
    )
}

fn axis_swap_matrix() -> Matrix4 {
    Matrix4::identity()
}

fn gizmo_screen_size(radius: f64) -> f64 {
    if radius <= 1.0e-6 {
        return GIZMO_SCREEN_SIZE_FALLBACK;
    }
    let inset = GIZMO_CIRCLE_INSET.clamp(0.1, 1.0);
    2.0 * radius / inset
}

fn mesh_radius(mesh: &PolygonMesh) -> f64 {
    mesh.positions()
        .iter()
        .map(|p| (p.x * p.x + p.y * p.y + p.z * p.z).sqrt())
        .fold(0.0, f64::max)
}

fn read_accessor_vec3(glb: &GltfRoot, accessor_index: usize) -> Vec<[f32; 3]> {
    let values = read_accessor_f32(glb, accessor_index);
    values
        .chunks(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect()
}

fn read_accessor_vec2(glb: &GltfRoot, accessor_index: usize) -> Vec<[f32; 2]> {
    let values = read_accessor_f32(glb, accessor_index);
    values
        .chunks(2)
        .map(|chunk| [chunk[0], chunk[1]])
        .collect()
}

fn read_accessor_indices(glb: &GltfRoot, accessor_index: usize) -> Vec<u32> {
    let accessor = glb
        .accessors
        .get(accessor_index)
        .expect("gizmo accessor missing");
    let view = glb
        .buffer_views
        .get(accessor.buffer_view)
        .expect("gizmo buffer view missing");
    let offset = view.byte_offset.unwrap_or(0) + accessor.byte_offset.unwrap_or(0);
    let stride = view
        .byte_stride
        .unwrap_or_else(|| accessor.component_size());

    let mut indices = Vec::with_capacity(accessor.count);
    for i in 0..accessor.count {
        let base = offset + i * stride;
        let value = match accessor.component_type {
            5123 => u16::from_le_bytes(
                glb.bin[base..base + 2].try_into().expect("index u16"),
            ) as u32,
            5125 => u32::from_le_bytes(
                glb.bin[base..base + 4].try_into().expect("index u32"),
            ),
            _ => panic!("unsupported index component type"),
        };
        indices.push(value);
    }
    indices
}

fn read_accessor_f32(glb: &GltfRoot, accessor_index: usize) -> Vec<f32> {
    let accessor = glb
        .accessors
        .get(accessor_index)
        .expect("gizmo accessor missing");
    if accessor.component_type != 5126 {
        panic!("unsupported f32 accessor component type");
    }
    let view = glb
        .buffer_views
        .get(accessor.buffer_view)
        .expect("gizmo buffer view missing");
    let offset = view.byte_offset.unwrap_or(0) + accessor.byte_offset.unwrap_or(0);
    let stride = view
        .byte_stride
        .unwrap_or_else(|| accessor.component_size() * accessor.component_count());

    let mut values = Vec::with_capacity(accessor.count * accessor.component_count());
    for i in 0..accessor.count {
        let base = offset + i * stride;
        for c in 0..accessor.component_count() {
            let start = base + c * accessor.component_size();
            let value = f32::from_le_bytes(
                glb.bin[start..start + 4]
                    .try_into()
                    .expect("accessor f32"),
            );
            values.push(value);
        }
    }
    values
}

fn view_basis(viewer: &ViewerState) -> ViewBasis {
    let forward = (viewer.camera_target() - viewer.camera_position()).normalized();
    let mut right = forward.cross(viewer.camera_up());
    if right.length() <= 1.0e-6 {
        right = Vec3::new(1.0, 0.0, 0.0);
    }
    right = right.normalized();
    let up = right.cross(forward).normalized();
    ViewBasis::new(right, up, forward)
}

fn pixel_size(rect: Rect, pixels_per_point: f32) -> [u32; 2] {
    let width = (rect.width() * pixels_per_point).round().max(1.0) as u32;
    let height = (rect.height() * pixels_per_point).round().max(1.0) as u32;
    [width, height]
}

fn edge_name_for_index(index: usize) -> Option<&'static str> {
    let vertices = cube_vertices();
    let (a_idx, b_idx) = EDGE_DEFS.get(index).copied()?;
    let a = vertices[a_idx];
    let b = vertices[b_idx];
    edge_name_from_vertices(a, b)
}

fn corner_name_for_index(index: usize) -> Option<&'static str> {
    let vertices = cube_vertices();
    let v = vertices.get(index)?;
    let x = if v.x > 0.0 { "right" } else { "left" };
    let y = if v.y > 0.0 { "back" } else { "front" };
    let z = if v.z > 0.0 { "top" } else { "bottom" };
    Some(match (y, x, z) {
        ("front", "left", "bottom") => "corner_front_left_bottom",
        ("front", "left", "top") => "corner_front_left_top",
        ("front", "right", "bottom") => "corner_front_right_bottom",
        ("front", "right", "top") => "corner_front_right_top",
        ("back", "left", "bottom") => "corner_back_left_bottom",
        ("back", "left", "top") => "corner_back_left_top",
        ("back", "right", "bottom") => "corner_back_right_bottom",
        ("back", "right", "top") => "corner_back_right_top",
        _ => return None,
    })
}

fn edge_name_from_vertices(a: Vec3, b: Vec3) -> Option<&'static str> {
    let mut back = a.y < -0.25 && b.y < -0.25;
    let mut front = a.y > 0.25 && b.y > 0.25;
    let left = a.x < -0.25 && b.x < -0.25;
    let right = a.x > 0.25 && b.x > 0.25;
    let bottom = a.z < -0.25 && b.z < -0.25;
    let top = a.z > 0.25 && b.z > 0.25;
    std::mem::swap(&mut front, &mut back);

    if back {
        if bottom {
            return Some("edge_back_bottom");
        }
        if top {
            return Some("edge_back_top");
        }
        if left {
            return Some("edge_back_left");
        }
        if right {
            return Some("edge_back_right");
        }
    }

    if front {
        if bottom {
            return Some("edge_front_bottom");
        }
        if top {
            return Some("edge_front_top");
        }
        if left {
            return Some("edge_front_left");
        }
        if right {
            return Some("edge_front_right");
        }
    }

    if top {
        if left {
            return Some("edge_top_left");
        }
        if right {
            return Some("edge_top_right");
        }
    }

    if bottom {
        if left {
            return Some("edge_left_bottom");
        }
        if right {
            return Some("edge_right_bottom");
        }
    }

    None
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

fn color_to_vec4(color: Color32, alpha: f32) -> Vector4 {
    let [r, g, b, _] = color.to_array();
    Vector4::new(
        srgb_to_linear(r) as f64,
        srgb_to_linear(g) as f64,
        srgb_to_linear(b) as f64,
        alpha as f64,
    )
}

fn srgb_to_linear(value: u8) -> f32 {
    let c = value as f32 / 255.0;
    c.powf(2.2)
}

fn to_point(value: Vec3) -> Point3 {
    Point3::new(value.x, value.y, value.z)
}

fn to_vector(value: Vec3) -> Vector3 {
    Vector3::new(value.x, value.y, value.z)
}

#[derive(Debug, Deserialize)]
struct GltfRoot {
    nodes: Vec<GltfNode>,
    meshes: Vec<GltfMesh>,
    accessors: Vec<GltfAccessor>,
    #[serde(rename = "bufferViews")]
    buffer_views: Vec<GltfBufferView>,
    #[serde(skip)]
    bin: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct GltfNode {
    name: Option<String>,
    mesh: Option<usize>,
    rotation: Option<[f32; 4]>,
    translation: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
    matrix: Option<[f32; 16]>,
}

#[derive(Debug, Deserialize)]
struct GltfMesh {
    primitives: Vec<GltfPrimitive>,
}

#[derive(Debug, Deserialize)]
struct GltfPrimitive {
    attributes: GltfAttributes,
    indices: usize,
}

#[derive(Debug, Deserialize)]
struct GltfAttributes {
    #[serde(rename = "POSITION")]
    position: usize,
    #[serde(rename = "NORMAL")]
    normal: usize,
    #[serde(rename = "TEXCOORD_0")]
    texcoord_0: usize,
}

#[derive(Debug, Deserialize)]
struct GltfAccessor {
    #[serde(rename = "bufferView")]
    buffer_view: usize,
    #[serde(rename = "byteOffset")]
    byte_offset: Option<usize>,
    #[serde(rename = "componentType")]
    component_type: u32,
    count: usize,
    #[serde(rename = "type")]
    accessor_type: String,
}

#[derive(Debug, Deserialize)]
struct GltfBufferView {
    buffer: usize,
    #[serde(rename = "byteOffset")]
    byte_offset: Option<usize>,
    #[serde(rename = "byteLength")]
    byte_length: usize,
    #[serde(rename = "byteStride")]
    byte_stride: Option<usize>,
}

impl GltfAccessor {
    fn component_size(&self) -> usize {
        match self.component_type {
            5126 => 4,
            5123 => 2,
            5125 => 4,
            _ => panic!("unsupported component type"),
        }
    }

    fn component_count(&self) -> usize {
        match self.accessor_type.as_str() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => panic!("unsupported accessor type"),
        }
    }
}
