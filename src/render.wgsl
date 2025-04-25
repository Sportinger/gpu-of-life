@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let cell_value = get_cell_color(in.texCoord);
    
    // Read the cell's value from the current grid buffer
    let cell_state = cell_value.x;
    
    // If the cell is dead, draw it as black
    if (cell_state <= 0.5) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0); // Black for dead cells
    } 
    // Check cell color values
    else if (cell_state > 0.5 && cell_state < 2.0) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0); // White for normal living cells
    }
    else if (cell_state > 2.0 && cell_state < 4.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0); // Red cells
    }
    else if (cell_state > 3.0 && cell_state < 5.0) {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0); // Green cells
    }
    else if (cell_state > 4.0 && cell_state < 6.0) {
        return vec4<f32>(0.0, 0.3, 1.0, 1.0); // Blue cells
    }
    else if (cell_state > 5.0 && cell_state < 7.0) {
        return vec4<f32>(1.0, 1.0, 0.0, 1.0); // Yellow cells
    }
    else if (cell_state > 6.0 && cell_state < 8.0) {
        return vec4<f32>(0.8, 0.2, 1.0, 1.0); // Purple cells
    }
    else {
        // Any other state (shouldn't happen with normal rules)
        return vec4<f32>(0.5, 0.5, 0.5, 1.0);
    }
} 