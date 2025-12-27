use anyhow::Result;
use cryxtal_io::export_step;
use cryxtal_topology::SolidBuilder;

fn main() -> Result<()> {
    let solid = SolidBuilder::box_solid(100.0, 200.0, 300.0)?;
    export_step(&solid, "out/box.step")?;
    Ok(())
}
