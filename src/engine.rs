pub struct EngineConfig {
    pub initial_cash: f64,

    /// Round-trip cost per side, in basis points (1 bp = 0.01%).
    pub free_bps: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            initial_cash: 10_000.0,
            free_bps: 10.0,
        }
    }
}
