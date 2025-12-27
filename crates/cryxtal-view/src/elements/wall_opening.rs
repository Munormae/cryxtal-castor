use anyhow::{Context, Result};
use cryxtal_base::Guid;
use cryxtal_bim::{BimCategory, BimElement, ParameterSet, ParameterValue};
use cryxtal_topology::{Point3, Solid, SolidBuilder, Vector3, Wire};
use truck_modeling::{builder, Rad};

#[derive(Clone, Copy, Debug)]
pub struct OpeningData {
    pub index: usize,
    pub width: f64,
    pub height: f64,
    pub center_x: f64,
    pub center_z: f64,
}

#[derive(Clone, Copy, Debug)]
struct WallData {
    start: Point3,
    length: f64,
    thickness: f64,
    height: f64,
    angle: f64,
}

#[derive(Clone, Copy, Debug)]
struct OpeningRect {
    min_x: f64,
    max_x: f64,
    min_z: f64,
    max_z: f64,
    cut_bottom: bool,
}

pub fn apply_wall_opening(
    element: &mut BimElement,
    world_center: Point3,
    opening_width: f64,
    opening_height: f64,
) -> Result<OpeningData> {
    if element.category != BimCategory::Wall {
        anyhow::bail!("opening can only be applied to wall elements");
    }
    if opening_width <= 0.0 {
        anyhow::bail!("opening width must be > 0");
    }
    if opening_height <= 0.0 {
        anyhow::bail!("opening height must be > 0");
    }

    let wall = wall_data(element)?;
    let margin = opening_margin(wall.thickness);
    if wall.length <= margin * 2.0 {
        anyhow::bail!("wall length is too small for opening");
    }
    if wall.height <= margin * 2.0 {
        anyhow::bail!("wall height is too small for opening");
    }

    let max_width = (wall.length - margin * 2.0).max(0.0);
    let max_height = (wall.height - margin * 2.0).max(0.0);
    let opening_width = opening_width.min(max_width);
    let opening_height = opening_height.min(max_height);
    if opening_width <= 0.0 || opening_height <= 0.0 {
        anyhow::bail!("opening is too large for wall");
    }

    let local = world_to_wall_local(world_center, wall.start, wall.angle);
    let half_width = opening_width * 0.5;
    let half_height = opening_height * 0.5;
    let center_x = local
        .x
        .clamp(half_width + margin, wall.length - half_width - margin);
    let min_center_z = half_height;
    let max_center_z = (wall.height - half_height - margin).max(min_center_z);
    let center_z = local.z.clamp(min_center_z, max_center_z);

    let next_index = match element.parameters.get("OpeningCount") {
        Some(ParameterValue::Integer(value)) if *value >= 0 => (*value as usize) + 1,
        _ => 1,
    };
    element.insert_parameter(
        "OpeningCount",
        ParameterValue::Integer(next_index as i64),
    );
    let prefix = format!("Opening{next_index}");
    element.insert_parameter(format!("{prefix}Width"), ParameterValue::Number(opening_width));
    element.insert_parameter(format!("{prefix}Height"), ParameterValue::Number(opening_height));
    element.insert_parameter(format!("{prefix}CenterX"), ParameterValue::Number(center_x));
    element.insert_parameter(format!("{prefix}CenterZ"), ParameterValue::Number(center_z));

    rebuild_wall_from_openings(element)?;
    read_opening_from_wall(element, next_index)
}

pub fn rebuild_wall_from_openings(element: &mut BimElement) -> Result<()> {
    if element.category != BimCategory::Wall {
        anyhow::bail!("openings can only be applied to wall elements");
    }
    let wall = wall_data(element)?;
    let margin = opening_margin(wall.thickness);

    let openings = collect_openings(element, wall.length, wall.height, margin)?;
    ensure_openings_do_not_overlap(&openings)?;
    element.geometry = build_wall_with_openings(
        wall.start,
        wall.length,
        wall.thickness,
        wall.height,
        wall.angle,
        &openings,
    )?;

    Ok(())
}

pub fn read_opening_from_wall(element: &BimElement, index: usize) -> Result<OpeningData> {
    let prefix = format!("Opening{index}");
    let width = read_number(element, &format!("{prefix}Width"))?;
    let height = read_number(element, &format!("{prefix}Height"))?;
    let center_x = read_number(element, &format!("{prefix}CenterX"))?;
    let center_z = read_number(element, &format!("{prefix}CenterZ"))?;
    Ok(OpeningData {
        index,
        width,
        height,
        center_x,
        center_z,
    })
}

pub fn build_opening_element(host: &BimElement, data: &OpeningData) -> Result<BimElement> {
    if host.category != BimCategory::Wall {
        anyhow::bail!("host element is not a wall");
    }
    let wall = wall_data(host)?;
    let solid = build_opening_solid(&wall, data)?;

    let mut parameters = ParameterSet::new();
    parameters.insert("Width".to_string(), ParameterValue::Number(data.width));
    parameters.insert("Height".to_string(), ParameterValue::Number(data.height));
    parameters.insert("CenterX".to_string(), ParameterValue::Number(data.center_x));
    parameters.insert("CenterZ".to_string(), ParameterValue::Number(data.center_z));
    parameters.insert(
        "OpeningIndex".to_string(),
        ParameterValue::Integer(data.index as i64),
    );
    parameters.insert(
        "HostGuid".to_string(),
        ParameterValue::Text(host.guid.to_string()),
    );
    parameters.insert(
        "HostName".to_string(),
        ParameterValue::Text(host.name.clone()),
    );
    parameters.insert(
        "Thickness".to_string(),
        ParameterValue::Number(wall.thickness),
    );

    let name = format!("Opening {}", data.index);
    Ok(BimElement::new(
        Guid::new(),
        name,
        BimCategory::Opening,
        parameters,
        solid,
    ))
}

pub fn sync_opening_from_wall(opening: &mut BimElement, host: &BimElement) -> Result<()> {
    if opening.category != BimCategory::Opening {
        anyhow::bail!("syncing requires an opening element");
    }
    if host.category != BimCategory::Wall {
        anyhow::bail!("syncing requires a wall host");
    }
    let index = read_opening_index(opening)?;
    let data = read_opening_from_wall(host, index)?;
    let wall = wall_data(host)?;
    update_opening_parameters(opening, host, wall.thickness, &data);
    opening.geometry = build_opening_solid(&wall, &data)?;
    Ok(())
}

pub fn opening_index_at_point(element: &BimElement, world_point: Point3) -> Result<Option<usize>> {
    if element.category != BimCategory::Wall {
        anyhow::bail!("opening lookup expects a wall element");
    }
    let wall = wall_data(element)?;
    let local = world_to_wall_local(world_point, wall.start, wall.angle);
    let count = match element.parameters.get("OpeningCount") {
        Some(ParameterValue::Integer(value)) if *value > 0 => *value as usize,
        _ => 0,
    };
    if count == 0 {
        return Ok(None);
    }

    let eps = 1.0e-4;
    for index in 1..=count {
        let prefix = format!("Opening{index}");
        let width_key = format!("{prefix}Width");
        let height_key = format!("{prefix}Height");
        let center_x_key = format!("{prefix}CenterX");
        let center_z_key = format!("{prefix}CenterZ");

        let width = match read_number(element, &width_key) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let height = match read_number(element, &height_key) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let center_x = match read_number(element, &center_x_key) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let center_z = match read_number(element, &center_z_key) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if width <= 0.0 || height <= 0.0 {
            continue;
        }

        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let min_x = center_x - half_width - eps;
        let max_x = center_x + half_width + eps;
        let min_z = center_z - half_height - eps;
        let max_z = center_z + half_height + eps;
        if local.x >= min_x && local.x <= max_x && local.z >= min_z && local.z <= max_z {
            return Ok(Some(index));
        }
    }

    Ok(None)
}

fn update_opening_parameters(
    opening: &mut BimElement,
    host: &BimElement,
    thickness: f64,
    data: &OpeningData,
) {
    opening.insert_parameter("Width", ParameterValue::Number(data.width));
    opening.insert_parameter("Height", ParameterValue::Number(data.height));
    opening.insert_parameter("CenterX", ParameterValue::Number(data.center_x));
    opening.insert_parameter("CenterZ", ParameterValue::Number(data.center_z));
    opening.insert_parameter(
        "OpeningIndex",
        ParameterValue::Integer(data.index as i64),
    );
    opening.insert_parameter(
        "HostGuid",
        ParameterValue::Text(host.guid.to_string()),
    );
    opening.insert_parameter(
        "HostName",
        ParameterValue::Text(host.name.clone()),
    );
    opening.insert_parameter("Thickness", ParameterValue::Number(thickness));
}

fn read_opening_index(opening: &BimElement) -> Result<usize> {
    match opening.parameters.get("OpeningIndex") {
        Some(ParameterValue::Integer(value)) if *value > 0 => Ok(*value as usize),
        _ => anyhow::bail!("opening index is missing"),
    }
}

fn wall_data(element: &BimElement) -> Result<WallData> {
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
    let length = read_number(element, "Length")?;
    let thickness = read_number(element, "Thickness")?;
    let height = read_number(element, "Height")?;
    if length <= 0.0 {
        anyhow::bail!("wall length is too small");
    }
    if thickness <= 0.0 {
        anyhow::bail!("wall thickness is too small");
    }
    if height <= 0.0 {
        anyhow::bail!("wall height is too small");
    }
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let angle = dy.atan2(dx);
    Ok(WallData {
        start,
        length,
        thickness,
        height,
        angle,
    })
}

fn opening_margin(thickness: f64) -> f64 {
    (thickness * 0.02).max(1.0)
}

fn build_opening_solid(wall: &WallData, data: &OpeningData) -> Result<Solid> {
    let half_width = data.width * 0.5;
    let half_height = data.height * 0.5;
    let highlight_offset = opening_margin(wall.thickness);
    let visual_thickness = wall.thickness + highlight_offset * 2.0;
    let mut opening = SolidBuilder::box_solid(data.width, visual_thickness, data.height)
        .context("failed to build opening solid")?;
    opening = builder::translated(
        &opening,
        Vector3::new(
            data.center_x - half_width,
            -visual_thickness * 0.5,
            data.center_z - half_height,
        ),
    );
    opening = builder::rotated(
        &opening,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::unit_z(),
        Rad(wall.angle),
    );
    Ok(builder::translated(
        &opening,
        Vector3::new(wall.start.x, wall.start.y, wall.start.z),
    ))
}

fn collect_openings(
    element: &mut BimElement,
    length: f64,
    wall_height: f64,
    margin: f64,
) -> Result<Vec<OpeningRect>> {
    let count = match element.parameters.get("OpeningCount") {
        Some(ParameterValue::Integer(value)) if *value > 0 => *value as usize,
        _ => 0,
    };

    let mut openings = Vec::with_capacity(count);
    let mut updates = Vec::new();

    for index in 1..=count {
        let prefix = format!("Opening{index}");
        let width_key = format!("{prefix}Width");
        let height_key = format!("{prefix}Height");
        let center_x_key = format!("{prefix}CenterX");
        let center_z_key = format!("{prefix}CenterZ");

        let orig_width = read_number(element, &width_key)?;
        let orig_height = read_number(element, &height_key)?;
        let center_x = read_number(element, &center_x_key)?;
        let center_z = read_number(element, &center_z_key)?;

    let max_width = (length - margin * 2.0).max(0.0);
    let max_height = (wall_height - margin * 2.0).max(0.0);
        let width = orig_width.min(max_width);
        let height = orig_height.min(max_height);
        if width <= 0.0 || height <= 0.0 {
            anyhow::bail!("opening {index} is too large for wall");
        }

        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let adj_center_x =
            center_x.clamp(half_width + margin, length - half_width - margin);
        let min_center_z = half_height;
        let max_center_z = (wall_height - half_height - margin).max(min_center_z);
        let adj_center_z = center_z.clamp(min_center_z, max_center_z);

        if (width - orig_width).abs() > f64::EPSILON {
            updates.push((width_key, ParameterValue::Number(width)));
        }
        if (height - orig_height).abs() > f64::EPSILON {
            updates.push((height_key, ParameterValue::Number(height)));
        }
        if (adj_center_x - center_x).abs() > f64::EPSILON {
            updates.push((center_x_key, ParameterValue::Number(adj_center_x)));
        }
        if (adj_center_z - center_z).abs() > f64::EPSILON {
            updates.push((center_z_key, ParameterValue::Number(adj_center_z)));
        }

        let min_z = (adj_center_z - half_height).max(0.0);
        let max_z = adj_center_z + half_height;
        openings.push(OpeningRect {
            min_x: adj_center_x - half_width,
            max_x: adj_center_x + half_width,
            min_z,
            max_z,
            cut_bottom: min_z <= 1.0e-6,
        });
    }

    for (key, value) in updates {
        element.insert_parameter(key, value);
    }

    Ok(openings)
}

fn ensure_openings_do_not_overlap(openings: &[OpeningRect]) -> Result<()> {
    for (idx, opening) in openings.iter().enumerate() {
        for other in openings.iter().skip(idx + 1) {
            let overlap_x = opening.min_x < other.max_x && other.min_x < opening.max_x;
            let overlap_z = opening.min_z < other.max_z && other.min_z < opening.max_z;
            if overlap_x && overlap_z {
                anyhow::bail!("openings overlap");
            }
        }
    }
    Ok(())
}

fn build_wall_with_openings(
    start: Point3,
    length: f64,
    thickness: f64,
    wall_height: f64,
    angle: f64,
    openings: &[OpeningRect],
) -> Result<Solid> {
    let mut wires = Vec::with_capacity(1 + openings.len());
    let mut bottom_cuts = Vec::new();
    let mut holes = Vec::new();
    for opening in openings {
        if opening.cut_bottom {
            bottom_cuts.push(*opening);
        } else {
            holes.push(opening);
        }
    }

    if bottom_cuts.is_empty() {
        wires.push(rectangle_wire(0.0, 0.0, length, wall_height, false));
    } else {
        wires.push(outline_with_bottom_cuts(length, wall_height, &bottom_cuts));
    }

    for opening in holes {
        wires.push(rectangle_wire(
            opening.min_x,
            opening.min_z,
            opening.max_x,
            opening.max_z,
            true,
        ));
    }

    let face = builder::try_attach_plane(wires).context("failed to build wall face")?;
    let solid = builder::tsweep(&face, Vector3::unit_y() * thickness);
    let solid = builder::translated(&solid, Vector3::new(0.0, -thickness * 0.5, 0.0));
    let solid = builder::rotated(
        &solid,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::unit_z(),
        Rad(angle),
    );
    Ok(builder::translated(
        &solid,
        Vector3::new(start.x, start.y, start.z),
    ))
}

fn outline_with_bottom_cuts(length: f64, wall_height: f64, cuts: &[OpeningRect]) -> Wire {
    let mut cuts = cuts.to_vec();
    cuts.sort_by(|a, b| {
        b.max_x
            .partial_cmp(&a.max_x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut points = Vec::new();
    points.push((0.0, wall_height));
    points.push((length, wall_height));
    points.push((length, 0.0));

    let mut cursor_x = length;
    for cut in cuts {
        if cut.max_x < cursor_x - 1.0e-6 {
            points.push((cut.max_x, 0.0));
        }
        points.push((cut.max_x, cut.max_z));
        points.push((cut.min_x, cut.max_z));
        points.push((cut.min_x, 0.0));
        cursor_x = cut.min_x;
    }

    if cursor_x > 1.0e-6 {
        points.push((0.0, 0.0));
    }

    polygon_wire(&points)
}

fn rectangle_wire(
    min_x: f64,
    min_z: f64,
    max_x: f64,
    max_z: f64,
    reverse: bool,
) -> Wire {
    let points = if reverse {
        [
            (min_x, min_z),
            (max_x, min_z),
            (max_x, max_z),
            (min_x, max_z),
        ]
    } else {
        [
            (min_x, min_z),
            (min_x, max_z),
            (max_x, max_z),
            (max_x, min_z),
        ]
    };

    let vertices = points.map(|(x, z)| builder::vertex(Point3::new(x, 0.0, z)));
    let edges = vec![
        builder::line(&vertices[0], &vertices[1]),
        builder::line(&vertices[1], &vertices[2]),
        builder::line(&vertices[2], &vertices[3]),
        builder::line(&vertices[3], &vertices[0]),
    ];
    edges.into()
}

fn polygon_wire(points: &[(f64, f64)]) -> Wire {
    let vertices: Vec<_> = points
        .iter()
        .map(|(x, z)| builder::vertex(Point3::new(*x, 0.0, *z)))
        .collect();
    let mut edges = Vec::with_capacity(points.len());
    for idx in 0..points.len() {
        let next = (idx + 1) % points.len();
        edges.push(builder::line(&vertices[idx], &vertices[next]));
    }
    edges.into()
}

fn read_number(element: &BimElement, key: &str) -> Result<f64> {
    match element.parameters.get(key) {
        Some(ParameterValue::Number(value)) => Ok(*value),
        _ => anyhow::bail!("missing or invalid wall parameter: {key}"),
    }
}

fn world_to_wall_local(point: Point3, start: Point3, angle: f64) -> Point3 {
    let dx = point.x - start.x;
    let dy = point.y - start.y;
    let cos = angle.cos();
    let sin = angle.sin();
    Point3::new(
        dx * cos + dy * sin,
        -dx * sin + dy * cos,
        point.z - start.z,
    )
}
