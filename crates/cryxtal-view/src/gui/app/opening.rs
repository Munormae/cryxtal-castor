use cryxtal_bim::{BimCategory, BimElement, ParameterValue};
use cryxtal_topology::Point3;
use egui::Ui;

use crate::elements::{
    apply_wall_opening, build_opening_element, rebuild_wall_from_openings, sync_opening_from_wall,
};
use crate::viewer::{Point2, Rect};

use super::{CryxtalApp, ToolMode};

impl CryxtalApp {
    pub(super) fn opening_panel(&mut self, ui: &mut Ui) {
        ui.heading("Wall Opening");

        ui.label("Width");
        ui.add(
            egui::DragValue::new(&mut self.opening_params.width)
                .range(10.0..=100000.0)
                .speed(1.0)
                .fixed_decimals(0),
        );

        ui.label("Height");
        ui.add(
            egui::DragValue::new(&mut self.opening_params.height)
                .range(10.0..=100000.0)
                .speed(1.0)
                .fixed_decimals(0),
        );

        ui.label(self.opening_status_text());

        if ui.button("Cancel Opening").clicked() {
            self.cancel_opening();
        }
    }

    pub(super) fn opening_properties_panel(&mut self, ui: &mut Ui) {
        let Some(selected) = self.selected else {
            return;
        };
        let Some(opening) = self.elements.get(selected) else {
            return;
        };
        if opening.category != BimCategory::Opening {
            return;
        }

        let Some(opening_index) = opening_index(opening) else {
            ui.label("Opening index is missing.");
            return;
        };
        let Some(mut width) = opening_number(opening, "Width") else {
            ui.label("Opening width is missing.");
            return;
        };
        let Some(mut height) = opening_number(opening, "Height") else {
            ui.label("Opening height is missing.");
            return;
        };
        let Some(mut center_x) = opening_number(opening, "CenterX") else {
            ui.label("Opening center X is missing.");
            return;
        };
        let Some(mut center_z) = opening_number(opening, "CenterZ") else {
            ui.label("Opening center Z is missing.");
            return;
        };

        ui.heading("Opening Properties");
        ui.label(format!("Index: {opening_index}"));
        ui.label(format!("Host: {}", opening_host_label(opening)));

        ui.add_space(6.0);
        ui.label("Width");
        let changed_width = ui
            .add(
                egui::DragValue::new(&mut width)
                    .range(10.0..=100000.0)
                    .speed(1.0)
                    .fixed_decimals(0),
            )
            .changed();

        ui.label("Height");
        let changed_height = ui
            .add(
                egui::DragValue::new(&mut height)
                    .range(10.0..=100000.0)
                    .speed(1.0)
                    .fixed_decimals(0),
            )
            .changed();

        ui.label("Center X");
        let changed_center_x = ui
            .add(
                egui::DragValue::new(&mut center_x)
                    .range(0.0..=100000.0)
                    .speed(1.0)
                    .fixed_decimals(0),
            )
            .changed();

        ui.label("Center Z");
        let changed_center_z = ui
            .add(
                egui::DragValue::new(&mut center_z)
                    .range(0.0..=100000.0)
                    .speed(1.0)
                    .fixed_decimals(0),
            )
            .changed();

        if changed_width || changed_height || changed_center_x || changed_center_z {
            self.apply_opening_edits(
                selected,
                opening_index,
                width,
                height,
                center_x,
                center_z,
            );
        }
    }

    fn opening_status_text(&self) -> String {
        if self.tool_mode != ToolMode::CreateOpening {
            return String::new();
        }
        let has_wall_selected = self
            .selected
            .and_then(|idx| self.elements.get(idx))
            .map(|element| element.category == BimCategory::Wall)
            .unwrap_or(false);
        if has_wall_selected {
            "Click the opening center on the wall.".to_string()
        } else {
            "Select a wall or click one to place the opening center.".to_string()
        }
    }

    pub(super) fn activate_opening_tool(&mut self) {
        self.tool_mode = ToolMode::CreateOpening;
        self.clear_selection_drag();
        self.pending_wall_start = None;
    }

    fn cancel_opening(&mut self) {
        self.tool_mode = ToolMode::Select;
        self.clear_selection_drag();
        self.viewer.cancel_interaction();
    }

    pub(super) fn handle_opening_click(&mut self, pos: Point2, rect: Rect) {
        let picked = self.viewer.pick_element(pos, rect, &self.element_meshes);
        let Some((index, picked_point)) = picked else {
            self.push_log("No element under cursor".to_string());
            return;
        };

        let host_index = match self.elements.get(index) {
            Some(element) if element.category == BimCategory::Wall => Some(index),
            Some(element) if element.category == BimCategory::Opening => {
                self.opening_host_index(element)
            }
            _ => None,
        };

        let Some(host_index) = host_index else {
            self.push_log("Opening tool expects a wall".to_string());
            return;
        };

        let snapped = match self.element_meshes.get(host_index) {
            Some(mesh) => self
                .viewer
                .pick_point(pos, rect, std::slice::from_ref(mesh), true)
                .unwrap_or(picked_point),
            None => picked_point,
        };
        let point = Point3::new(snapped.x, snapped.y, snapped.z);

        let Some(host) = self.elements.get_mut(host_index) else {
            return;
        };

        let data = match apply_wall_opening(
            host,
            point,
            self.opening_params.width,
            self.opening_params.height,
        ) {
            Ok(data) => data,
            Err(err) => {
                self.push_log(format!("Opening failed: {err}"));
                return;
            }
        };

        let host_snapshot = match self.elements.get(host_index).cloned() {
            Some(element) => element,
            None => return,
        };

        let mut opening_element = match build_opening_element(&host_snapshot, &data) {
            Ok(element) => element,
            Err(err) => {
                self.push_log(format!("Opening build failed: {err}"));
                return;
            }
        };
        opening_element.insert_parameter(
            "HostIndex",
            ParameterValue::Integer(host_index as i64),
        );

        self.add_opening_element(opening_element, host_index);
    }

    fn add_opening_element(&mut self, mut element: BimElement, host_index: usize) {
        let host_layer = self
            .elements
            .get(host_index)
            .and_then(|host| match host.parameters.get("Layer") {
                Some(ParameterValue::Text(value)) => Some(value.clone()),
                _ => None,
            });
        let fallback_layer = self
            .layers
            .get(self.active_layer)
            .map(|layer| layer.name.clone())
            .unwrap_or_else(|| "Default".to_string());
        let layer = host_layer.unwrap_or(fallback_layer);
        element.insert_parameter("Layer", ParameterValue::Text(layer));
        self.elements.push(element);
        self.sync_openings_for_wall(host_index);
        self.rebuild_scene();
        if !self.elements.is_empty() {
            self.set_selected(Some(self.elements.len() - 1));
        }
        self.push_log("Opening added".to_string());
    }

    fn apply_opening_edits(
        &mut self,
        opening_idx: usize,
        opening_index: usize,
        width: f64,
        height: f64,
        center_x: f64,
        center_z: f64,
    ) {
        let host_index = self
            .elements
            .get(opening_idx)
            .and_then(|opening| self.opening_host_index(opening));
        let Some(host_index) = host_index else {
            self.push_log("Opening host wall not found".to_string());
            return;
        };

        let Some(host) = self.elements.get(host_index).cloned() else {
            return;
        };

        let mut candidate = host.clone();
        update_wall_opening_params(
            &mut candidate,
            opening_index,
            width,
            height,
            center_x,
            center_z,
        );
        if let Err(err) = rebuild_wall_from_openings(&mut candidate) {
            self.push_log(format!("Opening update failed: {err}"));
            return;
        }

        if let Some(host_mut) = self.elements.get_mut(host_index) {
            host_mut.parameters = candidate.parameters;
            host_mut.geometry = candidate.geometry;
        }

        self.sync_openings_for_wall(host_index);
        self.rebuild_scene();
    }

    fn sync_openings_for_wall(&mut self, host_index: usize) {
        let Some(host) = self.elements.get(host_index).cloned() else {
            return;
        };
        let host_guid = host.guid.to_string();

        let opening_indices: Vec<usize> = self
            .elements
            .iter()
            .enumerate()
            .filter_map(|(idx, element)| {
                if element.category != BimCategory::Opening {
                    return None;
                }
                let guid_match = opening_host_guid(element)
                    .map(|guid| guid == host_guid)
                    .unwrap_or(false);
                let index_match = opening_host_index_param(element) == Some(host_index);
                if guid_match || index_match {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        let host_layer = match host.parameters.get("Layer") {
            Some(ParameterValue::Text(value)) => Some(value.clone()),
            _ => None,
        };

        for idx in opening_indices {
            if let Some(opening) = self.elements.get_mut(idx) {
                if let Some(layer) = host_layer.clone() {
                    opening.insert_parameter("Layer", ParameterValue::Text(layer));
                }
                opening.insert_parameter(
                    "HostIndex",
                    ParameterValue::Integer(host_index as i64),
                );
                if let Err(err) = sync_opening_from_wall(opening, &host) {
                    self.push_log(format!("Opening sync failed: {err}"));
                }
            }
        }
    }

    fn opening_host_index(&self, opening: &BimElement) -> Option<usize> {
        if let Some(ParameterValue::Integer(value)) = opening.parameters.get("HostIndex") {
            let index = *value as usize;
            if self
                .elements
                .get(index)
                .map(|element| element.category == BimCategory::Wall)
                .unwrap_or(false)
            {
                return Some(index);
            }
        }

        let guid = opening_host_guid(opening)?;
        self.elements.iter().position(|element| {
            element.category == BimCategory::Wall && element.guid.to_string() == guid
        })
    }
}

fn opening_number(opening: &BimElement, key: &str) -> Option<f64> {
    match opening.parameters.get(key) {
        Some(ParameterValue::Number(value)) => Some(*value),
        _ => None,
    }
}

fn opening_index(opening: &BimElement) -> Option<usize> {
    match opening.parameters.get("OpeningIndex") {
        Some(ParameterValue::Integer(value)) if *value > 0 => Some(*value as usize),
        _ => None,
    }
}

fn opening_host_guid(opening: &BimElement) -> Option<&str> {
    match opening.parameters.get("HostGuid") {
        Some(ParameterValue::Text(value)) => Some(value.as_str()),
        _ => None,
    }
}

fn opening_host_index_param(opening: &BimElement) -> Option<usize> {
    match opening.parameters.get("HostIndex") {
        Some(ParameterValue::Integer(value)) if *value >= 0 => Some(*value as usize),
        _ => None,
    }
}

fn opening_host_label(opening: &BimElement) -> String {
    if let Some(ParameterValue::Text(value)) = opening.parameters.get("HostName") {
        if !value.trim().is_empty() {
            return value.clone();
        }
    }
    opening_host_guid(opening).unwrap_or("Unknown").to_string()
}

fn update_wall_opening_params(
    host: &mut BimElement,
    index: usize,
    width: f64,
    height: f64,
    center_x: f64,
    center_z: f64,
) {
    let count = match host.parameters.get("OpeningCount") {
        Some(ParameterValue::Integer(value)) if *value > 0 => *value as usize,
        _ => 0,
    };
    if index > count {
        host.insert_parameter(
            "OpeningCount",
            ParameterValue::Integer(index as i64),
        );
    }

    let prefix = format!("Opening{index}");
    host.insert_parameter(format!("{prefix}Width"), ParameterValue::Number(width));
    host.insert_parameter(format!("{prefix}Height"), ParameterValue::Number(height));
    host.insert_parameter(format!("{prefix}CenterX"), ParameterValue::Number(center_x));
    host.insert_parameter(format!("{prefix}CenterZ"), ParameterValue::Number(center_z));
}
