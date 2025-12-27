use anyhow::Result;

pub fn export_ifc_stub(_path: impl AsRef<std::path::Path>) -> Result<()> {
    Err(cryxtal_base::Error::NotImplemented("IFC export is not implemented").into())
}
