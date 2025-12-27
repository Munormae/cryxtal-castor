use anyhow::Result;
use cryxtal_io::{DEFAULT_TESSELLATION_TOLERANCE, export_step, triangulate_solid};
use cryxtal_topology::SolidBuilder;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(file_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let stamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(_) => 0,
    };
    path.push(format!("cryxtal_{stamp}_{file_name}"));
    path
}

#[test]
fn export_step_creates_file() -> Result<()> {
    let solid = SolidBuilder::box_solid(100.0, 200.0, 300.0)?;
    let path = temp_path("box.step");

    export_step(&solid, &path)?;

    let metadata = fs::metadata(&path)?;
    assert!(metadata.len() > 0);

    let _ = fs::remove_file(&path);
    Ok(())
}

#[test]
fn triangulation_produces_mesh() -> Result<()> {
    let solid = SolidBuilder::box_solid(100.0, 200.0, 300.0)?;
    let mesh = triangulate_solid(&solid, DEFAULT_TESSELLATION_TOLERANCE);
    assert!(!mesh.positions().is_empty());
    assert!(mesh.faces().len() > 0);
    Ok(())
}
