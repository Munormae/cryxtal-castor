use cryxtal_base::{Error, Result};
use cryxtal_topology::Solid;

pub struct ViewerStub;

impl ViewerStub {
    pub fn open(_solid: &Solid) -> Result<()> {
        Err(Error::NotImplemented("viewer is not implemented"))
    }
}
