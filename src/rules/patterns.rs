use super::Pattern;

impl Pattern {
    /// Get the cells for a pattern centered at position (x, y)
    pub fn cells(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        match self {
            Pattern::Blinker => vec![
                (x, y-1), (x, y), (x, y+1)
            ],
            Pattern::Toad => vec![
                (x-1, y), (x, y), (x+1, y),
                (x-2, y+1), (x-1, y+1), (x, y+1)
            ],
            Pattern::Block => vec![
                (x, y), (x+1, y),
                (x, y+1), (x+1, y+1)
            ],
            Pattern::Glider => vec![
                (x, y+1),
                (x+1, y+2),
                (x+2, y), (x+2, y+1), (x+2, y+2)
            ],
            Pattern::LightweightSpaceship => vec![
                (x, y+1), (x, y+3),
                (x+1, y),
                (x+2, y),
                (x+3, y), (x+3, y+3),
                (x+4, y), (x+4, y+1), (x+4, y+2)
            ],
            Pattern::GosperGliderGun => vec![
                // Left block
                (x+1, y+5), (x+1, y+6),
                (x+2, y+5), (x+2, y+6),

                // Left ship
                (x+11, y+5), (x+11, y+6), (x+11, y+7),
                (x+12, y+4), (x+12, y+8),
                (x+13, y+3), (x+13, y+9),
                (x+14, y+3), (x+14, y+9),
                (x+15, y+6),
                (x+16, y+4), (x+16, y+8),
                (x+17, y+5), (x+17, y+6), (x+17, y+7),
                (x+18, y+6),

                // Right ship
                (x+21, y+3), (x+21, y+4), (x+21, y+5),
                (x+22, y+3), (x+22, y+4), (x+22, y+5),
                (x+23, y+2), (x+23, y+6),
                (x+25, y+1), (x+25, y+2), (x+25, y+6), (x+25, y+7),

                // Right block
                (x+35, y+3), (x+35, y+4),
                (x+36, y+3), (x+36, y+4)
            ],
        }
    }
} 