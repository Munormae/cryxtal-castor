use cryxtal_topology::{Point3, Solid, SolidBuilder};
use thiserror::Error;

pub const DEFAULT_SHAPEOPS_TOLERANCE: f64 = 0.05;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("boolean operation failed")]
    BooleanFailed,
    #[error(transparent)]
    Topology(#[from] cryxtal_topology::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn difference(base: &Solid, tool: &Solid, tol: f64) -> Result<Solid> {
    if tol <= 0.0 {
        return Err(Error::InvalidParameter("tolerance must be > 0".to_string()));
    }

    let mut inverted_tool = tool.clone();
    inverted_tool.not();

    let result = truck_shapeops::and(base, &inverted_tool, tol).ok_or(Error::BooleanFailed)?;
    Ok(result)
}

pub fn union(base: &Solid, tool: &Solid, tol: f64) -> Result<Solid> {
    if tol <= 0.0 {
        return Err(Error::InvalidParameter("tolerance must be > 0".to_string()));
    }

    truck_shapeops::or(base, tool, tol).ok_or(Error::BooleanFailed)
}

pub fn plate_with_hole(
    width: f64,
    height: f64,
    thickness: f64,
    hole_diameter: f64,
    tol: f64,
) -> Result<Solid> {
    if hole_diameter <= 0.0 {
        return Err(Error::InvalidParameter(
            "hole_diameter must be > 0".to_string(),
        ));
    }
    if hole_diameter >= width.min(height) {
        return Err(Error::InvalidParameter(
            "hole_diameter must be smaller than width and height".to_string(),
        ));
    }

    let plate = SolidBuilder::plate(width, height, thickness)?;
    let radius = hole_diameter * 0.5;
    let clearance = thickness * 0.1;
    let center = Point3::new(width * 0.5, height * 0.5, -clearance);
    let cylinder = SolidBuilder::cylinder_z(center, radius, thickness + 2.0 * clearance)?;

    difference(&plate, &cylinder, tol)
}
