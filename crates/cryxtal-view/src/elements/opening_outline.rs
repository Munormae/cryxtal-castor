use cryxtal_bim::{BimCategory, BimElement, ParameterValue};
use cryxtal_topology::Point3;

pub fn opening_outline_points(
    opening: &BimElement,
    elements: &[BimElement],
) -> Option<[Point3; 4]> {
    if opening.category != BimCategory::Opening {
        return None;
    }

    let width = read_number(opening, "Width")?;
    let height = read_number(opening, "Height")?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let center_x = read_number(opening, "CenterX")?;
    let center_z = read_number(opening, "CenterZ")?;
    let host = find_opening_host(opening, elements)?;
    let (start_x, start_y, start_z, end_x, end_y) = wall_start_end(host)?;

    let angle = (end_y - start_y).atan2(end_x - start_x);
    let cos = angle.cos();
    let sin = angle.sin();
    let half_width = width * 0.5;
    let half_height = height * 0.5;

    let local = [
        (center_x - half_width, center_z - half_height),
        (center_x - half_width, center_z + half_height),
        (center_x + half_width, center_z + half_height),
        (center_x + half_width, center_z - half_height),
    ];

    let to_world = |x: f64, z: f64| -> Point3 {
        let dx = x * cos;
        let dy = x * sin;
        Point3::new(start_x + dx, start_y + dy, start_z + z)
    };

    Some([
        to_world(local[0].0, local[0].1),
        to_world(local[1].0, local[1].1),
        to_world(local[2].0, local[2].1),
        to_world(local[3].0, local[3].1),
    ])
}

fn read_number(element: &BimElement, key: &str) -> Option<f64> {
    match element.parameters.get(key) {
        Some(ParameterValue::Number(value)) => Some(*value),
        _ => None,
    }
}

fn wall_start_end(host: &BimElement) -> Option<(f64, f64, f64, f64, f64)> {
    Some((
        read_number(host, "StartX")?,
        read_number(host, "StartY")?,
        read_number(host, "StartZ")?,
        read_number(host, "EndX")?,
        read_number(host, "EndY")?,
    ))
}

fn find_opening_host<'a>(opening: &BimElement, elements: &'a [BimElement]) -> Option<&'a BimElement> {
    if let Some(ParameterValue::Integer(value)) = opening.parameters.get("HostIndex") {
        if *value >= 0 {
            if let Some(host) = elements.get(*value as usize) {
                if host.category == BimCategory::Wall {
                    return Some(host);
                }
            }
        }
    }

    let guid = match opening.parameters.get("HostGuid") {
        Some(ParameterValue::Text(value)) => value.as_str(),
        _ => return None,
    };
    elements
        .iter()
        .find(|element| element.category == BimCategory::Wall && element.guid.to_string() == guid)
}
