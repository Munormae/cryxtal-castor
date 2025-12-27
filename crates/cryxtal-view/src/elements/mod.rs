use anyhow::{Context, Result};
use cryxtal_base::Guid;
use cryxtal_bim::{BimCategory, BimElement, ParameterSet, ParameterValue};
use cryxtal_shapeops::{DEFAULT_SHAPEOPS_TOLERANCE, plate_with_hole};
use cryxtal_topology::SolidBuilder;
#[cfg(feature = "gui")]
use cryxtal_topology::Point3;
#[cfg(feature = "gui")]
use cryxtal_topology::Vector3;
#[cfg(feature = "gui")]
use truck_modeling::builder;
#[cfg(feature = "gui")]
use truck_modeling::Rad;

#[cfg(feature = "gui")]
mod wall_opening;
#[cfg(feature = "gui")]
mod opening_outline;
#[cfg(feature = "gui")]
mod rebar;
#[cfg(feature = "gui")]
pub use wall_opening::{
    apply_wall_opening, build_opening_element, opening_index_at_point,
    rebuild_wall_from_openings, sync_opening_from_wall,
};
#[cfg(feature = "gui")]
pub use opening_outline::opening_outline_points;
#[cfg(feature = "gui")]
pub use rebar::{apply_rebar_edit, build_rebar_between_points, rebar_data};

pub fn build_box_element(
    width: f64,
    height: f64,
    depth: f64,
    name: Option<&str>,
) -> Result<BimElement> {
    let solid =
        SolidBuilder::box_solid(width, height, depth).context("failed to build box solid")?;

    let mut parameters = ParameterSet::new();
    parameters.insert("Width".to_string(), ParameterValue::Number(width));
    parameters.insert("Height".to_string(), ParameterValue::Number(height));
    parameters.insert("Depth".to_string(), ParameterValue::Number(depth));

    let element_name = match name {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => "Box".to_string(),
    };

    Ok(BimElement::new(
        Guid::new(),
        element_name,
        BimCategory::Generic,
        parameters,
        solid,
    ))
}

pub fn build_plate_element(
    width: f64,
    height: f64,
    thickness: f64,
    hole: f64,
    material: Option<&str>,
    name: Option<&str>,
) -> Result<BimElement> {
    let solid = plate_with_hole(width, height, thickness, hole, DEFAULT_SHAPEOPS_TOLERANCE)
        .context("failed to build plate with hole")?;

    let mut parameters = ParameterSet::new();
    parameters.insert("Width".to_string(), ParameterValue::Number(width));
    parameters.insert("Height".to_string(), ParameterValue::Number(height));
    parameters.insert("Thickness".to_string(), ParameterValue::Number(thickness));
    parameters.insert("HoleDiameter".to_string(), ParameterValue::Number(hole));
    if let Some(value) = material {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            parameters.insert(
                "Material".to_string(),
                ParameterValue::Text(trimmed.to_string()),
            );
        }
    }

    let element_name = match name {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => "PlateWithHole".to_string(),
    };

    Ok(BimElement::new(
        Guid::new(),
        element_name,
        BimCategory::Slab,
        parameters,
        solid,
    ))
}

#[cfg(feature = "gui")]
pub fn build_wall_between_points(
    start: Point3,
    end: Point3,
    thickness: f64,
    height: f64,
    name: Option<&str>,
) -> Result<BimElement> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 1.0e-6 {
        anyhow::bail!("wall length is too small");
    }

    let solid = SolidBuilder::box_solid(length, thickness, height)
        .context("failed to build wall solid")?;
    let solid = builder::translated(&solid, Vector3::new(0.0, -thickness * 0.5, 0.0));
    let angle = dy.atan2(dx);
    let solid = builder::rotated(
        &solid,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::unit_z(),
        Rad(angle),
    );
    let solid = builder::translated(
        &solid,
        Vector3::new(start.x, start.y, start.z),
    );

    let mut parameters = ParameterSet::new();
    parameters.insert("Length".to_string(), ParameterValue::Number(length));
    parameters.insert("Thickness".to_string(), ParameterValue::Number(thickness));
    parameters.insert("Height".to_string(), ParameterValue::Number(height));
    parameters.insert("StartX".to_string(), ParameterValue::Number(start.x));
    parameters.insert("StartY".to_string(), ParameterValue::Number(start.y));
    parameters.insert("StartZ".to_string(), ParameterValue::Number(start.z));
    parameters.insert("EndX".to_string(), ParameterValue::Number(end.x));
    parameters.insert("EndY".to_string(), ParameterValue::Number(end.y));
    parameters.insert("EndZ".to_string(), ParameterValue::Number(end.z));

    let element_name = match name {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => "Wall".to_string(),
    };

    Ok(BimElement::new(
        Guid::new(),
        element_name,
        BimCategory::Wall,
        parameters,
        solid,
    ))
}
