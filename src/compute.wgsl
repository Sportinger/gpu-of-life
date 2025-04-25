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
} 