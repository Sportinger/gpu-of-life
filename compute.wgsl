struct SimParams {
    width: u32,
    height: u32,
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
            if (cell_state_in[idx] > 0.5) {
                count = count + 1u;
            }
        }
    }
    
    return count;
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
    
    // Apply Game of Life rules
    if (cell > 0.5) { // Cell is alive
        // Survival rules
        if (neighbors >= game_rules.survival_min && neighbors <= game_rules.survival_max) {
            cell_state_out[idx] = 1.0;
        } else {
            cell_state_out[idx] = 0.0; // Cell dies
        }
    } else { // Cell is dead
        // Birth rules
        if (neighbors == game_rules.birth_count) {
            cell_state_out[idx] = 1.0; // Cell becomes alive
        } else {
            cell_state_out[idx] = 0.0; // Cell stays dead
        }
    }
} 