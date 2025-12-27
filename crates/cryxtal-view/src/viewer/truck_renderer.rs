use truck_base::cgmath64::{InnerSpace, Matrix4, Point3, Rad, SquareMatrix, Vector3, Vector4};
use truck_platform::{
    BackendBufferConfig, Camera, DeviceHandler, Light, LightType, ProjectionMethod,
    RenderTextureConfig, Scene, SceneDescriptor, StudioConfig,
};
use truck_meshalgo::prelude::{NormalFilters, OptimizingFilter};
use truck_polymesh::{Faces, PolygonMesh, StandardAttributes, Transformed};
use truck_rendimpl::{
    CreatorCreator, InstanceCreator, Material, PolygonInstance, PolygonState, WireFrameInstance,
    WireFrameState,
};

use super::math::Vec3;
use super::ui::{Color32, Rect};
use super::{ViewMode, ViewerMesh, ViewerState};

pub struct TruckRenderer {
    scene: Scene,
    creator: InstanceCreator,
    device: wgpu::Device,
    target: RenderTarget,
    target_revision: u64,
    mesh_revision: u64,
    instances: Vec<ElementInstances>,
    axes: AxisInstances,
    last_view_mode: Option<ViewMode>,
    last_selected: Option<usize>,
    last_hovered: Option<usize>,
    last_colors_hash: u64,
    instances_dirty: bool,
}

struct RenderTarget {
    size: [u32; 2],
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct ElementInstances {
    surface: PolygonInstance,
    wire: WireFrameInstance,
}

struct AxisInstances {
    x: PolygonInstance,
    y: PolygonInstance,
    z: PolygonInstance,
}

impl TruckRenderer {
    pub fn new(adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let initial_size = [1, 1];
        let scene_desc = SceneDescriptor {
            studio: StudioConfig {
                background: wgpu::Color {
                    r: 0.07,
                    g: 0.08,
                    b: 0.09,
                    a: 1.0,
                },
                camera: Camera::default(),
                lights: vec![Light {
                    position: Point3::new(1.0, 1.0, 1.0),
                    color: Vector3::new(1.0, 1.0, 1.0),
                    light_type: LightType::Point,
                }],
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
        let creator = scene.instance_creator();
        let target = RenderTarget::new(&device, initial_size);
        let axes = AxisInstances::new(&creator);
        let mut renderer = Self {
            scene,
            creator,
            device,
            target,
            target_revision: 0,
            mesh_revision: 0,
            instances: Vec::new(),
            axes,
            last_view_mode: None,
            last_selected: None,
            last_hovered: None,
            last_colors_hash: 0,
            instances_dirty: true,
        };
        renderer.axes.add_to_scene(&mut renderer.scene);
        renderer
    }

    pub fn render(
        &mut self,
        rect: Rect,
        scale_factor: f32,
        viewer: &ViewerState,
        bounds: Option<(Vec3, Vec3)>,
        meshes: &[ViewerMesh],
        poly_meshes: &[PolygonMesh],
        mesh_revision: u64,
        element_colors: &[Color32],
        element_visibility: &[bool],
        element_wireframe: &[bool],
        element_skeleton_solid: &[bool],
        hovered: Option<usize>,
        selected: Option<usize>,
        view_mode: ViewMode,
    ) -> bool {
        let size = pixel_size(rect, scale_factor);
        if size[0] == 0 || size[1] == 0 {
            return false;
        }

        self.ensure_target(size);
        self.sync_meshes(mesh_revision, meshes, poly_meshes);
        self.update_camera(viewer, bounds, rect);
        self.update_instances(
            view_mode,
            element_colors,
            element_visibility,
            element_wireframe,
            element_skeleton_solid,
            hovered,
            selected,
        );

        self.scene.render(&self.target.view);
        true
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

    fn sync_meshes(
        &mut self,
        mesh_revision: u64,
        meshes: &[ViewerMesh],
        poly_meshes: &[PolygonMesh],
    ) {
        if self.mesh_revision == mesh_revision {
            return;
        }
        self.mesh_revision = mesh_revision;
        self.scene.clear_objects();
        self.instances.clear();

        let count = meshes.len().min(poly_meshes.len());
        let mut instances = Vec::new();
        for idx in 0..count {
            let mesh = &meshes[idx];
            let poly = &poly_meshes[idx];
            if mesh.is_empty() {
                continue;
            }
            let surface_state = PolygonState {
                matrix: Matrix4::identity(),
                material: flat_material(Color32::from_rgb(180, 190, 200), 1.0, false),
                texture: None,
                backface_culling: true,
            };
            let surface = self.creator.create_instance(poly, &surface_state);
            let wire_state = WireFrameState {
                matrix: Matrix4::identity(),
                color: Vector4::new(0.2, 0.2, 0.2, 1.0),
            };
            let edges = edge_segments(mesh);
            let wire = self.creator.create_instance(&edges, &wire_state);
            instances.push(ElementInstances { surface, wire });
        }
        self.instances = instances;
        self.rebuild_draw_order();
        self.instances_dirty = true;
    }

    fn update_camera(&mut self, viewer: &ViewerState, bounds: Option<(Vec3, Vec3)>, rect: Rect) {
        let eye = to_point(viewer.camera_position());
        let target = to_point(viewer.camera_target());
        let up = to_vector(viewer.camera_up());
        let matrix = Matrix4::look_at_rh(eye, target, up);
        let matrix = matrix.invert().unwrap_or_else(Matrix4::identity);
        let (near_clip, far_clip) = clip_planes(viewer.distance(), bounds);
        let screen_size = ortho_screen_size(viewer, rect);
        let camera = Camera {
            matrix,
            method: ProjectionMethod::parallel(screen_size),
            near_clip,
            far_clip,
        };
        let studio = self.scene.studio_config_mut();
        studio.camera = camera;
        if let Some(light) = studio.lights.first_mut() {
            light.position = eye;
            light.light_type = LightType::Point;
        }
    }

    fn update_instances(
        &mut self,
        view_mode: ViewMode,
        element_colors: &[Color32],
        element_visibility: &[bool],
        element_wireframe: &[bool],
        element_skeleton_solid: &[bool],
        hovered: Option<usize>,
        selected: Option<usize>,
    ) {
        let highlight = Color32::from_rgb(255, 210, 90);
        let hover = Color32::from_rgb(70, 230, 255);
        let default_color = Color32::from_rgb(180, 190, 200);
        let material_color = Color32::from_rgb(170, 175, 185);
        let colors_hash = hash_colors(element_colors);
        let update_pipeline = self.last_view_mode.map_or(true, |mode| mode != view_mode);
        let colors_changed = self.last_colors_hash != colors_hash;
        let selected_changed = self.last_selected != selected;
        let hovered_changed = self.last_hovered != hovered;
        let update_all = self.instances_dirty || update_pipeline || colors_changed;

        if !update_all && !selected_changed && !hovered_changed {
            return;
        }

        if update_pipeline {
            self.rebuild_draw_order();
        }

        if update_all {
            for idx in 0..self.instances.len() {
                let visible = element_visibility.get(idx).copied().unwrap_or(true);
                let wireframe = element_wireframe.get(idx).copied().unwrap_or(true);
                let skeleton_solid = element_skeleton_solid.get(idx).copied().unwrap_or(false);
                self.update_instance_state(
                    idx,
                    view_mode,
                    element_colors,
                    visible,
                    wireframe,
                    skeleton_solid,
                    hovered,
                    selected,
                    hover,
                    highlight,
                    default_color,
                    material_color,
                    update_pipeline,
                );
            }
        } else {
            let mut indices = Vec::new();
            let push_unique = |idx: usize, list: &mut Vec<usize>| {
                if !list.contains(&idx) {
                    list.push(idx);
                }
            };
            if selected_changed {
                if let Some(prev) = self.last_selected {
                    push_unique(prev, &mut indices);
                }
                if let Some(curr) = selected {
                    push_unique(curr, &mut indices);
                }
            }
            if hovered_changed {
                if let Some(prev) = self.last_hovered {
                    push_unique(prev, &mut indices);
                }
                if let Some(curr) = hovered {
                    push_unique(curr, &mut indices);
                }
            }

            for idx in indices {
                let visible = element_visibility.get(idx).copied().unwrap_or(true);
                let wireframe = element_wireframe.get(idx).copied().unwrap_or(true);
                let skeleton_solid = element_skeleton_solid.get(idx).copied().unwrap_or(false);
                self.update_instance_state(
                    idx,
                    view_mode,
                    element_colors,
                    visible,
                    wireframe,
                    skeleton_solid,
                    hovered,
                    selected,
                    hover,
                    highlight,
                    default_color,
                    material_color,
                    false,
                );
            }
        }

        self.last_view_mode = Some(view_mode);
        self.last_selected = selected;
        self.last_hovered = hovered;
        self.last_colors_hash = colors_hash;
        self.instances_dirty = false;
    }

    fn update_instance_state(
        &mut self,
        idx: usize,
        view_mode: ViewMode,
        element_colors: &[Color32],
        visible: bool,
        wireframe: bool,
        skeleton_solid: bool,
        hovered: Option<usize>,
        selected: Option<usize>,
        hover: Color32,
        highlight: Color32,
        default_color: Color32,
        material_color: Color32,
        update_pipeline: bool,
    ) {
        let Some(instance) = self.instances.get_mut(idx) else {
            return;
        };

        let base = element_colors.get(idx).copied().unwrap_or(default_color);
        let base = if Some(idx) == selected {
            blend_color(base, highlight, 0.45)
        } else if Some(idx) == hovered {
            blend_color(base, hover, 0.35)
        } else {
            base
        };
        let (mut surface_visible, mut wire_visible, surface_color, mut wire_color, mut alpha, mut alpha_blend) =
            match view_mode {
                ViewMode::Skeleton => {
                    let mut wire = if Some(idx) == selected {
                        blend_color(base, highlight, 0.6)
                    } else if Some(idx) == hovered {
                        blend_color(base, hover, 0.6)
                    } else {
                        base
                    };
                    if skeleton_solid && Some(idx) != selected && Some(idx) != hovered {
                        wire = darken_color(wire, 0.35);
                    }
                    let mut surface_visible = false;
                    let mut alpha = 1.0;
                    let mut alpha_blend = false;
                    if skeleton_solid {
                        surface_visible = true;
                        alpha = 0.32;
                        alpha_blend = true;
                    }
                    (surface_visible, true, base, wire, alpha, alpha_blend)
                }
                ViewMode::LayerOpaque => {
                    let wire = darken_color(base, 0.55);
                    (true, true, base, wire, 1.0, false)
                }
                ViewMode::LayerTransparent => {
                    let wire = darken_color(base, 0.55);
                    (true, true, base, wire, 0.5, true)
                }
                ViewMode::Material => {
                    let wire = darken_color(material_color, 0.55);
                    (true, true, material_color, wire, 1.0, false)
                }
            };

        if !visible {
            surface_visible = false;
            wire_visible = Some(idx) == selected || Some(idx) == hovered;
            if Some(idx) == selected {
                wire_color = highlight;
            } else if Some(idx) == hovered {
                wire_color = hover;
            }
        }
        if !wireframe && view_mode != ViewMode::Skeleton {
            wire_visible = false;
        }

        let material = flat_material(surface_color, alpha, alpha_blend);
        instance.surface.instance_state_mut().material = material;
        instance.wire.instance_state_mut().color = color_to_vec4(wire_color, 1.0);

        self.scene.set_visibility(&instance.surface, surface_visible);
        self.scene.set_visibility(&instance.wire, wire_visible);
        self.scene.update_bind_group(&instance.surface);
        self.scene.update_bind_group(&instance.wire);
        if update_pipeline {
            self.scene.update_pipeline(&instance.surface);
        }
    }

    fn rebuild_draw_order(&mut self) {
        self.scene.clear_objects();
        for instance in &self.instances {
            self.scene.add_object(&instance.surface);
        }
        for instance in &self.instances {
            self.scene.add_object(&instance.wire);
        }
        self.axes.add_to_scene(&mut self.scene);
    }
}

impl RenderTarget {
    fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("truck_scene"),
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
        Self {
            size,
            texture,
            view,
        }
    }
}

impl AxisInstances {
    fn new(creator: &InstanceCreator) -> Self {
        let length = 1000.0;
        let mesh = axis_mesh(length);
        let y_mesh = mesh.transformed(Matrix4::from_angle_z(Rad(std::f64::consts::FRAC_PI_2)));
        let z_mesh = mesh.transformed(Matrix4::from_angle_y(Rad(-std::f64::consts::FRAC_PI_2)));

        let x_state = axis_state(Color32::from_rgb(255, 90, 90));
        let y_state = axis_state(Color32::from_rgb(90, 255, 140));
        let z_state = axis_state(Color32::from_rgb(90, 160, 255));

        let x = creator.create_instance(&mesh, &x_state);
        let y = creator.create_instance(&y_mesh, &y_state);
        let z = creator.create_instance(&z_mesh, &z_state);
        Self { x, y, z }
    }

    fn add_to_scene(&self, scene: &mut Scene) {
        scene.add_object(&self.x);
        scene.add_object(&self.y);
        scene.add_object(&self.z);
    }
}

fn axis_state(color: Color32) -> PolygonState {
    PolygonState {
        matrix: Matrix4::identity(),
        material: flat_material(color, 1.0, false),
        texture: None,
        backface_culling: false,
    }
}

fn axis_mesh(length: f64) -> PolygonMesh {
    let shaft_half = 6.0;
    let head_half = 18.0;
    let head_len = 120.0;
    let neck_len = 40.0;
    let shaft_len = length - head_len - neck_len;
    let head_start = shaft_len + neck_len;

    let positions = vec![
        Point3::new(0.0, -shaft_half, -shaft_half),
        Point3::new(0.0, shaft_half, -shaft_half),
        Point3::new(0.0, shaft_half, shaft_half),
        Point3::new(0.0, -shaft_half, shaft_half),
        Point3::new(shaft_len, -shaft_half, -shaft_half),
        Point3::new(shaft_len, shaft_half, -shaft_half),
        Point3::new(shaft_len, shaft_half, shaft_half),
        Point3::new(shaft_len, -shaft_half, shaft_half),
        Point3::new(head_start, -head_half, -head_half),
        Point3::new(head_start, head_half, -head_half),
        Point3::new(head_start, head_half, head_half),
        Point3::new(head_start, -head_half, head_half),
        Point3::new(length, 0.0, 0.0),
    ];

    let mut faces: Vec<Vec<usize>> = Vec::new();
    faces.push(oriented_face(
        &[0, 1, 2, 3],
        &positions,
        Vector3::new(-1.0, 0.0, 0.0),
    ));
    faces.push(oriented_face(
        &[0, 4, 7, 3],
        &positions,
        Vector3::new(0.0, -1.0, 0.0),
    ));
    faces.push(oriented_face(
        &[1, 2, 6, 5],
        &positions,
        Vector3::new(0.0, 1.0, 0.0),
    ));
    faces.push(oriented_face(
        &[0, 1, 5, 4],
        &positions,
        Vector3::new(0.0, 0.0, -1.0),
    ));
    faces.push(oriented_face(
        &[3, 7, 6, 2],
        &positions,
        Vector3::new(0.0, 0.0, 1.0),
    ));

    faces.push(oriented_face(
        &[4, 5, 9, 8],
        &positions,
        Vector3::new(0.0, 0.0, -1.0),
    ));
    faces.push(oriented_face(
        &[7, 11, 10, 6],
        &positions,
        Vector3::new(0.0, 0.0, 1.0),
    ));
    faces.push(oriented_face(
        &[4, 8, 11, 7],
        &positions,
        Vector3::new(0.0, -1.0, 0.0),
    ));
    faces.push(oriented_face(
        &[5, 6, 10, 9],
        &positions,
        Vector3::new(0.0, 1.0, 0.0),
    ));

    faces.push(oriented_face(
        &[8, 9, 12],
        &positions,
        Vector3::new(0.0, 0.0, -1.0),
    ));
    faces.push(oriented_face(
        &[11, 10, 12],
        &positions,
        Vector3::new(0.0, 0.0, 1.0),
    ));
    faces.push(oriented_face(
        &[8, 12, 11],
        &positions,
        Vector3::new(0.0, -1.0, 0.0),
    ));
    faces.push(oriented_face(
        &[9, 10, 12],
        &positions,
        Vector3::new(0.0, 1.0, 0.0),
    ));

    let faces = Faces::from_iter(faces.iter());
    let mut mesh = PolygonMesh::new(
        StandardAttributes {
            positions,
            ..Default::default()
        },
        faces,
    );
    mesh.add_naive_normals(true);
    mesh.put_together_same_attrs(truck_base::tolerance::TOLERANCE);
    mesh
}

fn oriented_face(indices: &[usize], positions: &[Point3], expected: Vector3) -> Vec<usize> {
    if indices.len() < 3 {
        return indices.to_vec();
    }
    let a = positions[indices[0]];
    let b = positions[indices[1]];
    let c = positions[indices[2]];
    let normal = (b - a).cross(c - a);
    if normal.magnitude2() <= 1.0e-12 {
        return indices.to_vec();
    }
    if normal.dot(expected) < 0.0 {
        let mut reversed = indices.to_vec();
        reversed.reverse();
        reversed
    } else {
        indices.to_vec()
    }
}

fn edge_segments(mesh: &ViewerMesh) -> Vec<(Point3, Point3)> {
    let mut segments: Vec<(Point3, Point3)> = mesh.edges
        .iter()
        .map(|edge| {
            let a = mesh.positions[edge[0]];
            let b = mesh.positions[edge[1]];
            (to_point(a), to_point(b))
        })
        .collect();
    if segments.is_empty() {
        let origin = Point3::new(0.0, 0.0, 0.0);
        segments.push((origin, origin));
    }
    segments
}

fn flat_material(color: Color32, alpha: f32, alpha_blend: bool) -> Material {
    Material {
        albedo: color_to_vec4(color, alpha),
        roughness: 1.0,
        reflectance: 0.0,
        ambient_ratio: 1.0,
        background_ratio: 0.0,
        alpha_blend,
    }
}

fn color_to_vec4(color: Color32, alpha: f32) -> Vector4 {
    let [r, g, b, _] = color.to_array();
    let r = srgb_to_linear(r);
    let g = srgb_to_linear(g);
    let b = srgb_to_linear(b);
    Vector4::new(r as f64, g as f64, b as f64, alpha as f64)
}

fn srgb_to_linear(value: u8) -> f32 {
    let c = value as f32 / 255.0;
    c.powf(2.2)
}

fn darken_color(base: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = base.to_array();
    let scale = (1.0 - factor).clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        ((r as f32) * scale).clamp(0.0, 255.0) as u8,
        ((g as f32) * scale).clamp(0.0, 255.0) as u8,
        ((b as f32) * scale).clamp(0.0, 255.0) as u8,
        a,
    )
}

fn blend_color(base: Color32, tint: Color32, factor: f32) -> Color32 {
    let [br, bg, bb, ba] = base.to_array();
    let [tr, tg, tb, ta] = tint.to_array();
    let mix = |b: u8, t: u8| -> u8 {
        let value = (b as f32) * (1.0 - factor) + (t as f32) * factor;
        value.clamp(0.0, 255.0) as u8
    };
    let mix_a = |b: u8, t: u8| -> u8 {
        let value = (b as f32) * (1.0 - factor) + (t as f32) * factor;
        value.clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(
        mix(br, tr),
        mix(bg, tg),
        mix(bb, tb),
        mix_a(ba, ta),
    )
}

fn hash_colors(colors: &[Color32]) -> u64 {
    let mut hash = 1469598103934665603u64;
    for color in colors {
        for byte in color.to_array() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
    }
    hash ^ (colors.len() as u64)
}

fn pixel_size(rect: Rect, pixels_per_point: f32) -> [u32; 2] {
    let width = (rect.width() * pixels_per_point).round().max(1.0) as u32;
    let height = (rect.height() * pixels_per_point).round().max(1.0) as u32;
    [width, height]
}

fn clip_planes(distance: f64, bounds: Option<(Vec3, Vec3)>) -> (f64, f64) {
    let near = (distance * 1.0e-4).max(0.01);
    let far = if let Some((min, max)) = bounds {
        let size = max - min;
        let radius = size.max_component().max(1.0) * 0.5;
        (distance + radius * 4.0).max(near + 1.0)
    } else {
        (distance * 50.0).max(near + 1.0)
    };
    (near, far)
}

fn ortho_screen_size(viewer: &ViewerState, rect: Rect) -> f64 {
    let view_size = rect.width().min(rect.height()) as f64;
    let fov = viewer.fov_deg().to_radians();
    let persp = view_size / (2.0 * (fov * 0.5).tan());
    let scale = persp / viewer.distance().max(1.0);
    let height = rect.height().max(1.0) as f64;
    if scale <= 1.0e-6 {
        height
    } else {
        height / scale
    }
}

fn to_point(value: Vec3) -> Point3 {
    Point3::new(value.x, value.y, value.z)
}

fn to_vector(value: Vec3) -> Vector3 {
    Vector3::new(value.x, value.y, value.z)
}

impl TruckRenderer {
    pub fn target_view(&self) -> &wgpu::TextureView {
        &self.target.view
    }

    pub fn target_size(&self) -> [u32; 2] {
        self.target.size
    }

    pub fn target_revision(&self) -> u64 {
        self.target_revision
    }
}
