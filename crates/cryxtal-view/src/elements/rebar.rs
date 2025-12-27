use anyhow::{Context, Result};
use cryxtal_base::Guid;
use cryxtal_bim::{BimCategory, BimElement, ParameterSet, ParameterValue};
use cryxtal_shapeops::{DEFAULT_SHAPEOPS_TOLERANCE, union};
use cryxtal_topology::{Point3, Solid, SolidBuilder, Vector3};
use truck_modeling::{builder, Rad};

#[derive(Clone, Debug)]
pub struct RebarData {
    pub points: Vec<Point3>,
    pub diameter: f64,
    pub length: f64,
}

pub fn build_rebar_between_points(
    start: Point3,
    end: Point3,
    diameter: f64,
    name: Option<&str>,
) -> Result<BimElement> {
    let points = vec![start, end];
    let data = rebar_data_from_points(&points, diameter)?;
    let solid = build_rebar_solid(&data.points, data.diameter)?;

    let mut parameters = ParameterSet::new();
    write_rebar_parameters(&mut parameters, &data);

    let element_name = match name {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => "Rebar".to_string(),
    };

    Ok(BimElement::new(
        Guid::new(),
        element_name,
        BimCategory::Rebar,
        parameters,
        solid,
    ))
}

pub fn apply_rebar_edit(
    element: &mut BimElement,
    points: &[Point3],
    diameter: f64,
) -> Result<RebarData> {
    if element.category != BimCategory::Rebar {
        anyhow::bail!("rebar edit expects a rebar element");
    }
    let data = rebar_data_from_points(points, diameter)?;
    element.geometry = build_rebar_solid(&data.points, data.diameter)?;
    write_rebar_parameters(&mut element.parameters, &data);
    Ok(data)
}

pub fn rebar_data(element: &BimElement) -> Result<RebarData> {
    if element.category != BimCategory::Rebar {
        anyhow::bail!("rebar data expects a rebar element");
    }
    let points = read_rebar_points(element)?;
    let diameter = read_number(element, "Diameter")?;
    rebar_data_from_points(&points, diameter)
}

fn rebar_data_from_points(points: &[Point3], diameter: f64) -> Result<RebarData> {
    if diameter <= 0.0 {
        anyhow::bail!("rebar diameter must be > 0");
    }
    if points.len() < 2 {
        anyhow::bail!("rebar must have at least 2 points");
    }
    let mut length = 0.0;
    for window in points.windows(2) {
        let start = window[0];
        let end = window[1];
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let dz = end.z - start.z;
        let seg_len = (dx * dx + dy * dy + dz * dz).sqrt();
        if seg_len <= 1.0e-6 {
            anyhow::bail!("rebar segment is too short");
        }
        length += seg_len;
    }
    Ok(RebarData {
        points: points.to_vec(),
        diameter,
        length,
    })
}

fn build_rebar_solid(points: &[Point3], diameter: f64) -> Result<Solid> {
    let mut segments = points.windows(2);
    let Some(first) = segments.next() else {
        anyhow::bail!("rebar must have at least 2 points");
    };
    let mut solid = build_rebar_segment(first[0], first[1], diameter)?;
    for segment in segments {
        let next_solid = build_rebar_segment(segment[0], segment[1], diameter)?;
        solid = union(&solid, &next_solid, DEFAULT_SHAPEOPS_TOLERANCE)
            .context("failed to union rebar segments")?;
    }
    Ok(solid)
}

fn build_rebar_segment(start: Point3, end: Point3, diameter: f64) -> Result<Solid> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let dz = end.z - start.z;
    let length = (dx * dx + dy * dy + dz * dz).sqrt();
    if length <= 1.0e-6 {
        anyhow::bail!("rebar segment length is too small");
    }
    let radius = diameter * 0.5;
    let mut solid = SolidBuilder::cylinder_z(Point3::new(0.0, 0.0, 0.0), radius, length)
        .context("failed to build rebar segment")?;
    let dir = Vector3::new(dx, dy, dz);
    let (axis, angle) = rotation_from_z(dir, length);
    if angle.abs() > 1.0e-8 {
        solid = builder::rotated(&solid, Point3::new(0.0, 0.0, 0.0), axis, Rad(angle));
    }
    solid = builder::translated(
        &solid,
        Vector3::new(start.x, start.y, start.z),
    );
    Ok(solid)
}

fn rotation_from_z(dir: Vector3, length: f64) -> (Vector3, f64) {
    let nx = dir.x / length;
    let ny = dir.y / length;
    let nz = dir.z / length;
    let dot = nz.clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle <= 1.0e-8 {
        return (Vector3::new(0.0, 0.0, 1.0), 0.0);
    }
    let axis = Vector3::new(-ny, nx, 0.0);
    let axis_len = (axis.x * axis.x + axis.y * axis.y + axis.z * axis.z).sqrt();
    if axis_len <= 1.0e-8 {
        return (Vector3::new(1.0, 0.0, 0.0), std::f64::consts::PI);
    }
    (
        Vector3::new(axis.x / axis_len, axis.y / axis_len, axis.z / axis_len),
        angle,
    )
}

fn write_rebar_parameters(parameters: &mut ParameterSet, data: &RebarData) {
    clear_rebar_params(parameters);
    parameters.insert(
        "PointCount".to_string(),
        ParameterValue::Integer(data.points.len() as i64),
    );
    for (index, point) in data.points.iter().enumerate() {
        let idx = index + 1;
        parameters.insert(
            format!("Point{idx}X"),
            ParameterValue::Number(point.x),
        );
        parameters.insert(
            format!("Point{idx}Y"),
            ParameterValue::Number(point.y),
        );
        parameters.insert(
            format!("Point{idx}Z"),
            ParameterValue::Number(point.z),
        );
    }
    let start = data.points.first().copied().unwrap_or(Point3::new(0.0, 0.0, 0.0));
    let end = data.points.last().copied().unwrap_or(start);
    parameters.insert("StartX".to_string(), ParameterValue::Number(start.x));
    parameters.insert("StartY".to_string(), ParameterValue::Number(start.y));
    parameters.insert("StartZ".to_string(), ParameterValue::Number(start.z));
    parameters.insert("EndX".to_string(), ParameterValue::Number(end.x));
    parameters.insert("EndY".to_string(), ParameterValue::Number(end.y));
    parameters.insert("EndZ".to_string(), ParameterValue::Number(end.z));
    parameters.insert("Diameter".to_string(), ParameterValue::Number(data.diameter));
    parameters.insert("Length".to_string(), ParameterValue::Number(data.length));
}

fn read_number(element: &BimElement, key: &str) -> Result<f64> {
    match element.parameters.get(key) {
        Some(ParameterValue::Number(value)) => Ok(*value),
        _ => anyhow::bail!("missing or invalid rebar parameter: {key}"),
    }
}

fn read_rebar_points(element: &BimElement) -> Result<Vec<Point3>> {
    if let Some(ParameterValue::Integer(value)) = element.parameters.get("PointCount") {
        let count = *value as usize;
        if count < 2 {
            anyhow::bail!("rebar must have at least 2 points");
        }
        let mut points = Vec::with_capacity(count);
        for idx in 1..=count {
            let x = read_number(element, &format!("Point{idx}X"))?;
            let y = read_number(element, &format!("Point{idx}Y"))?;
            let z = read_number(element, &format!("Point{idx}Z"))?;
            points.push(Point3::new(x, y, z));
        }
        return Ok(points);
    }

    let start = Point3::new(
        read_number(element, "StartX")?,
        read_number(element, "StartY")?,
        read_number(element, "StartZ")?,
    );
    let end = Point3::new(
        read_number(element, "EndX")?,
        read_number(element, "EndY")?,
        read_number(element, "EndZ")?,
    );
    Ok(vec![start, end])
}

fn clear_rebar_params(parameters: &mut ParameterSet) {
    let keys: Vec<String> = parameters
        .keys()
        .filter(|key| is_rebar_param_key(key))
        .cloned()
        .collect();
    for key in keys {
        parameters.remove(&key);
    }
}

fn is_rebar_param_key(key: &str) -> bool {
    key == "PointCount"
        || key == "StartX"
        || key == "StartY"
        || key == "StartZ"
        || key == "EndX"
        || key == "EndY"
        || key == "EndZ"
        || key == "Diameter"
        || key == "Length"
        || (key.starts_with("Point")
            && (key.ends_with('X') || key.ends_with('Y') || key.ends_with('Z')))
}
