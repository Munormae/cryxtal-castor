use thiserror::Error;
use truck_modeling::{Rad, builder};

pub use truck_modeling::{Curve, Edge, Face, Point3, Shell, Solid, Surface, Vector3, Vertex, Wire};

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error(transparent)]
    Modeling(#[from] truck_modeling::errors::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct SolidBuilder;

impl SolidBuilder {
    pub fn box_solid(width: f64, height: f64, depth: f64) -> Result<Solid> {
        ensure_positive("width", width)?;
        ensure_positive("height", height)?;
        ensure_positive("depth", depth)?;

        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::unit_x() * width);
        let f = builder::tsweep(&e, Vector3::unit_y() * height);
        Ok(builder::tsweep(&f, Vector3::unit_z() * depth))
    }

    pub fn plate(width: f64, height: f64, thickness: f64) -> Result<Solid> {
        ensure_positive("width", width)?;
        ensure_positive("height", height)?;
        ensure_positive("thickness", thickness)?;

        let face = rectangle_face(width, height, 0.0)?;
        Ok(builder::tsweep(&face, Vector3::unit_z() * thickness))
    }

    pub fn cylinder_z(center: Point3, radius: f64, height: f64) -> Result<Solid> {
        ensure_positive("radius", radius)?;
        ensure_positive("height", height)?;

        let face = circle_face(center, radius)?;
        Ok(builder::tsweep(&face, Vector3::unit_z() * height))
    }
}

fn rectangle_face(width: f64, height: f64, z: f64) -> Result<Face> {
    let v0 = builder::vertex(Point3::new(0.0, 0.0, z));
    let v1 = builder::vertex(Point3::new(width, 0.0, z));
    let v2 = builder::vertex(Point3::new(width, height, z));
    let v3 = builder::vertex(Point3::new(0.0, height, z));

    let wire: Wire = vec![
        builder::line(&v0, &v1),
        builder::line(&v1, &v2),
        builder::line(&v2, &v3),
        builder::line(&v3, &v0),
    ]
    .into();

    Ok(builder::try_attach_plane(&[wire])?)
}

fn circle_face(center: Point3, radius: f64) -> Result<Face> {
    let v = builder::vertex(Point3::new(center.x + radius, center.y, center.z));
    let wire = builder::rsweep(
        &v,
        center,
        Vector3::unit_z(),
        Rad(std::f64::consts::PI * 2.0),
        32,
    );
    Ok(builder::try_attach_plane(&[wire])?)
}

fn ensure_positive(name: &str, value: f64) -> Result<()> {
    if value <= 0.0 {
        return Err(Error::InvalidParameter(format!("{name} must be > 0")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_solid_exists() -> Result<()> {
        let solid = SolidBuilder::box_solid(100.0, 200.0, 300.0)?;
        assert!(solid.face_iter().count() > 0);
        Ok(())
    }
}
