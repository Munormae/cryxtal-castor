pub use truck_geometry::base::{Point2, Point3, Vector2, Vector3};

pub mod curves {
    pub use truck_geometry::nurbs::{BSplineCurve, KnotVec};
    pub use truck_geometry::specifieds::Line;
}

pub mod surfaces {
    pub use truck_geometry::nurbs::BSplineSurface;
    pub use truck_geometry::specifieds::{Plane, Sphere};
}

pub mod profiles {
    use truck_geometry::base::Point2;

    #[derive(Clone, Copy, Debug)]
    pub struct RectangleProfile {
        pub width: f64,
        pub height: f64,
    }

    impl RectangleProfile {
        pub fn corners(&self) -> [Point2; 4] {
            [
                Point2::new(0.0, 0.0),
                Point2::new(self.width, 0.0),
                Point2::new(self.width, self.height),
                Point2::new(0.0, self.height),
            ]
        }
    }
}
