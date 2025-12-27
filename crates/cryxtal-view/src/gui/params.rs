pub struct WallParams {
    pub thickness: f64,
    pub height: f64,
    pub name: String,
}

impl Default for WallParams {
    fn default() -> Self {
        Self {
            thickness: 200.0,
            height: 3000.0,
            name: String::new(),
        }
    }
}
