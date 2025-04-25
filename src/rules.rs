/// Rules module for Conway's Game of Life simulation
///
/// This module contains rule definitions, cell state representations, and preset patterns
/// for the Game of Life simulation.

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
        // Classic Conway's Game of Life rules
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

    /// Preset for Conway's classic Game of Life (B3/S23)
    pub fn conway() -> Self {
        Self::default()
    }

    /// HighLife variant (B36/S23) - has a self-replicating pattern
    pub fn high_life() -> Self {
        Self {
            survival_min: 2,
            survival_max: 3,
            birth_count: 6, // Birth on 3 or 6 neighbors
        }
    }

    /// Day & Night variant (B3678/S34678)
    pub fn day_and_night() -> Self {
        Self {
            survival_min: 3,
            survival_max: 8,
            birth_count: 3,
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

/// Creates a helper function to get the index in a 1D array for a 2D grid position
pub fn get_index(x: u32, y: u32, width: u32) -> usize {
    (y * width + x) as usize
}

/// Given a grid position, count the number of live neighbors using wrapping boundaries
pub fn count_neighbors(grid: &[f32], x: u32, y: u32, width: u32, height: u32) -> u32 {
    let mut count = 0;
    
    for dy in 0..3 {
        for dx in 0..3 {
            // Skip the cell itself
            if dx == 1 && dy == 1 {
                continue;
            }
            
            // Calculate neighbor coordinates with wrapping
            let nx = (x + width + dx - 1) % width;
            let ny = (y + height + dy - 1) % height;
            
            let idx = get_index(nx, ny, width);
            if idx < grid.len() && grid[idx] > 0.5 {
                count += 1;
            }
        }
    }
    
    count
}

/// Apply Game of Life rules to grid for one generation
pub fn apply_rules(input: &[f32], output: &mut [f32], width: u32, height: u32, rules: &GameRules) {
    let size = (width * height) as usize;
    assert!(input.len() >= size);
    assert!(output.len() >= size);
    
    for y in 0..height {
        for x in 0..width {
            let idx = get_index(x, y, width);
            let cell = input[idx];
            let neighbors = count_neighbors(input, x, y, width, height);
            
            let is_alive = cell > 0.5;
            
            output[idx] = if is_alive {
                // Apply survival rules
                if neighbors >= rules.survival_min && neighbors <= rules.survival_max {
                    1.0
                } else {
                    0.0
                }
            } else {
                // Apply birth rules
                if neighbors == rules.birth_count {
                    1.0
                } else {
                    0.0
                }
            };
        }
    }
} 