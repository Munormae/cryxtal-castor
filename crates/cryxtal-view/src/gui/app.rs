use anyhow::Result;
use cryxtal_bim::{BimCategory, BimElement, ParameterValue};
use cryxtal_io::{DEFAULT_TESSELLATION_TOLERANCE, triangulate_solid};
use cryxtal_topology::Point3;
use egui::{self, FontId};
use egui_wgpu::{RenderState, RendererOptions, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use egui_wgpu::winit::Painter;
use egui_winit::State as EguiWinitState;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use std::{sync::mpsc, thread};
use truck_polymesh::PolygonMesh;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

use crate::elements::build_wall_between_points;
use crate::viewer::{
    Align2 as ViewerAlign2, Color32, Modifiers, OverlayPainter, Point2, Rect, Stroke, Vec2,
    GizmoMode, GizmoRenderer, ViewMode, ViewerInput, ViewerMesh, ViewerState, TruckRenderer,
};
use super::layers::Layer;
use super::model::{ModelInfo, format_point, merge_bounds, mesh_bounds};
use super::params::WallParams;
use self::hover_outline::paint_hover_outline;
use self::opening_params::WallOpeningParams;
use self::rebar_params::RebarParams;
use self::rebar_wireframe::tune_rebar_wireframe;

mod hover;
mod hover_outline;
mod opening;
mod opening_params;
mod rebar;
mod rebar_params;
mod rebar_wireframe;

const SELECTION_DRAG_THRESHOLD: f32 = 4.0;


#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolMode {
    Select,
    CreateWall,
    CreateOpening,
    CreateRebar,
}

impl Default for ToolMode {
    fn default() -> Self {
        Self::Select
    }
}

#[derive(Default)]
struct InputState {
    pointer_pos: Option<Point2>,
    pointer_delta: Vec2,
    primary_down: bool,
    secondary_down: bool,
    middle_down: bool,
    primary_clicked: bool,
    double_clicked: bool,
    scroll_delta: f32,
    modifiers: Modifiers,
    key_v_pressed: bool,
    key_v_down: bool,
}

struct MeshBuildResult {
    idx: usize,
    viewer_mesh: ViewerMesh,
    poly_mesh: PolygonMesh,
    bounds: Option<(Point3, Point3)>,
    vertices: usize,
    faces: usize,
}

pub fn run_gui() -> Result<()> {
    let event_loop = EventLoop::new().map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let window = event_loop
        .create_window(
            winit::window::Window::default_attributes()
                .with_title("CryXtal Castor")
                .with_min_inner_size(LogicalSize::new(1200.0, 720.0)),
        )
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let window = Arc::new(window);

    let egui_ctx = egui::Context::default();
    let mut painter = create_painter(egui_ctx.clone())?;
    pollster::block_on(painter.set_window(egui::ViewportId::ROOT, Some(window.clone())))
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let render_state = painter
        .render_state()
        .ok_or_else(|| anyhow::anyhow!("wgpu render state not initialized"))?;

    let mut egui_state = EguiWinitState::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        &event_loop,
        Some(window.scale_factor() as f32),
        window.theme(),
        painter.max_texture_side(),
    );

    let mut app = CryxtalApp::new(
        render_state.adapter.clone(),
        render_state.device.clone(),
        render_state.queue.clone(),
    );

    let clear_color = egui_ctx.style().visuals.window_fill;
    let [r, g, b, a] = clear_color.to_array();
    let clear_color = [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ];

    #[allow(deprecated)]
    event_loop
        .run(move |event, event_loop| {
            event_loop.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    if matches!(event, WindowEvent::CloseRequested) {
                        event_loop.exit();
                        return;
                    }

                    let response = egui_state.on_window_event(&window, &event);
                    if response.repaint {
                        window.request_redraw();
                    }

                    match event {
                        WindowEvent::Resized(size) => {
                            if let (Some(width), Some(height)) =
                                (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                            {
                                painter.on_window_resized(egui::ViewportId::ROOT, width, height);
                            }
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = window.inner_size();
                            if let (Some(width), Some(height)) =
                                (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                            {
                                painter.on_window_resized(egui::ViewportId::ROOT, width, height);
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            let raw_input = egui_state.take_egui_input(&window);
                            let full_output = egui_ctx.run(raw_input, |ctx| {
                                app.ui(ctx, &render_state);
                            });

                            egui_state.handle_platform_output(&window, full_output.platform_output);

                            let clipped_primitives = egui_ctx
                                .tessellate(full_output.shapes, full_output.pixels_per_point);
                            let _ = painter.paint_and_update_textures(
                                egui::ViewportId::ROOT,
                                full_output.pixels_per_point,
                                clear_color,
                                &clipped_primitives,
                                &full_output.textures_delta,
                                Vec::new(),
                            );
                            app.on_frame_presented();
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    Ok(())
}

fn create_painter(ctx: egui::Context) -> Result<Painter> {
    let mut configuration = WgpuConfiguration::default();
    let power_preference = match std::env::var("CRYXTAL_POWER_PREF") {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "high" | "high_performance" | "high-performance" => {
                wgpu::PowerPreference::HighPerformance
            }
            "default" => wgpu::PowerPreference::default(),
            _ => wgpu::PowerPreference::LowPower,
        },
        Err(_) => wgpu::PowerPreference::LowPower,
    };
    configuration.wgpu_setup = WgpuSetup::CreateNew(WgpuSetupCreateNew {
        power_preference,
        device_descriptor: Arc::new(|adapter| {
            let required_limits =
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
            wgpu::DeviceDescriptor {
                label: Some("cryxtal-view"),
                required_features: wgpu::Features::empty(),
                required_limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::default(),
            }
        }),
        ..Default::default()
    });

    let painter = pollster::block_on(Painter::new(
        ctx,
        configuration,
        false,
        RendererOptions::default(),
    ));
    Ok(painter)
}

struct CryxtalApp {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    wall_params: WallParams,
    opening_params: WallOpeningParams,
    rebar_params: RebarParams,
    tool_mode: ToolMode,
    pending_wall_start: Option<Point3>,
    pending_rebar_start: Option<Point3>,
    selected: Option<usize>,
    last_selected: Option<usize>,
    hovered: Option<usize>,
    elements: Vec<BimElement>,
    element_meshes: Vec<ViewerMesh>,
    element_polymeshes: Vec<PolygonMesh>,
    model_info: Option<ModelInfo>,
    viewer: ViewerState,
    viewer_mesh: Option<ViewerMesh>,
    truck_renderer: TruckRenderer,
    gizmo_renderer: Option<GizmoRenderer>,
    gizmo_init_rx: Option<mpsc::Receiver<GizmoRenderer>>,
    gizmo_init_started: bool,
    frame_presented: bool,
    log: Vec<String>,
    layers: Vec<Layer>,
    active_layer: usize,
    view_mode: ViewMode,
    mesh_revision: u64,
    input: InputState,
    selection_drag_start: Option<Point2>,
    selection_drag_rect: Option<Rect>,
    selection_dragging: bool,
    suppress_click: bool,
    pending_box_select: Option<Rect>,
    last_view_distance: f64,
    last_view_pivot: (f64, f64, f64),
    view_rows_dirty: bool,
    view_rows: Vec<(String, String)>,
    last_frame: Instant,
    selected_name: String,
    show_layer_creator: bool,
    new_layer_name: String,
    new_layer_color: Color32,
    layer_creator_message: String,
    render_texture_id: Option<egui::TextureId>,
    render_texture_revision: u64,
    gizmo_texture_id: Option<egui::TextureId>,
    gizmo_texture_revision: u64,
}

impl CryxtalApp {
    fn new(adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let truck_renderer = TruckRenderer::new(adapter.clone(), device.clone(), queue.clone());
        let layers = vec![Layer {
            name: "Default".to_string(),
            color: Color32::from_rgb(180, 190, 200),
        }];
        Self {
            adapter,
            device,
            queue,
            wall_params: WallParams::default(),
            opening_params: WallOpeningParams::default(),
            rebar_params: RebarParams::default(),
            tool_mode: ToolMode::default(),
            pending_wall_start: None,
            pending_rebar_start: None,
            selected: None,
            last_selected: None,
            hovered: None,
            elements: Vec::new(),
            element_meshes: Vec::new(),
            element_polymeshes: Vec::new(),
            model_info: None,
            viewer: ViewerState::default(),
            viewer_mesh: None,
            truck_renderer,
            gizmo_renderer: None,
            gizmo_init_rx: None,
            gizmo_init_started: false,
            frame_presented: false,
            log: Vec::new(),
            layers,
            active_layer: 0,
            view_mode: ViewMode::LayerOpaque,
            mesh_revision: 0,
            input: InputState::default(),
            selection_drag_start: None,
            selection_drag_rect: None,
            selection_dragging: false,
            suppress_click: false,
            pending_box_select: None,
            last_view_distance: 0.0,
            last_view_pivot: (0.0, 0.0, 0.0),
            view_rows_dirty: true,
            view_rows: Vec::new(),
            last_frame: Instant::now(),
            selected_name: String::new(),
            show_layer_creator: false,
            new_layer_name: String::new(),
            new_layer_color: Color32::from_rgb(242, 179, 95),
            layer_creator_message: String::new(),
            render_texture_id: None,
            render_texture_revision: 0,
            gizmo_texture_id: None,
            gizmo_texture_revision: 0,
        }
    }

    fn ui(&mut self, ctx: &egui::Context, render_state: &RenderState) {
        self.try_finish_gizmo_init();
        self.start_gizmo_init_if_needed();
        self.sync_selection_on_change();
        self.update_view_rows_if_needed();

        let panel_mode = match self.tool_mode {
            ToolMode::CreateWall => "wall",
            ToolMode::CreateOpening => "opening",
            ToolMode::CreateRebar => "rebar",
            ToolMode::Select if self.selected.is_some() => "selection",
            _ => "view",
        };

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                ui.heading("CryXtal Castor");
                ui.add(egui::Separator::default().vertical());

                if ui
                    .selectable_label(self.tool_mode == ToolMode::CreateWall, "Wall")
                    .clicked()
                {
                    self.activate_wall_tool();
                }
                if ui
                    .selectable_label(self.tool_mode == ToolMode::CreateOpening, "Opening")
                    .clicked()
                {
                    self.activate_opening_tool();
                }
                if ui
                    .selectable_label(self.tool_mode == ToolMode::CreateRebar, "Rebar")
                    .clicked()
                {
                    self.activate_rebar_tool();
                }
                if ui.button("Reset View").clicked() {
                    self.viewer.reset_view();
                }
                if ui.button("Fit Model").clicked() {
                    self.fit_model();
                }
                if ui.button("Clear").clicked() {
                    self.clear_model();
                }
            });
        });

        egui::SidePanel::left("side_panel")
            .resizable(false)
            .exact_width(340.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                    ui.add_space(12.0);
                    ui.group(|ui| match panel_mode {
                        "selection" => self.selection_panel(ui),
                        "wall" => self.wall_panel(ui),
                        "opening" => self.opening_panel(ui),
                        "rebar" => self.rebar_panel(ui),
                        _ => self.view_panel(ui),
                    });
                    ui.add_space(20.0);
                });
            });

        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                ui.label("Layer");
                self.active_layer_combo(ui);
                if ui.button("New Layer").clicked() {
                    self.show_layer_creator = true;
                    self.layer_creator_message.clear();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            if response.clicked() {
                response.request_focus();
            }
            self.draw_viewport(ctx, ui, rect, response, render_state);
        });

        if self.show_layer_creator {
            self.layer_creator_modal(ctx);
        }

        self.sync_selected_name();
    }

    fn selection_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Properties");
        let category = self.selected_category();
        if !category.is_empty() {
            ui.label(category);
        }

        ui.add_space(4.0);
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut self.selected_name));

        ui.label("Layer");
        self.selected_layer_combo(ui);

        ui.add_space(8.0);
        ui.add(egui::Separator::default());
        let is_opening = self
            .selected
            .and_then(|idx| self.elements.get(idx))
            .map(|element| element.category == BimCategory::Opening)
            .unwrap_or(false);
        let is_rebar = self
            .selected
            .and_then(|idx| self.elements.get(idx))
            .map(|element| element.category == BimCategory::Rebar)
            .unwrap_or(false);
        if is_opening {
            self.opening_properties_panel(ui);
        } else if is_rebar {
            self.rebar_properties_panel(ui);
        } else {
            ui.label("Parameters");
            for (key, value) in self.selection_rows() {
                ui.label(format!("{key}: {value}"));
            }
        }
    }

    fn wall_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Wall Tool");

        ui.label("Thickness");
        ui.add(
            egui::DragValue::new(&mut self.wall_params.thickness)
                .range(10.0..=100000.0)
                .speed(1.0)
                .fixed_decimals(0),
        );

        ui.label("Height");
        ui.add(
            egui::DragValue::new(&mut self.wall_params.height)
                .range(10.0..=100000.0)
                .speed(1.0)
                .fixed_decimals(0),
        );

        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut self.wall_params.name));

        ui.label(self.wall_status_text());

        if ui.button("Cancel Wall").clicked() {
            self.cancel_wall();
        }
    }

    fn view_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("View");
        for (key, value) in &self.view_rows {
            ui.horizontal(|ui| {
                ui.label(format!("{key}:"));
                ui.label(value);
            });
        }
        ui.add_space(8.0);
        ui.label("Gizmo");
        let mode = self.viewer.gizmo_mode();
        ui.horizontal(|ui| {
            if ui.selectable_label(mode == GizmoMode::Cube, "Cube").clicked() {
                self.viewer.set_gizmo_mode(GizmoMode::Cube);
            }
            if ui.selectable_label(mode == GizmoMode::Axis, "Axis").clicked() {
                self.viewer.set_gizmo_mode(GizmoMode::Axis);
            }
        });
    }

    fn draw_viewport(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        response: egui::Response,
        render_state: &RenderState,
    ) {
        let bg = ui.visuals().panel_fill;
        ui.painter().rect_filled(rect, 0.0, bg);

        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let hovered = pointer_pos.map(|pos| rect.contains(pos)).unwrap_or(false);
        if hovered {
            response.clone().on_hover_cursor(egui::CursorIcon::None);
        }
        self.update_input(ctx, rect, hovered, response.has_focus());
        if response.double_clicked() {
            self.input.double_clicked = true;
        }

        
        let viewport_rect = Rect::from_min_size(
            Point2::new(0.0, 0.0),
            Vec2::new(rect.width(), rect.height()),
        );

        let dark_mode = ctx.style().visuals.dark_mode;
        self.tick_viewport(
            viewport_rect,
            hovered,
            ctx.pixels_per_point(),
            render_state,
            dark_mode,
        );

        if let Some(texture_id) = self.render_texture_id {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(texture_id, rect, uv, egui::Color32::WHITE);
        }

        if self.viewer.gizmo_mode() == GizmoMode::Cube {
            if let Some(texture_id) = self.gizmo_texture_id {
                let gizmo_rect = self.viewer.gizmo_rect(viewport_rect);
                let gizmo_rect = to_egui_rect(gizmo_rect, rect.min.to_vec2());
                let size = gizmo_rect.width().min(gizmo_rect.height());
                let center = gizmo_rect.center();
                let radius = size * 0.5;
                let bg = egui::Color32::from_rgba_unmultiplied(20, 22, 28, 200);
                let border = egui::Color32::from_rgba_unmultiplied(90, 95, 100, 220);
                ui.painter().circle_filled(center, radius, bg);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture_id, gizmo_rect, uv, egui::Color32::WHITE);
                ui.painter()
                    .circle_stroke(center, radius, egui::Stroke::new(1.0, border));
            }
        }

        let overlay_painter = ui.painter().with_clip_rect(rect);
        let mut overlay = EguiOverlayPainter::new(&overlay_painter, rect.min.to_vec2());
        let snap_active = matches!(
            self.tool_mode,
            ToolMode::CreateWall | ToolMode::CreateOpening | ToolMode::CreateRebar
        ) || self.viewer.is_pivot_pick_active(self.input.key_v_down);
        self.viewer.paint_overlay(
            &mut overlay,
            viewport_rect,
            &self.element_meshes,
            self.selected,
            self.view_mode,
            snap_active,
            self.input.pointer_pos,
            self.viewer.gizmo_mode() == GizmoMode::Axis,
        );
        let element_visibility = self.element_visibility();
        paint_hover_outline(
            &self.viewer,
            &mut overlay,
            viewport_rect,
            &self.element_meshes,
            &self.elements,
            self.hovered,
            self.selected,
            &element_visibility,
        );

        if self.tool_mode == ToolMode::Select {
            if let Some(selection) = self.selection_drag_rect {
                let fill = Color32::from_rgba_unmultiplied(120, 170, 255, 40);
                let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 170, 255, 160));
                overlay.rect_filled(selection, 2.0, fill);
                overlay.rect_stroke(selection, 2.0, stroke);
            }
        }
    }

    fn tick_viewport(
        &mut self,
        rect: Rect,
        hovered: bool,
        pixels_per_point: f32,
        render_state: &RenderState,
        dark_mode: bool,
    ) {
        self.try_finish_gizmo_init();
        self.start_gizmo_init_if_needed();
        let now = Instant::now();
        let mut dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        if !dt.is_finite() || dt <= 0.0 {
            dt = 0.016;
        }
        dt = dt.clamp(0.0, 0.1);

        if let Some(selection) = self.pending_box_select.take() {
            self.apply_box_selection(selection, rect);
        }

        let input = self.build_input(rect, hovered);
        let consumed = self.viewer.handle_input(&input, &self.element_meshes);
        self.update_hovered(rect, hovered);

        if !consumed && input.primary_clicked && !input.modifiers.ctrl {
            if let Some(pos) = input.pointer_pos {
                self.handle_viewport_click(pos, rect);
            }
        }
        self.viewer.update(dt);

        let element_colors = self.element_colors();
        let element_visibility = self.element_visibility();
        let element_wireframe = self.element_wireframe();
        let element_skeleton_solid = self.element_skeleton_solid();
        let bounds = self.viewer_mesh.as_ref().and_then(|mesh| mesh.bounds);
        let rendered = self.truck_renderer.render(
            rect,
            pixels_per_point,
            &self.viewer,
            bounds,
            &self.element_meshes,
            &self.element_polymeshes,
            self.mesh_revision,
            &element_colors,
            &element_visibility,
            &element_wireframe,
            &element_skeleton_solid,
            self.hovered,
            self.selected,
            self.view_mode,
        );
        if rendered {
            self.sync_render_texture(render_state);
        }

        let gizmo_rendered = self
            .gizmo_renderer
            .as_mut()
            .map(|renderer| {
                renderer.render(
                    rect,
                    pixels_per_point,
                    &self.viewer,
                    self.input.pointer_pos,
                    dark_mode,
                )
            })
            .unwrap_or(false);
        if gizmo_rendered {
            self.sync_gizmo_texture(render_state);
        }

        self.update_view_rows_if_needed();

        self.input.primary_clicked = false;
        self.input.double_clicked = false;
        self.input.scroll_delta = 0.0;
        self.input.key_v_pressed = false;
    }

    fn update_input(
        &mut self,
        ctx: &egui::Context,
        rect: egui::Rect,
        hovered: bool,
        focused: bool,
    ) {
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        self.input.pointer_pos = pointer_pos.map(|pos| {
            Point2::new(pos.x - rect.min.x, pos.y - rect.min.y)
        });

        let delta = ctx.input(|i| i.pointer.delta());
        self.input.pointer_delta = if hovered {
            Vec2::new(delta.x, delta.y)
        } else {
            Vec2::new(0.0, 0.0)
        };

        let modifiers = ctx.input(|i| i.modifiers);
        self.input.modifiers = Modifiers {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
        };

        self.input.primary_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        self.input.secondary_down =
            ctx.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
        self.input.middle_down =
            ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));

        if hovered {
            let scroll = ctx.input(|i| i.raw_scroll_delta);
            self.input.scroll_delta += scroll.y;
        }

        if hovered
            && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            self.suppress_click = false;
            if self.tool_mode == ToolMode::Select {
                self.selection_drag_start = self.input.pointer_pos;
                self.selection_drag_rect = None;
                self.selection_dragging = false;
            } else {
                self.clear_selection_drag();
            }
        }

        if self.tool_mode == ToolMode::Select && self.input.primary_down {
            if let (Some(start), Some(pos)) = (self.selection_drag_start, self.input.pointer_pos) {
                let delta = pos - start;
                if !self.selection_dragging
                    && (delta.x.abs() > SELECTION_DRAG_THRESHOLD
                        || delta.y.abs() > SELECTION_DRAG_THRESHOLD)
                {
                    self.selection_dragging = true;
                }
                if self.selection_dragging {
                    self.selection_drag_rect = Some(Rect::from_points(start, pos));
                }
            }
        }

        if ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            if self.selection_dragging {
                if let Some(selection) = self.selection_drag_rect {
                    self.pending_box_select = Some(selection);
                    self.suppress_click = true;
                }
            } else if hovered && !self.suppress_click {
                self.input.primary_clicked = true;
            }
            self.selection_drag_start = None;
            self.selection_drag_rect = None;
            self.selection_dragging = false;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.tool_mode = ToolMode::Select;
            self.clear_selection_drag();
            self.pending_wall_start = None;
            self.pending_rebar_start = None;
            self.viewer.cancel_interaction();
        }

        if focused {

            if modifiers.ctrl {
                if ctx.input(|i| i.key_pressed(egui::Key::Num1)) {
                    self.view_mode = ViewMode::Skeleton;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num2)) {
                    self.view_mode = ViewMode::LayerOpaque;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num3)) {
                    self.view_mode = ViewMode::LayerTransparent;
                } else if ctx.input(|i| i.key_pressed(egui::Key::Num4)) {
                    self.view_mode = ViewMode::Material;
                }
            }

            self.input.key_v_pressed = ctx.input(|i| i.key_pressed(egui::Key::V));
            self.input.key_v_down = ctx.input(|i| i.key_down(egui::Key::V));
        } else {
            self.input.key_v_pressed = false;
            self.input.key_v_down = false;
        }
    }

    fn build_input(&mut self, rect: Rect, hovered: bool) -> ViewerInput {
        let pointer_pos = if hovered { self.input.pointer_pos } else { None };
        let delta = if hovered {
            self.input.pointer_delta
        } else {
            Vec2::new(0.0, 0.0)
        };
        self.input.pointer_delta = Vec2::new(0.0, 0.0);

        ViewerInput {
            rect,
            pointer_pos,
            pointer_delta: delta,
            primary_down: self.input.primary_down,
            secondary_down: self.input.secondary_down,
            middle_down: self.input.middle_down,
            primary_clicked: self.input.primary_clicked,
            double_clicked: self.input.double_clicked,
            scroll_delta: self.input.scroll_delta,
            modifiers: self.input.modifiers,
            hovered,
            key_v_pressed: self.input.key_v_pressed,
            key_v_down: self.input.key_v_down,
        }
    }

    fn sync_render_texture(&mut self, render_state: &RenderState) {
        let revision = self.truck_renderer.target_revision();
        if self.render_texture_revision == revision && self.render_texture_id.is_some() {
            return;
        }

        let view = self.truck_renderer.target_view();
        let mut renderer = render_state.renderer.write();
        let texture_id = if let Some(id) = self.render_texture_id {
            renderer.update_egui_texture_from_wgpu_texture(
                &render_state.device,
                view,
                wgpu::FilterMode::Linear,
                id,
            );
            id
        } else {
            renderer.register_native_texture(
                &render_state.device,
                view,
                wgpu::FilterMode::Linear,
            )
        };
        self.render_texture_id = Some(texture_id);
        self.render_texture_revision = revision;
    }

    fn sync_gizmo_texture(&mut self, render_state: &RenderState) {
        let Some(gizmo_renderer) = &self.gizmo_renderer else {
            return;
        };
        let revision = gizmo_renderer.target_revision();
        if self.gizmo_texture_revision == revision && self.gizmo_texture_id.is_some() {
            return;
        }

        let view = gizmo_renderer.target_view();
        let mut renderer = render_state.renderer.write();
        let texture_id = if let Some(id) = self.gizmo_texture_id {
            renderer.update_egui_texture_from_wgpu_texture(
                &render_state.device,
                view,
                wgpu::FilterMode::Linear,
                id,
            );
            id
        } else {
            renderer.register_native_texture(
                &render_state.device,
                view,
                wgpu::FilterMode::Linear,
            )
        };
        self.gizmo_texture_id = Some(texture_id);
        self.gizmo_texture_revision = revision;
    }

    fn try_finish_gizmo_init(&mut self) {
        let Some(rx) = &self.gizmo_init_rx else {
            return;
        };
        if let Ok(renderer) = rx.try_recv() {
            self.gizmo_renderer = Some(renderer);
            self.gizmo_init_rx = None;
        }
    }

    fn start_gizmo_init_if_needed(&mut self) {
        if self.gizmo_renderer.is_some()
            || self.gizmo_init_started
            || !self.frame_presented
            || self.viewer.gizmo_mode() != GizmoMode::Cube
        {
            return;
        }
        let (tx, rx) = mpsc::channel::<GizmoRenderer>();
        let adapter = self.adapter.clone();
        let device = self.device.clone();
        let queue = self.queue.clone();
        self.gizmo_init_started = true;
        self.gizmo_init_rx = Some(rx);
        thread::spawn(move || {
            let renderer = GizmoRenderer::new(adapter, device, queue);
            let _ = tx.send(renderer);
        });
    }

    fn on_frame_presented(&mut self) {
        self.frame_presented = true;
        self.start_gizmo_init_if_needed();
    }
}

impl CryxtalApp {
    fn activate_wall_tool(&mut self) {
        self.tool_mode = ToolMode::CreateWall;
        self.clear_selection_drag();
        self.pending_wall_start = None;
        self.set_selected(None);
    }

    fn cancel_wall(&mut self) {
        self.tool_mode = ToolMode::Select;
        self.clear_selection_drag();
        self.pending_wall_start = None;
        self.viewer.cancel_interaction();
    }

    fn clear_model(&mut self) {
        self.elements.clear();
        self.rebuild_scene();
        self.set_selected(None);
        self.clear_selection_drag();
        self.pending_wall_start = None;
        self.pending_rebar_start = None;
        self.push_log("Model cleared".to_string());
    }

    fn set_active_layer(&mut self, index: usize) {
        if index < self.layers.len() {
            self.active_layer = index;
        }
    }

    fn set_element_layer(&mut self, index: usize) {
        let Some(selected) = self.selected else {
            return;
        };
        if index >= self.layers.len() {
            return;
        }
        if let Some(element) = self.elements.get_mut(selected) {
            let name = self.layers[index].name.clone();
            element.insert_parameter("Layer", ParameterValue::Text(name));
        }
    }

    fn create_layer(&mut self) {
        let name = self.new_layer_name.trim().to_string();
        if name.is_empty() {
            self.layer_creator_message = "Layer name is empty".to_string();
            return;
        }
        if self.layers.iter().any(|layer| layer.name == name) {
            self.layer_creator_message = "Layer name already exists".to_string();
            return;
        }
        let color = self.new_layer_color;
        self.layers.push(Layer { name, color });
        self.active_layer = self.layers.len().saturating_sub(1);
        self.show_layer_creator = false;
        self.new_layer_name.clear();
        self.layer_creator_message.clear();
    }

    fn cancel_layer_creator(&mut self) {
        self.show_layer_creator = false;
        self.new_layer_name.clear();
        self.layer_creator_message.clear();
    }

    fn fit_model(&mut self) {
        if let Some(mesh) = &self.viewer_mesh {
            if let Some(bounds) = mesh.bounds {
                self.viewer.fit_bounds(bounds);
            }
        }
    }

    fn handle_viewport_click(&mut self, pos: Point2, rect: Rect) {
        match self.tool_mode {
            ToolMode::Select => {
                if let Some(index) = self.hovered {
                    self.set_selected(Some(index));
                    return;
                }
                if let Some((index, _point)) =
                    self.viewer.pick_element(pos, rect, &self.element_meshes)
                {
                    self.set_selected(Some(index));
                } else {
                    self.set_selected(None);
                }
            }
            ToolMode::CreateWall => {
                if let Some(point) = self.viewer.pick_point(pos, rect, &self.element_meshes, true) {
                    let point = Point3::new(point.x, point.y, point.z);
                    let name = self.wall_params.name.clone();

                    if let Some(start) = self.pending_wall_start {
                        match build_wall_between_points(
                            start,
                            point,
                            self.wall_params.thickness,
                            self.wall_params.height,
                            Some(&name),
                        ) {
                            Ok(element) => {
                                self.pending_wall_start = None;
                                self.add_elements(vec![element], "Wall added", false);
                            }
                            Err(err) => self.push_log(format!("Wall build failed: {err}")),
                        }
                    } else {
                        self.pending_wall_start = Some(point);
                        self.push_log("Wall start set".to_string());
                    }
                }
            }
            ToolMode::CreateOpening => {
                self.handle_opening_click(pos, rect);
            }
            ToolMode::CreateRebar => {
                self.handle_rebar_click(pos, rect);
            }
        }
    }

    fn apply_box_selection(&mut self, selection: Rect, viewport: Rect) {
        if self.tool_mode != ToolMode::Select {
            return;
        }
        if selection.width() < SELECTION_DRAG_THRESHOLD
            && selection.height() < SELECTION_DRAG_THRESHOLD
        {
            return;
        }
        self.set_selected(self.viewer.pick_element_rect(viewport, selection, &self.element_meshes));
    }

    fn clear_selection_drag(&mut self) {
        self.selection_drag_start = None;
        self.selection_drag_rect = None;
        self.selection_dragging = false;
        self.pending_box_select = None;
        self.suppress_click = false;
    }
}

impl CryxtalApp {
    fn sync_selection_on_change(&mut self) {
        if self.selected == self.last_selected {
            return;
        }
        self.last_selected = self.selected;
        self.selected_name = self
            .selected
            .and_then(|idx| self.elements.get(idx).map(|element| element.name.clone()))
            .unwrap_or_default();

        if let Some(selected) = self.selected {
            let active = self
                .layers
                .get(self.active_layer)
                .map(|layer| layer.name.clone())
                .unwrap_or_else(|| "Default".to_string());
            if let Some(element) = self.elements.get_mut(selected) {
                if element.parameters.get("Layer").is_none() {
                    element.insert_parameter("Layer", ParameterValue::Text(active));
                }
            }
        }
    }

    fn selected_category(&self) -> String {
        let Some(selected) = self.selected else {
            return String::new();
        };
        self.elements
            .get(selected)
            .map(|element| format!("{:?}", element.category))
            .unwrap_or_default()
    }

    fn selected_layer_index(&self) -> Option<usize> {
        let Some(selected) = self.selected else {
            return None;
        };
        let element = self.elements.get(selected)?;
        let layer_name = match element.parameters.get("Layer") {
            Some(ParameterValue::Text(value)) => value.as_str(),
            _ => "",
        };
        if layer_name.is_empty() {
            return None;
        }
        self.layers.iter().position(|layer| layer.name == layer_name)
    }

    fn selection_rows(&self) -> Vec<(String, String)> {
        let Some(selected) = self.selected else {
            return Vec::new();
        };
        let Some(element) = self.elements.get(selected) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for (key, value) in &element.parameters {
            if key == "Layer" {
                continue;
            }
            rows.push((key.clone(), format!("{value:?}")));
        }
        rows
    }

    fn update_view_rows_if_needed(&mut self) {
        let distance = self.viewer.distance();
        let pivot = self.viewer.pivot_position();
        let distance_changed = (distance - self.last_view_distance).abs() > 1.0e-2;
        let pivot_changed = (pivot.x - self.last_view_pivot.0).abs() > 1.0e-2
            || (pivot.y - self.last_view_pivot.1).abs() > 1.0e-2
            || (pivot.z - self.last_view_pivot.2).abs() > 1.0e-2;

        if self.view_rows_dirty || distance_changed || pivot_changed {
            self.last_view_distance = distance;
            self.last_view_pivot = (pivot.x, pivot.y, pivot.z);
            self.view_rows_dirty = false;
            self.update_view_rows_model();
        }
    }

    fn update_view_rows_model(&mut self) {
        let mut rows = Vec::new();
        rows.push((
            "Camera distance".to_string(),
            format!("{:.2}", self.viewer.distance()),
        ));
        let pivot = self.viewer.pivot_position();
        rows.push((
            "Pivot".to_string(),
            format!("{:.2}, {:.2}, {:.2}", pivot.x, pivot.y, pivot.z),
        ));

        if let Some(info) = &self.model_info {
            rows.push(("Model".to_string(), info.label.clone()));
            rows.push(("Elements".to_string(), info.elements.to_string()));
            rows.push(("Vertices".to_string(), info.vertices.to_string()));
            rows.push(("Faces".to_string(), info.faces.to_string()));
            if let Some((min, max)) = info.bounds {
                let size = Point3::new(max.x - min.x, max.y - min.y, max.z - min.z);
                rows.push(("Bounds min".to_string(), format_point(&min)));
                rows.push(("Bounds max".to_string(), format_point(&max)));
                rows.push(("Size".to_string(), format_point(&size)));
            }
        }

        self.view_rows = rows;
    }

    fn wall_status_text(&self) -> String {
        if self.tool_mode != ToolMode::CreateWall {
            return String::new();
        }
        if let Some(start) = self.pending_wall_start {
            format!("Start: {:.2}, {:.2}, {:.2}", start.x, start.y, start.z)
        } else {
            "Click first point in the 3D view.".to_string()
        }
    }

    fn sync_selected_name(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        let Some(element) = self.elements.get_mut(selected) else {
            return;
        };
        if element.name != self.selected_name {
            element.name = self.selected_name.clone();
        }
    }

    fn element_colors(&self) -> Vec<Color32> {
        let default_color = self
            .layers
            .first()
            .map(|layer| layer.color)
            .unwrap_or_else(|| Color32::from_rgb(180, 190, 200));
        self.elements
            .iter()
            .map(|element| {
                let layer_name = match element.parameters.get("Layer") {
                    Some(ParameterValue::Text(value)) => value.as_str(),
                    _ => "",
                };
                self.layers
                    .iter()
                    .find(|layer| layer.name == layer_name)
                    .map(|layer| layer.color)
                    .unwrap_or(default_color)
            })
            .collect()
    }

    fn element_visibility(&self) -> Vec<bool> {
        self.elements
            .iter()
            .map(|element| element.category != BimCategory::Opening)
            .collect()
    }

    fn element_wireframe(&self) -> Vec<bool> {
        self.elements.iter().map(|_| true).collect()
    }

    fn element_skeleton_solid(&self) -> Vec<bool> {
        self.elements
            .iter()
            .map(|element| element.category == BimCategory::Rebar)
            .collect()
    }


    fn add_elements(&mut self, mut elements: Vec<BimElement>, log_label: &str, select_last: bool) {
        let active_layer = self
            .layers
            .get(self.active_layer)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "Default".to_string());
        for element in &mut elements {
            element.insert_parameter("Layer", ParameterValue::Text(active_layer.clone()));
        }
        let was_empty = self.elements.is_empty();
        self.elements.append(&mut elements);
        self.rebuild_scene();
        if select_last {
            if !self.elements.is_empty() {
                self.set_selected(Some(self.elements.len() - 1));
            } else {
                self.set_selected(None);
            }
        }
        if was_empty {
            if let Some(bounds) = self.viewer_mesh.as_ref().and_then(|mesh| mesh.bounds) {
                self.viewer.fit_bounds(bounds);
            }
        }
        self.push_log(log_label.to_string());
    }

    fn rebuild_scene(&mut self) {
        self.viewer.invalidate_snap_cache();
        if self.elements.is_empty() {
            self.viewer_mesh = None;
            self.model_info = None;
            self.element_meshes.clear();
            self.element_polymeshes.clear();
            self.set_selected(None);
            self.mesh_revision = self.mesh_revision.wrapping_add(1);
            self.view_rows_dirty = true;
            return;
        }

        let mut meshes = Vec::new();
        let mut poly_meshes = Vec::new();
        let mut bounds: Option<(Point3, Point3)> = None;
        let mut total_vertices = 0usize;
        let mut total_faces = 0usize;

        if self.elements.len() <= 1 {
            for element in &self.elements {
                let mesh = triangulate_solid(element.geometry(), DEFAULT_TESSELLATION_TOLERANCE);
                total_vertices += mesh.positions().len();
                total_faces += mesh.faces().len();
                bounds = merge_bounds(bounds, mesh_bounds(mesh.positions()));
                let mut viewer_mesh = ViewerMesh::from_mesh(&mesh);
                if element.category == BimCategory::Rebar {
                    tune_rebar_wireframe(&mut viewer_mesh);
                }
                poly_meshes.push(mesh);
                meshes.push(viewer_mesh);
            }
        } else {
            let (tx, rx) = mpsc::channel::<MeshBuildResult>();
            thread::scope(|scope| {
                for (idx, element) in self.elements.iter().enumerate() {
                    let element = element.clone();
                    let tx = tx.clone();
                    scope.spawn(move || {
                        let mesh =
                            triangulate_solid(element.geometry(), DEFAULT_TESSELLATION_TOLERANCE);
                        let vertices = mesh.positions().len();
                        let faces = mesh.faces().len();
                        let bounds = mesh_bounds(mesh.positions());
                        let mut viewer_mesh = ViewerMesh::from_mesh(&mesh);
                        if element.category == BimCategory::Rebar {
                            tune_rebar_wireframe(&mut viewer_mesh);
                        }
                        let _ = tx.send(MeshBuildResult {
                            idx,
                            viewer_mesh,
                            poly_mesh: mesh,
                            bounds,
                            vertices,
                            faces,
                        });
                    });
                }
            });
            drop(tx);

            let mut results = Vec::with_capacity(self.elements.len());
            for result in rx {
                results.push(result);
            }
            results.sort_by_key(|result| result.idx);

            for result in results {
                total_vertices += result.vertices;
                total_faces += result.faces;
                bounds = merge_bounds(bounds, result.bounds);
                poly_meshes.push(result.poly_mesh);
                meshes.push(result.viewer_mesh);
            }
        }

        self.element_meshes = meshes;
        self.element_polymeshes = poly_meshes;
        self.viewer_mesh = ViewerMesh::merge(&self.element_meshes);
        self.mesh_revision = self.mesh_revision.wrapping_add(1);
        let label = if self.elements.len() == 1 {
            self.elements[0].name.clone()
        } else {
            format!("Scene ({})", self.elements.len())
        };
        self.model_info = Some(ModelInfo {
            label,
            elements: self.elements.len(),
            vertices: total_vertices,
            faces: total_faces,
            bounds,
        });
        self.view_rows_dirty = true;

        if let Some(selected) = self.selected {
            if selected >= self.elements.len() {
                self.set_selected(None);
            }
        }
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() > 200 {
            self.log.remove(0);
        }
        self.log.push(line);
    }

    fn active_layer_combo(&mut self, ui: &mut egui::Ui) {
        let current = self
            .layers
            .get(self.active_layer)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "No layers".to_string());

        egui::ComboBox::from_id_source("active_layer_combo")
            .selected_text(current)
            .show_ui(ui, |ui| {
                let mut next = None;
                for (idx, layer) in self.layers.iter().enumerate() {
                    if ui.selectable_label(idx == self.active_layer, &layer.name).clicked() {
                        next = Some(idx);
                    }
                }
                if let Some(idx) = next {
                    self.set_active_layer(idx);
                }
            });

    }

    fn selected_layer_combo(&mut self, ui: &mut egui::Ui) {
        let Some(_) = self.selected else {
            let mut placeholder = String::new();
            ui.add_enabled(false, egui::TextEdit::singleline(&mut placeholder));
            return;
        };
        let current_index = self.selected_layer_index();
        let current = current_index
            .and_then(|idx| self.layers.get(idx).map(|layer| layer.name.clone()))
            .unwrap_or_else(|| "No layers".to_string());

        egui::ComboBox::from_id_source("selected_layer_combo")
            .selected_text(current)
            .show_ui(ui, |ui| {
                let mut next = None;
                for (idx, layer) in self.layers.iter().enumerate() {
                    let selected_row = current_index == Some(idx);
                    if ui.selectable_label(selected_row, &layer.name).clicked() {
                        next = Some(idx);
                    }
                }
                if let Some(idx) = next {
                    self.set_element_layer(idx);
                    self.last_selected = None;
                }
            });
    }

    fn layer_creator_modal(&mut self, ctx: &egui::Context) {
        let mut open = self.show_layer_creator;
        egui::Window::new("Create Layer")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.heading("Create Layer");
                ui.add_space(6.0);

                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.new_layer_name));

                ui.add_space(6.0);
                ui.label("Color");
                let mut color = to_egui_color(self.new_layer_color);
                if egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                )
                .changed()
                {
                    let [r, g, b, a] = color.to_array();
                    self.new_layer_color = Color32::from_rgba_unmultiplied(r, g, b, a);
                }

                if !self.layer_creator_message.is_empty() {
                    ui.label(&self.layer_creator_message);
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        self.create_layer();
                    }
                    if ui.button("Cancel").clicked() {
                        self.cancel_layer_creator();
                    }
                });
            });

        if !open {
            self.show_layer_creator = false;
        }
    }

    fn set_selected(&mut self, selected: Option<usize>) {
        self.selected = selected;
        self.last_selected = None;
    }
}

struct EguiOverlayPainter<'a> {
    painter: &'a egui::Painter,
    offset: egui::Vec2,
}

impl<'a> EguiOverlayPainter<'a> {
    fn new(painter: &'a egui::Painter, offset: egui::Vec2) -> Self {
        Self { painter, offset }
    }
}

impl OverlayPainter for EguiOverlayPainter<'_> {
    fn rect_filled(&mut self, rect: Rect, radius: f32, fill: Color32) {
        let egui_rect = to_egui_rect(rect, self.offset);
        self.painter
            .rect_filled(egui_rect, radius, to_egui_color(fill));
    }

    fn rect_stroke(&mut self, rect: Rect, radius: f32, stroke: Stroke) {
        let egui_rect = to_egui_rect(rect, self.offset);
        let stroke = egui::Stroke::new(stroke.width, to_egui_color(stroke.color));
        self.painter
            .rect_stroke(egui_rect, radius, stroke, egui::StrokeKind::Inside);
    }

    fn line_segment(&mut self, start: Point2, end: Point2, stroke: Stroke) {
        let points = [
            to_egui_pos(start, self.offset),
            to_egui_pos(end, self.offset),
        ];
        let stroke = egui::Stroke::new(stroke.width, to_egui_color(stroke.color));
        self.painter.line_segment(points, stroke);
    }

    fn circle_filled(&mut self, center: Point2, radius: f32, fill: Color32) {
        let center = to_egui_pos(center, self.offset);
        self.painter
            .circle_filled(center, radius, to_egui_color(fill));
    }

    fn circle_stroke(&mut self, center: Point2, radius: f32, stroke: Stroke) {
        let center = to_egui_pos(center, self.offset);
        let stroke = egui::Stroke::new(stroke.width, to_egui_color(stroke.color));
        self.painter.circle_stroke(center, radius, stroke);
    }

    fn polygon(&mut self, points: Vec<Point2>, fill: Color32, stroke: Stroke) {
        let points: Vec<egui::Pos2> =
            points.into_iter().map(|p| to_egui_pos(p, self.offset)).collect();
        let stroke = egui::Stroke::new(stroke.width, to_egui_color(stroke.color));
        self.painter.add(egui::Shape::convex_polygon(
            points,
            to_egui_color(fill),
            stroke,
        ));
    }

    fn text(&mut self, pos: Point2, align: ViewerAlign2, text: String, size: f32, color: Color32) {
        let pos = to_egui_pos(pos, self.offset);
        let align = match align {
            ViewerAlign2::LeftTop => egui::Align2::LEFT_TOP,
            ViewerAlign2::CenterCenter => egui::Align2::CENTER_CENTER,
        };
        self.painter.text(
            pos,
            align,
            text,
            FontId::proportional(size),
            to_egui_color(color),
        );
    }
}

fn to_egui_pos(pos: Point2, offset: egui::Vec2) -> egui::Pos2 {
    egui::pos2(pos.x + offset.x, pos.y + offset.y)
}

fn to_egui_rect(rect: Rect, offset: egui::Vec2) -> egui::Rect {
    let min = to_egui_pos(rect.min, offset);
    let max = to_egui_pos(rect.max, offset);
    egui::Rect::from_min_max(min, max)
}

fn to_egui_color(color: Color32) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}
