use super::ui::{Point2, Rect, Vec2};

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewerInput {
    pub rect: Rect,
    pub pointer_pos: Option<Point2>,
    pub pointer_delta: Vec2,
    pub primary_down: bool,
    pub secondary_down: bool,
    pub middle_down: bool,
    pub primary_clicked: bool,
    pub double_clicked: bool,
    pub scroll_delta: f32,
    pub modifiers: Modifiers,
    pub hovered: bool,
    pub key_v_pressed: bool,
    pub key_v_down: bool,
}
