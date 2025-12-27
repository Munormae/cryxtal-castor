#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Point2,
    pub max: Point2,
}

impl Point2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point2) -> f32 {
        (self - other).length()
    }
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Rect {
    pub fn from_min_size(min: Point2, size: Vec2) -> Self {
        Self {
            min,
            max: Point2::new(min.x + size.x, min.y + size.y),
        }
    }

    pub fn from_points(a: Point2, b: Point2) -> Self {
        let min = Point2::new(a.x.min(b.x), a.y.min(b.y));
        let max = Point2::new(a.x.max(b.x), a.y.max(b.y));
        Self { min, max }
    }

    pub fn from_center_size(center: Point2, size: Vec2) -> Self {
        let half = Vec2::new(size.x * 0.5, size.y * 0.5);
        Self {
            min: Point2::new(center.x - half.x, center.y - half.y),
            max: Point2::new(center.x + half.x, center.y + half.y),
        }
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Point2 {
        Point2::new((self.min.x + self.max.x) * 0.5, (self.min.y + self.max.y) * 0.5)
    }

    pub fn left_top(&self) -> Point2 {
        Point2::new(self.min.x, self.min.y)
    }

    pub fn right(&self) -> f32 {
        self.max.x
    }

    pub fn top(&self) -> f32 {
        self.min.y
    }

    pub fn contains(&self, pos: Point2) -> bool {
        pos.x >= self.min.x && pos.x <= self.max.x && pos.y >= self.min.y && pos.y <= self.max.y
    }

    pub fn intersects(&self, other: Rect) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }
}

impl std::ops::Add<Vec2> for Point2 {
    type Output = Point2;

    fn add(self, rhs: Vec2) -> Point2 {
        Point2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub<Point2> for Point2 {
    type Output = Vec2;

    fn sub(self, rhs: Point2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Add<Vec2> for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Vec2;

    fn mul(self, rhs: f32) -> Vec2 {
        Vec2::new(self.x * rhs, self.y * rhs)
    }
}

impl Vec2 {
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color32 {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_gray(value: u8) -> Self {
        Self {
            r: value,
            g: value,
            b: value,
            a: 255,
        }
    }

    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Stroke {
    pub width: f32,
    pub color: Color32,
}

impl Stroke {
    pub fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align2 {
    LeftTop,
    CenterCenter,
}

pub const fn pos2(x: f32, y: f32) -> Point2 {
    Point2::new(x, y)
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}
