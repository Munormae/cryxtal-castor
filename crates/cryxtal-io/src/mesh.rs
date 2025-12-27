use anyhow::{Context, Result, bail};
use cryxtal_topology::Solid;
use std::fs::File;
use std::path::Path;
use truck_meshalgo::prelude::*;
use truck_polymesh::{PolygonMesh, obj};

pub const DEFAULT_TESSELLATION_TOLERANCE: f64 = 0.5;

pub fn triangulate_solid(solid: &Solid, tol: f64) -> PolygonMesh {
    let mut mesh = solid.triangulation(tol).to_polygon();
    mesh.add_naive_normals(true);
    mesh.put_together_same_attrs(truck_base::tolerance::TOLERANCE);
    mesh.remove_unused_attrs();
    mesh
}

pub fn export_obj(solid: &Solid, path: impl AsRef<Path>, tol: f64) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }

    let mesh = triangulate_solid(solid, tol);
    if mesh.positions().is_empty() {
        bail!("triangulation produced empty mesh");
    }

    let file = File::create(path).with_context(|| format!("create OBJ file {}", path.display()))?;
    obj::write(&mesh, file).with_context(|| format!("write OBJ file {}", path.display()))?;
    Ok(())
}
