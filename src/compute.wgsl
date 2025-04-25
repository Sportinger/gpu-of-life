struct SimParams {
    width: u32,
    height: u32,
    seed: u32,
    _pad: u32,
};

let idx = cell_index(x, y);
let cell = cell_state_in[idx];
let neighbors = count_neighbors(x, y);

// Apply Game of Life rules
let is_alive: bool = cell > 0.5;
var next_state: f32 = cell;

if (is_alive) {
    // Survival check
    if (neighbors < game_rules.survival_min || neighbors > game_rules.survival_max) {
        // Cell would die
        next_state = 0.0;
    }
} else {
    // Birth check
    if (neighbors == game_rules.birth_count) {
        next_state = 1.0;
    } else {
        next_state = 0.0;
    }
}

// If a live cell dies this step (was alive, now 0) apply 10% chance to become red (2.0)
if (is_alive && next_state == 0.0) {
    // simple rng based on coords and seed
    let rnd = fract(sin(dot(vec3<f32>(f32(x), f32(y), f32(sim_params.seed)), vec3<f32>(12.9898,78.233,45.164))) * 43758.5453);
    if (rnd < 0.1) {
        next_state = 2.0; // red cell survives
    }
}

cell_state_out[idx] = next_state;

fn is_alive(cell_value: vec4<f32>) -> bool {
    return cell_value.x > 0.5;
}

fn get_cell_color(cell_value: vec4<f32>) -> f32 {
    // Preserve the cell color value when a cell is alive
    if (cell_value.x > 0.5) {
        return cell_value.x;
    }
    return 0.0; // Dead cells have no color
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // ... existing code ...
    
    // Apply Conway's Game of Life rules
    let current_cell = get_cell(global_id.xy);
    let cell_alive = is_alive(current_cell);
    let cell_color = get_cell_color(current_cell);
    
    var next_state = 0.0;
    
    if (cell_alive) {
        // Any live cell with fewer than two live neighbors dies (underpopulation)
        // Any live cell with more than three live neighbors dies (overpopulation)
        if (live_neighbors == 2 || live_neighbors == 3) {
            next_state = cell_color; // Keep the same color value
        }
    } else {
        // Any dead cell with exactly three live neighbors becomes a live cell (reproduction)
        if (live_neighbors == 3) {
            // For new cells, we'll use a mixed color of the neighbors
            next_state = calculate_new_cell_color(global_id.xy);
        }
    }
    
    // Write the new state to the output texture
    textureStore(output_texture, global_id.xy, vec4<f32>(next_state, 0.0, 0.0, 1.0));
}

fn calculate_new_cell_color(position: vec2<u32>) -> f32 {
    // For born cells, we'll use a default white color (1.0)
    // A more complex approach could average the colors of live neighbors
    return 1.0;
} 