struct SimParams {
    width: u32,
    height: u32,
};

struct RenderParams {
    zoom: f32,
    view_offset: vec2<f32>, // Matches the [f32; 2] in Rust
    // padding not explicitly needed in WGSL struct
};

@group(0) @binding(0) var<uniform> sim_params: SimParams;
@group(0) @binding(1) var<storage, read> grid_state: array<f32>; // Read the current state buffer
@group(0) @binding(2) var<uniform> render_params: RenderParams; // NEW BINDING

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // No need to pass anything else
};

// Vertex shader: Output a full-screen triangle
// We define the triangle vertices directly in clip space
@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    // Define vertices for a full-screen triangle (clip space)
    // x = -1, y = -1  (Bottom-Left)
    // x =  3, y = -1  (Far Right-Bottom)
    // x = -1, y =  3  (Far Top-Left)
    // This covers the entire screen because coordinates outside [-1, 1] are clipped,
    // but the triangle formed includes the [-1, 1] square.
    let x = f32(in_vertex_index / 2u) * 4.0 - 1.0;
    let y = f32(in_vertex_index % 2u) * 4.0 - 1.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

// Fragment shader: Read grid state and output color
@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    // Apply zoom and offset:
    // 1. Add view offset (which is in grid coordinates) to the pixel coordinate.
    // 2. Scale the combined coordinate by the inverse zoom factor.
    let coord_with_offset = frag_coord.xy + render_params.view_offset;
    let scaled_coord = coord_with_offset / render_params.zoom;

    // Use scaled coordinates for grid lookup
    let grid_x = i32(floor(scaled_coord.x));
    let grid_y = i32(floor(scaled_coord.y));

    let width = i32(sim_params.width);
    let height = i32(sim_params.height);

    // Check if the *logical* grid coordinate is within bounds
    if (grid_x < 0 || grid_x >= width || grid_y < 0 || grid_y >= height) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0); // Black background
    }

    // Calculate 1D index 
    let index = u32(grid_y * width + grid_x);

    // Read state from buffer
    var cell_value = 0.0; 
    if (index < arrayLength(&grid_state)) {
        cell_value = grid_state[index];
    } else {
        return vec4<f32>(0.0, 0.0, 0.5, 1.0); // Dark Blue error
    }

    let color = vec3<f32>(cell_value);
    return vec4<f32>(color, 1.0);
} 