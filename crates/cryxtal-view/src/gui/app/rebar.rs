use cryxtal_bim::BimCategory;
use cryxtal_topology::Point3;
use egui::Ui;

use crate::elements::{apply_rebar_edit, build_rebar_between_points, rebar_data};
use crate::viewer::{Point2, Rect};

use super::{CryxtalApp, ToolMode};

impl CryxtalApp {
    pub(super) fn rebar_panel(&mut self, ui: &mut Ui) {
        ui.heading("Rebar Tool");

        ui.label("Diameter");
        ui.add(
            egui::DragValue::new(&mut self.rebar_params.diameter)
                .range(2.0..=1000.0)
                .speed(1.0)
                .fixed_decimals(1),
        );

        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut self.rebar_params.name));

        ui.label(self.rebar_status_text());

        if ui.button("Cancel Rebar").clicked() {
            self.cancel_rebar();
        }
    }

    pub(super) fn rebar_properties_panel(&mut self, ui: &mut Ui) {
        let Some(selected) = self.selected else {
            return;
        };
        let Some(rebar) = self.elements.get(selected) else {
            return;
        };
        if rebar.category != BimCategory::Rebar {
            return;
        }

        let data = match rebar_data(rebar) {
            Ok(data) => data,
            Err(err) => {
                ui.label(format!("Rebar data error: {err}"));
                return;
            }
        };

        let start = data.points.first().copied().unwrap_or(Point3::new(0.0, 0.0, 0.0));
        let end = data.points.last().copied().unwrap_or(start);
        let mut start_x = start.x;
        let mut start_y = start.y;
        let mut start_z = start.z;
        let mut end_x = end.x;
        let mut end_y = end.y;
        let mut end_z = end.z;
        let mut diameter = data.diameter;

        ui.heading("Rebar Properties");
        ui.label(format!("Length: {:.1}", data.length));

        ui.add_space(6.0);
        ui.label("Start X");
        let changed_start_x = ui
            .add(
                egui::DragValue::new(&mut start_x)
                    .range(-1.0e6..=1.0e6)
                    .speed(1.0)
                    .fixed_decimals(2),
            )
            .changed();

        ui.label("Start Y");
        let changed_start_y = ui
            .add(
                egui::DragValue::new(&mut start_y)
                    .range(-1.0e6..=1.0e6)
                    .speed(1.0)
                    .fixed_decimals(2),
            )
            .changed();

        ui.label("Start Z");
        let changed_start_z = ui
            .add(
                egui::DragValue::new(&mut start_z)
                    .range(-1.0e6..=1.0e6)
                    .speed(1.0)
                    .fixed_decimals(2),
            )
            .changed();

        ui.label("End X");
        let changed_end_x = ui
            .add(
                egui::DragValue::new(&mut end_x)
                    .range(-1.0e6..=1.0e6)
                    .speed(1.0)
                    .fixed_decimals(2),
            )
            .changed();

        ui.label("End Y");
        let changed_end_y = ui
            .add(
                egui::DragValue::new(&mut end_y)
                    .range(-1.0e6..=1.0e6)
                    .speed(1.0)
                    .fixed_decimals(2),
            )
            .changed();

        ui.label("End Z");
        let changed_end_z = ui
            .add(
                egui::DragValue::new(&mut end_z)
                    .range(-1.0e6..=1.0e6)
                    .speed(1.0)
                    .fixed_decimals(2),
            )
            .changed();

        ui.label("Diameter");
        let changed_diameter = ui
            .add(
                egui::DragValue::new(&mut diameter)
                    .range(2.0..=1000.0)
                    .speed(1.0)
                    .fixed_decimals(1),
            )
            .changed();

        if changed_start_x
            || changed_start_y
            || changed_start_z
            || changed_end_x
            || changed_end_y
            || changed_end_z
            || changed_diameter
        {
            let mut points = data.points.clone();
            if points.len() >= 2 {
                points[0] = Point3::new(start_x, start_y, start_z);
                let last = points.len() - 1;
                points[last] = Point3::new(end_x, end_y, end_z);
            } else {
                points = vec![
                    Point3::new(start_x, start_y, start_z),
                    Point3::new(end_x, end_y, end_z),
                ];
            }
            self.apply_rebar_edits(selected, &points, diameter);
        }
    }

    pub(super) fn activate_rebar_tool(&mut self) {
        self.tool_mode = ToolMode::CreateRebar;
        self.clear_selection_drag();
        self.pending_rebar_start = None;
        self.set_selected(None);
    }

    fn cancel_rebar(&mut self) {
        self.tool_mode = ToolMode::Select;
        self.clear_selection_drag();
        self.pending_rebar_start = None;
        self.viewer.cancel_interaction();
    }

    pub(super) fn handle_rebar_click(&mut self, pos: Point2, rect: Rect) {
        let Some(point) = self.viewer.pick_point(pos, rect, &self.element_meshes, true) else {
            return;
        };
        let point = Point3::new(point.x, point.y, point.z);
        let name = self.rebar_params.name.clone();

        if let Some(start) = self.pending_rebar_start {
            match build_rebar_between_points(start, point, self.rebar_params.diameter, Some(&name)) {
                Ok(element) => {
                    self.pending_rebar_start = None;
                    self.add_elements(vec![element], "Rebar added", false);
                }
                Err(err) => self.push_log(format!("Rebar build failed: {err}")),
            }
        } else {
            self.pending_rebar_start = Some(point);
            self.push_log("Rebar start set".to_string());
        }
    }

    fn rebar_status_text(&self) -> String {
        if self.tool_mode != ToolMode::CreateRebar {
            return String::new();
        }
        if self.pending_rebar_start.is_some() {
            "Click the rebar end point.".to_string()
        } else {
            "Click the rebar start point.".to_string()
        }
    }

    fn apply_rebar_edits(
        &mut self,
        index: usize,
        points: &[Point3],
        diameter: f64,
    ) {
        let Some(rebar) = self.elements.get_mut(index) else {
            return;
        };
        if let Err(err) = apply_rebar_edit(rebar, points, diameter) {
            self.push_log(format!("Rebar update failed: {err}"));
            return;
        }
        self.rebuild_scene();
    }
}
