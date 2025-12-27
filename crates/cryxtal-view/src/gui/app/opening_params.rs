pub struct WallOpeningParams {
    pub width: f64,
    pub height: f64,
}

impl Default for WallOpeningParams {
    fn default() -> Self {
        Self {
            width: 900.0,
            height: 2100.0,
        }
    }
}
