use cryxtal_bim::{BimCategory, ParameterValue};
use cryxtal_topology::Point3;

use crate::elements::opening_index_at_point;
use crate::viewer::Rect;

use super::CryxtalApp;

impl CryxtalApp {
    pub(super) fn update_hovered(&mut self, rect: Rect, hovered: bool) {
        if !hovered
            || self.input.primary_down
            || self.input.secondary_down
            || self.input.middle_down
            || self.selection_dragging
        {
            self.hovered = None;
            return;
        }

        let Some(pos) = self.input.pointer_pos else {
            self.hovered = None;
            return;
        };

        let pick = self.viewer.pick_element(pos, rect, &self.element_meshes);
        let Some((index, hit_point)) = pick else {
            self.hovered = None;
            return;
        };

        let Some(element) = self.elements.get(index) else {
            self.hovered = None;
            return;
        };

        if element.category == BimCategory::Opening {
            self.hovered = Some(index);
            return;
        }

        if element.category == BimCategory::Wall {
            let world_point = Point3::new(hit_point.x, hit_point.y, hit_point.z);
            if let Ok(Some(opening_index)) = opening_index_at_point(element, world_point) {
                if let Some(opening_element) =
                    self.find_opening_element_index(index, opening_index)
                {
                    self.hovered = Some(opening_element);
                    return;
                }
            }
        }

        self.hovered = Some(index);
    }

    fn find_opening_element_index(&self, host_index: usize, opening_index: usize) -> Option<usize> {
        let host_guid = self.elements.get(host_index)?.guid.to_string();
        self.elements
            .iter()
            .enumerate()
            .find_map(|(idx, element)| {
                if element.category != BimCategory::Opening {
                    return None;
                }
                let matches_opening = match element.parameters.get("OpeningIndex") {
                    Some(ParameterValue::Integer(value)) if *value > 0 => {
                        *value as usize == opening_index
                    }
                    _ => false,
                };
                if !matches_opening {
                    return None;
                }
                let guid_match = match element.parameters.get("HostGuid") {
                    Some(ParameterValue::Text(value)) => value == &host_guid,
                    _ => false,
                };
                let index_match = match element.parameters.get("HostIndex") {
                    Some(ParameterValue::Integer(value)) if *value >= 0 => {
                        *value as usize == host_index
                    }
                    _ => false,
                };
                if guid_match || index_match {
                    Some(idx)
                } else {
                    None
                }
            })
    }
}
