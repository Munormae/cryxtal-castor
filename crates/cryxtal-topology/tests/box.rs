use cryxtal_topology::{Result, SolidBuilder};

#[test]
fn box_solid_exists() -> Result<()> {
    let solid = SolidBuilder::box_solid(100.0, 200.0, 300.0)?;
    assert!(solid.face_iter().count() > 0);
    Ok(())
}
