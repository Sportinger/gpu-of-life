use super::GameRules;

impl GameRules {
    /// Preset for Conway's classic Game of Life (B3/S23)
    pub fn conway() -> Self {
        Self::default() // Assuming Default is defined in the parent module
    }

    /// HighLife variant (B36/S23) - has a self-replicating pattern
    pub fn high_life() -> Self {
        Self {
            survival_min: 2,
            survival_max: 3,
            birth_count: 6, // Birth on 3 or 6 neighbors
            // Note: Original HighLife birth rule is B36. We need to ensure the shader
            // handles multiple birth counts if we want true HighLife via parameters.
            // Currently, the shader only supports one birth_count.
        }
    }

    /// Day & Night variant (B3678/S34678)
    pub fn day_and_night() -> Self {
        Self {
            survival_min: 3,
            survival_max: 8,
            birth_count: 3,
            // Note: Similar to HighLife, Day & Night has more complex rules (B3678/S34678)
            // than can be represented solely by the current GameRules struct parameters.
            // A custom shader would be needed for this rule set.
        }
    }
} 