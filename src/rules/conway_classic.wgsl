struct SimParams {
    width: u32,
    height: u32,
    lucky_chance: f32,
    seed: u32,
    enable_lucky_rule: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

struct GameRules {
    survival_min: u32,
    survival_max: u32,
    birth_count: u32,
    _padding: u32, // Ensure 16-byte alignment
}

@group(0) @binding(0) var<uniform> sim_params: SimParams;
@group(0) @binding(1) var<storage, read> cell_state_in: array<f32>;
@group(0) @binding(2) var<storage, read_write> cell_state_out: array<f32>;
@group(0) @binding(3) var<uniform> game_rules: GameRules;

fn cell_index(x: u32, y: u32) -> u32 {
    return (y % sim_params.height) * sim_params.width + (x % sim_params.width);
}

// Simple hash function for random values
fn hash(value: u32) -> f32 {
    let state = value * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    let result = (word >> 22u) ^ word;
    return f32(result) / 4294967295.0;
}

fn count_neighbors(x: u32, y: u32) -> u32 {
    var count: u32 = 0u;
    let width = sim_params.width;
    let height = sim_params.height;
    
    // Check all 8 neighbors with wrapping at boundaries
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            // Skip the cell itself
            if (dx == 0 && dy == 0) {
                continue;
            }
            
            // Calculate wrapped coordinates
            var nx: u32 = u32(i32(x) + dx);
            var ny: u32 = u32(i32(y) + dy);
            
            // Wrap around grid boundaries
            if (i32(nx) < 0) { nx = width - 1u; } 
            else if (nx >= width) { nx = 0u; }
            
            if (i32(ny) < 0) { ny = height - 1u; } 
            else if (ny >= height) { ny = 0u; }
            
            let idx = cell_index(nx, ny);
            // Only count cells with value > 0.5 (living cells of any color)
            if (cell_state_in[idx] > 0.5) {
                count = count + 1u;
            }
        }
    }
    
    return count;
}

// Function to determine if a cell is a specific color
fn is_color(value: f32, base_color: f32) -> bool {
    // Check if the value is within a small range of the expected color value
    // This helps with floating point precision issues
    return abs(value - base_color) < 0.1;
}

// Function to get the dominant color of living neighbors
fn get_neighbor_colors(x: u32, y: u32) -> f32 {
    var color_counts: array<u32, 6> = array<u32, 6>(0u, 0u, 0u, 0u, 0u, 0u); // Counts for each color
    var alive_count: u32 = 0u;
    let width = sim_params.width;
    let height = sim_params.height;
    
    // Check all 8 neighbors with wrapping at boundaries
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            // Skip the cell itself
            if (dx == 0 && dy == 0) {
                continue;
            }
            
            // Calculate wrapped coordinates
            var nx: u32 = u32(i32(x) + dx);
            var ny: u32 = u32(i32(y) + dy);
            
            // Wrap around grid boundaries
            if (i32(nx) < 0) { nx = width - 1u; } 
            else if (nx >= width) { nx = 0u; }
            
            if (i32(ny) < 0) { ny = height - 1u; } 
            else if (ny >= height) { ny = 0u; }
            
            let idx = cell_index(nx, ny);
            let cell_value = cell_state_in[idx];
            
            // Only consider living cells
            if (cell_value > 0.5) {
                alive_count += 1u;
                
                // Count each color
                if (is_color(cell_value, 1.0)) { // White
                    color_counts[0] += 1u;
                } else if (is_color(cell_value, 2.0) || is_color(cell_value, 3.0)) { // Red
                    color_counts[1] += 1u;
                } else if (is_color(cell_value, 4.0)) { // Green
                    color_counts[2] += 1u;
                } else if (is_color(cell_value, 5.0)) { // Blue
                    color_counts[3] += 1u;
                } else if (is_color(cell_value, 6.0)) { // Yellow
                    color_counts[4] += 1u;
                } else if (is_color(cell_value, 7.0)) { // Purple
                    color_counts[5] += 1u;
                }
            }
        }
    }
    
    // Find the dominant color
    if (alive_count > 0u) {
        var max_count: u32 = 0u;
        var dominant_color_idx: u32 = 0u;
        
        for (var i: u32 = 0u; i < 6u; i = i + 1u) {
            if (color_counts[i] > max_count) {
                max_count = color_counts[i];
                dominant_color_idx = i;
            }
        }
        
        // Return the value for the dominant color
        if (dominant_color_idx == 0u) {
            return 1.0; // White
        } else if (dominant_color_idx == 1u) {
            return 3.0; // Red
        } else if (dominant_color_idx == 2u) {
            return 4.0; // Green
        } else if (dominant_color_idx == 3u) {
            return 5.0; // Blue
        } else if (dominant_color_idx == 4u) {
            return 6.0; // Yellow
        } else if (dominant_color_idx == 5u) {
            return 7.0; // Purple
        }
        
        return 1.0; // Default to white if something goes wrong
    } else {
        return 1.0; // Default to white if somehow there are no living neighbors
    }
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Get current cell position
    let x = global_id.x;
    let y = global_id.y;
    
    // Bounds check
    if (x >= sim_params.width || y >= sim_params.height) {
        return;
    }
    
    let idx = cell_index(x, y);
    let cell = cell_state_in[idx];
    let neighbors = count_neighbors(x, y);
    
    // Generate a value based on position and frame counter (now seed) for deterministic randomness
    let random_seed = x + y * sim_params.width + sim_params.seed;
    let random_value = hash(random_seed);
    
    // Find out if the cell is alive (either normal or red)
    let is_alive = cell > 0.5;
    
    // Apply Game of Life rules
    if (is_alive) {
        // Survival rules
        if (neighbors >= game_rules.survival_min && neighbors <= game_rules.survival_max) {
            cell_state_out[idx] = cell; // Keep the same color
        } else {
            // Underpopulation or Overpopulation - Cell would normally die.
            // Check if the lucky rule is enabled AND the random chance passes.
            if (sim_params.enable_lucky_rule == 1u && random_value < sim_params.lucky_chance) {
                // If the cell is already a specific color, keep that color but make it "lucky"
                // Note: We're using 3.0 for lucky cells instead of 2.0 to differentiate from red cells
                if (is_color(cell, 1.0)) { // White
                    cell_state_out[idx] = 3.0; // Lucky cell with a red tint
                } else if (is_color(cell, 3.0) || is_color(cell, 2.0)) { // Already red or lucky
                    cell_state_out[idx] = 3.0; // Keep as lucky
                } else {
                    // Keep the original color for other colors when they become lucky
                    cell_state_out[idx] = cell;
                }
            } else {
                cell_state_out[idx] = 0.0; // Cell dies normally
            }
        }
    } else { // Cell is dead
        // Birth rules
        if (neighbors == game_rules.birth_count) {
            // Use dominant color of neighbors instead of average
            cell_state_out[idx] = get_neighbor_colors(x, y);
        } else {
            cell_state_out[idx] = 0.0; // Cell stays dead
        }
    }
} 