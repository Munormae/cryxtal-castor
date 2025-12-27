pub struct RebarParams {
    pub diameter: f64,
    pub name: String,
}

impl Default for RebarParams {
    fn default() -> Self {
        Self {
            diameter: 16.0,
            name: String::new(),
        }
    }
}
