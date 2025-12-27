use cryxtal_base::Guid;
use cryxtal_topology::Solid;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BimCategory {
    Wall,
    Slab,
    Beam,
    Opening,
    Rebar,
    Generic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ParameterValue {
    Integer(i64),
    Number(f64),
    Bool(bool),
    Text(String),
}

pub type ParameterSet = BTreeMap<String, ParameterValue>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BimElement {
    pub guid: Guid,
    pub name: String,
    pub category: BimCategory,
    pub parameters: ParameterSet,
    pub geometry: Solid,
}

impl BimElement {
    pub fn new(
        guid: Guid,
        name: impl Into<String>,
        category: BimCategory,
        parameters: ParameterSet,
        geometry: Solid,
    ) -> Self {
        Self {
            guid,
            name: name.into(),
            category,
            parameters,
            geometry,
        }
    }

    pub fn insert_parameter(&mut self, key: impl Into<String>, value: ParameterValue) {
        self.parameters.insert(key.into(), value);
    }

    pub fn geometry(&self) -> &Solid {
        &self.geometry
    }
}
