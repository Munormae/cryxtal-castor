use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Guid(Uuid);

impl Guid {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for Guid {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LengthUnit {
    Millimeter,
    Meter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AngleUnit {
    Radian,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Units {
    pub length: LengthUnit,
    pub angle: AngleUnit,
}

impl Default for Units {
    fn default() -> Self {
        Self {
            length: LengthUnit::Millimeter,
            angle: AngleUnit::Radian,
        }
    }
}

impl Units {
    pub const fn metric_mm() -> Self {
        Self {
            length: LengthUnit::Millimeter,
            angle: AngleUnit::Radian,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tolerance {
    pub linear: f64,
    pub angular: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            linear: 1.0e-6,
            angular: 1.0e-6,
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
