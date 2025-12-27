use super::ui::{Align2, Color32, Point2, Rect, Stroke};

#[derive(Clone, Debug)]
pub enum OverlayShape {
    Rect {
        rect: Rect,
        fill: Option<Color32>,
        stroke: Option<Stroke>,
        radius: f32,
    },
    Line {
        start: Point2,
        end: Point2,
        stroke: Stroke,
    },
    Circle {
        center: Point2,
        radius: f32,
        fill: Option<Color32>,
        stroke: Option<Stroke>,
    },
    Polygon {
        points: Vec<Point2>,
        fill: Option<Color32>,
        stroke: Option<Stroke>,
    },
    Text {
        pos: Point2,
        align: Align2,
        text: String,
        size: f32,
        color: Color32,
    },
}

pub trait OverlayPainter {
    fn rect_filled(&mut self, rect: Rect, radius: f32, fill: Color32);
    fn rect_stroke(&mut self, rect: Rect, radius: f32, stroke: Stroke);
    fn line_segment(&mut self, start: Point2, end: Point2, stroke: Stroke);
    fn circle_filled(&mut self, center: Point2, radius: f32, fill: Color32);
    fn circle_stroke(&mut self, center: Point2, radius: f32, stroke: Stroke);
    fn polygon(&mut self, points: Vec<Point2>, fill: Color32, stroke: Stroke);
    fn text(&mut self, pos: Point2, align: Align2, text: String, size: f32, color: Color32);
}

#[derive(Default)]
pub struct OverlayCollector {
    pub shapes: Vec<OverlayShape>,
}

impl OverlayPainter for OverlayCollector {
    fn rect_filled(&mut self, rect: Rect, radius: f32, fill: Color32) {
        self.shapes.push(OverlayShape::Rect {
            rect,
            fill: Some(fill),
            stroke: None,
            radius,
        });
    }

    fn rect_stroke(&mut self, rect: Rect, radius: f32, stroke: Stroke) {
        self.shapes.push(OverlayShape::Rect {
            rect,
            fill: None,
            stroke: Some(stroke),
            radius,
        });
    }

    fn line_segment(&mut self, start: Point2, end: Point2, stroke: Stroke) {
        self.shapes.push(OverlayShape::Line { start, end, stroke });
    }

    fn circle_filled(&mut self, center: Point2, radius: f32, fill: Color32) {
        self.shapes.push(OverlayShape::Circle {
            center,
            radius,
            fill: Some(fill),
            stroke: None,
        });
    }

    fn circle_stroke(&mut self, center: Point2, radius: f32, stroke: Stroke) {
        self.shapes.push(OverlayShape::Circle {
            center,
            radius,
            fill: None,
            stroke: Some(stroke),
        });
    }

    fn polygon(&mut self, points: Vec<Point2>, fill: Color32, stroke: Stroke) {
        self.shapes.push(OverlayShape::Polygon {
            points,
            fill: Some(fill),
            stroke: Some(stroke),
        });
    }

    fn text(&mut self, pos: Point2, align: Align2, text: String, size: f32, color: Color32) {
        self.shapes.push(OverlayShape::Text {
            pos,
            align,
            text,
            size,
            color,
        });
    }
}
