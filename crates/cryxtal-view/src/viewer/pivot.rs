use super::math::Vec3;
use super::overlay::OverlayPainter;
use super::ui::{Color32, Point2, Stroke, Vec2};

#[derive(Clone, Debug)]
pub struct PivotState {
    position: Vec3,
    pick_mode: bool,
}

impl Default for PivotState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            pick_mode: false,
        }
    }
}

impl PivotState {
    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    pub fn arm_pick(&mut self) {
        self.pick_mode = true;
    }

    pub fn is_pick_active(&self, key_down: bool) -> bool {
        self.pick_mode || key_down
    }

    pub fn disarm_pick(&mut self) {
        self.pick_mode = false;
    }

    pub fn draw<F, P>(&self, painter: &mut P, mut project: F)
    where
        F: FnMut(Vec3) -> Option<(Point2, f64)>,
        P: OverlayPainter,
    {
        if let Some((pos, _)) = project(self.position) {
            let stroke = Stroke::new(1.5, Color32::from_gray(230));
            let size = 6.0;
            painter.line_segment(
                pos + Vec2::new(-size, 0.0),
                pos + Vec2::new(size, 0.0),
                stroke,
            );
            painter.line_segment(
                pos + Vec2::new(0.0, -size),
                pos + Vec2::new(0.0, size),
                stroke,
            );
            painter.circle_stroke(pos, size * 0.8, stroke);
        }
    }
}
