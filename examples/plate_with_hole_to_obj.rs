use anyhow::Result;
use cryxtal_io::{DEFAULT_TESSELLATION_TOLERANCE, export_obj};
use cryxtal_shapeops::{DEFAULT_SHAPEOPS_TOLERANCE, plate_with_hole};

fn main() -> Result<()> {
    let solid = plate_with_hole(1000.0, 200.0, 200.0, 100.0, DEFAULT_SHAPEOPS_TOLERANCE)?;
    export_obj(&solid, "out/plate.obj", DEFAULT_TESSELLATION_TOLERANCE)?;
    Ok(())
}
