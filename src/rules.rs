/// Rules module for Conway's Game of Life simulation
///
/// This module contains rule definitions, cell state representations, and preset patterns
/// for the Game of Life simulation.

// Declare sub-modules
pub mod presets;
pub mod patterns;

// Re-export contents for easier access

/// Game of Life standard rules:
/// 1. Any live cell with fewer than two live neighbors dies (underpopulation)
/// 2. Any live cell with two or three live neighbors lives (survival)
/// 3. Any live cell with more than three live neighbors dies (overpopulation)
/// 4. Any dead cell with exactly three live neighbors becomes alive (reproduction)
#[derive(Debug, Clone, Copy)]
pub struct GameRules {
    /// Minimum neighbors for a live cell to survive
    pub survival_min: u32,
    /// Maximum neighbors for a live cell to survive
    pub survival_max: u32,
    /// Number of neighbors for a dead cell to become alive
    pub birth_count: u32,
}

impl Default for GameRules {
    fn default() -> Self {
        // Classic Conway's Game of Life rules (B3/S23)
        Self {
            survival_min: 2,
            survival_max: 3,
            birth_count: 3,
        }
    }
}

impl GameRules {
    /// Create a new rule set with custom parameters
    pub fn new(survival_min: u32, survival_max: u32, birth_count: u32) -> Self {
        Self {
            survival_min,
            survival_max,
            birth_count,
        }
    }
}

/// Predefined patterns for initializing the grid
pub enum Pattern {
    /// A small oscillator
    Blinker,
    /// A small oscillator
    Toad,
    /// A small stationary pattern
    Block,
    /// A diagonal spaceship
    Glider,
    /// A horizontal spaceship
    LightweightSpaceship,
    /// A pattern that grows indefinitely
    GosperGliderGun,
}

/// Utility to place a pattern on a grid
pub fn place_pattern_on_grid(grid: &mut [f32], width: u32, height: u32, pattern: &Pattern, x: u32, y: u32) {
    let cells = pattern.cells(x, y);
    
    for (cell_x, cell_y) in cells {
        if cell_x < width && cell_y < height {
            let idx = (cell_y * width + cell_x) as usize;
            if idx < grid.len() {
                grid[idx] = 1.0;
            }
        }
    }
}

/// Initialize a grid with a specific pattern at the center
pub fn initialize_grid_with_pattern(width: u32, height: u32, pattern: &Pattern) -> Vec<f32> {
    let size = (width * height) as usize;
    let mut grid = vec![0.0f32; size];
    
    // Place the pattern at the center of the grid
    let center_x = width / 2;
    let center_y = height / 2;
    
    place_pattern_on_grid(&mut grid, width, height, pattern, center_x, center_y);
    
    grid
}