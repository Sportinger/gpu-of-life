struct SimParams {
    width: u32,
    height: u32,
};

@group(0) @binding(0) var<uniform> params: SimParams;
@group(0) @binding(1) var<storage, read> input_grid: array<f32>; // Input buffer
@group(0) @binding(2) var<storage, read_write> output_grid: array<f32>; // Output buffer

// Function to get cell state from buffer, handling boundary conditions (wrap around)
fn cell_state(x: i32, y: i32) -> f32 {
    let width = i32(params.width);
    let height = i32(params.height);
    // Wrap coordinates
    let ix = (x + width) % width;
    let iy = (y + height) % height;
    let index = u32(iy * width + ix);
    // Check bounds just in case, though modulo should handle it
    if (index >= arrayLength(&input_grid)) {
        return 0.0; // Or handle error appropriately
    }
    return input_grid[index];
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = i32(global_id.x);
    let y = i32(global_id.y);

    if (global_id.x >= params.width || global_id.y >= params.height) {
        return;
    }

    // --- Restore Original GoL Logic --- 
    var live_neighbors = 0u;
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            if (dx == 0 && dy == 0) {
                continue;
            }
            // Use cell_state which handles wrapping
            if (cell_state(x + dx, y + dy) > 0.5) { 
                live_neighbors = live_neighbors + 1u;
            }
        }
    }
    let current_state = cell_state(x, y);
    var next_state = 0.0;
    if (current_state > 0.5) { // Alive
        if (live_neighbors == 2u || live_neighbors == 3u) {
            next_state = 1.0; // Survives
        }
    } else { // Dead
        if (live_neighbors == 3u) {
            next_state = 1.0; // Becomes alive
        }
    }
    // --- End Original GoL Logic ---

    let index = global_id.y * params.width + global_id.x;
    output_grid[index] = next_state;
} 