use anyhow::{Context, Result};
use cryxtal_topology::Solid;
use std::path::Path;
use truck_stepio::out;

pub fn export_step(solid: &Solid, path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }

    let compressed = solid.compress();
    let header = out::StepHeaderDescriptor {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model.step")
            .to_string(),
        organization_system: "cryxtal-castor".to_string(),
        ..Default::default()
    };

    let step_string =
        out::CompleteStepDisplay::new(out::StepModel::from(&compressed), header).to_string();

    std::fs::write(path, step_string)
        .with_context(|| format!("write STEP file {}", path.display()))?;
    Ok(())
}

pub fn import_step(_path: impl AsRef<Path>) -> Result<Solid> {
    Err(cryxtal_base::Error::NotImplemented("STEP import is not implemented").into())
}
