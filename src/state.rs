use crate::compute::{SimParams, WORKGROUP_SIZE, create_compute_bind_groups, ShaderGameRules};
use crate::render::{RenderParams, MIN_ZOOM, create_render_bind_group_layout, create_render_bind_groups};
use crate::rules::{Pattern, place_pattern_on_grid, GameRules};
use wgpu::util::DeviceExt;
use winit::{
    dpi::PhysicalPosition,
    window::Window,
};
use std::sync::Arc;
use std::borrow::Cow; // Needed for ShaderSource

// GUI Imports
use egui_winit::State as EguiWinitState;
use egui_wgpu::Renderer as EguiWgpuRenderer;
use egui::Context as EguiContext;
use std::time::Instant;
use std::time::Duration; // For throttling

// Cursor modes for different tools
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorMode {
    Paint,               // Default - paint cells
    PlaceGlider,         // Place standard gliders
    PlaceLWSS,           // Place lightweight spaceships
    PlacePulsar,         // Place pulsar oscillator
    PlaceGosperGun,      // Place Gosper glider gun
    PlacePentadecathlon, // Place pentadecathlon oscillator
    PlaceSimkinGun,      // Place Simkin glider gun
    ClearArea,           // Clear cells in an area
    RandomFill,          // Fill with random cells
}

// Cell colors for placed cells
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellColor {
    White,  // Default white (1.0)
    Red,    // Red (3.0)
    Green,  // Green (4.0)
    Blue,   // Blue (5.0)
    Yellow, // Yellow (6.0)
    Purple, // Purple (7.0)
}

impl Default for CellColor {
    fn default() -> Self {
        Self::White
    }
}

impl CellColor {
    // Convert the enum to its float representation for the shader
    pub fn to_value(&self) -> f32 {
        match self {
            CellColor::White => 1.0,
            CellColor::Red => 3.0,
            CellColor::Green => 4.0,
            CellColor::Blue => 5.0,
            CellColor::Yellow => 6.0,
            CellColor::Purple => 7.0,
        }
    }
}

impl Default for CursorMode {
    fn default() -> Self {
        Self::Paint
    }
}

// const BRUSH_RADIUS: i32 = 3; // Remove constant, will use state field

pub struct State {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,

    pub grid_width: u32,
    pub grid_height: u32,
    pub grid_buffers: [wgpu::Buffer; 2],
    pub sim_param_buffer: wgpu::Buffer,
    pub rules_buffer: wgpu::Buffer,
    pub current_rules: GameRules,

    // --- Compute related fields ---
    pub compute_shader_source: String, // Store the source code
    pub compute_bind_group_layout: wgpu::BindGroupLayout,
    pub compute_pipeline_layout: wgpu::PipelineLayout, // Store the layout
    pub compute_pipeline: wgpu::ComputePipeline, // The current pipeline
    pub compute_bind_groups: [wgpu::BindGroup; 2],
    // --- End Compute ---

    pub render_pipeline: wgpu::RenderPipeline,
    pub render_bind_group_layout: wgpu::BindGroupLayout,
    pub render_bind_groups: [wgpu::BindGroup; 2],
    pub render_param_buffer: wgpu::Buffer,

    pub frame_num: usize,
    pub zoom: f32,
    pub view_offset: [f32; 2], // Current view offset (in grid coordinates)
    pub is_right_mouse_pressed: bool,
    pub is_left_mouse_pressed: bool,
    pub last_mouse_pos: Option<PhysicalPosition<f64>>,
    pub cursor_pos: Option<PhysicalPosition<f64>>, // For zoom centering

    // Drag handling state
    pub is_dragging: bool,
    pub drag_start_pos: Option<PhysicalPosition<f64>>,
    pub last_action_time: Option<std::time::Instant>,
    pub last_glider_time: Option<std::time::Instant>,
    pub last_paint_time: Option<std::time::Instant>,
    pub last_clear_time: Option<std::time::Instant>,
    pub last_random_time: Option<std::time::Instant>,
    pub last_lwss_time: Option<std::time::Instant>,
    pub last_pulsar_time: Option<std::time::Instant>,
    pub last_gosper_gun_time: Option<std::time::Instant>,
    pub last_pentadecathlon_time: Option<std::time::Instant>,
    pub last_simkin_gun_time: Option<std::time::Instant>,

    // Context menu state
    pub right_click_start_pos: Option<PhysicalPosition<f64>>,
    pub right_drag_started: bool,
    pub show_context_menu: bool,
    pub context_menu_pos: Option<PhysicalPosition<f64>>,
    pub cursor_mode: CursorMode,
    pub show_submenu: bool,
    pub submenu_parent: Option<String>,  // Identifies which option the submenu is for
    pub submenu_pos: Option<PhysicalPosition<f64>>,

    // GUI state
    pub egui_ctx: EguiContext,
    pub egui_winit_state: EguiWinitState,
    pub egui_renderer: EguiWgpuRenderer,
    pub menu_open: bool,
    pub lucky_rule_enabled: bool,
    pub brush_radius: u32,
    pub lucky_chance_percent: u32,
    pub current_cell_color: CellColor, // Current color for placed cells
    // Cell counting state
    pub live_cell_count: Option<u32>,
    pub last_count_update_time: Option<Instant>,
    // Simulation speed control
    pub simulation_speed: u32,           // Steps per second (1-240)
    pub last_update_time: Instant,       // When we last ran a simulation step
    pub accumulated_time: f32,           // Accumulated time for simulation steps
    // FPS tracking
    pub frame_times: Vec<f32>,           // Circular buffer of recent frame times in seconds
    pub frame_time_index: usize,         // Current position in the circular buffer
    pub last_frame_time: Instant,        // Time of the last rendered frame
    pub fps: f32,                        // Current calculated FPS
}

impl State {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let initial_grid_width = size.width.max(1);
        let initial_grid_height = size.height.max(1);

        log::info!("Initializing wgpu...");

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: initial_grid_width,
            height: initial_grid_height,
            present_mode: wgpu::PresentMode::Immediate,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![surface_format.into()],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create Game Rules
        let game_rules = GameRules::default(); // Default Conway's rules
        let shader_rules = ShaderGameRules::from(&game_rules);
        let rules_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Game Rules Buffer"),
            contents: bytemuck::bytes_of(&shader_rules),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create Grid Resources
        let (grid_buffers, sim_param_buffer) =
            Self::create_grid_buffers(&device, initial_grid_width, initial_grid_height);
        queue.write_buffer(&sim_param_buffer, 0, bytemuck::bytes_of(&SimParams {
            width: initial_grid_width,
            height: initial_grid_height,
            lucky_chance: 0.1,
            seed: 0,
            enable_lucky_rule: 0,
            _padding: [0; 3],
        }));
        Self::initialize_grid_buffer(&queue, &grid_buffers[0], initial_grid_width, initial_grid_height);

        // Create Render Resources
        let initial_zoom = MIN_ZOOM;
        let initial_view_offset = [0.0, 0.0];
        let render_param_data = RenderParams {
            zoom: initial_zoom,
            view_offset: initial_view_offset,
            _padding: 0.0,
        };
        let render_param_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Render Parameters"),
            contents: bytemuck::bytes_of(&render_param_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Load initial compute shader source (will be compiled later)
        let initial_compute_shader_source = include_str!("rules/conway_classic.wgsl").to_string();

        // Define a minimal placeholder compute shader for the initial temporary pipeline.
        // It MUST define the same structs as the real shader for layout compatibility.
        let placeholder_compute_shader_source = r#"
struct SimParams {
    width: u32,
    height: u32,
    lucky_chance: f32,
    seed: u32,
    enable_lucky_rule: u32,
    // _padding: array<u32, 3>,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

struct GameRules {
    survival_min: u32,
    survival_max: u32,
    birth_count: u32,
    _padding: u32,
}

@group(0) @binding(0) var<uniform> sim_params: SimParams;
@group(0) @binding(1) var<storage, read> cell_state_in: array<f32>;
@group(0) @binding(2) var<storage, read_write> cell_state_out: array<f32>;
@group(0) @binding(3) var<uniform> game_rules: GameRules;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Minimal main function for placeholder - does nothing useful
    let idx = global_id.x + global_id.y * sim_params.width;
    if (idx < arrayLength(&cell_state_out)) {
        cell_state_out[idx] = 0.0; // Just write 0
    }
}
        "#;

        // Load render shader (doesn't need dynamic loading for now)
        let render_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Render Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../render.wgsl").into()),
        });

        // --- Setup Compute Pipeline ---
        let compute_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Compute Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry { // SimParams
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { // Input Grid
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { // Output Grid
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { // Game Rules
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Compute Pipeline Layout"),
            bind_group_layouts: &[&compute_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Initial pipeline creation will happen via recreate_compute_pipeline

        let compute_bind_groups = create_compute_bind_groups(
            &device, &compute_bind_group_layout, &grid_buffers, &sim_param_buffer, &rules_buffer
        );
        // --- End Compute Pipeline Setup ---


        // Render Pipeline
        let render_bind_group_layout = create_render_bind_group_layout(&device);
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&render_bind_group_layout],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader_module,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader_module,
                entry_point: "fs_main",
                targets: &[Some(config.format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let render_bind_groups = create_render_bind_groups(
            &device, &render_bind_group_layout, &grid_buffers, &sim_param_buffer, &render_param_buffer
        );

        log::info!("Initializing egui...");
        let egui_ctx = EguiContext::default();
        let egui_winit_state = EguiWinitState::new(egui_ctx.clone(), egui_ctx.viewport_id(), &window, None, None);
        let egui_renderer = EguiWgpuRenderer::new(&device, config.format, None, 1);
        log::info!("egui initialized.");

        log::info!("wgpu initialized successfully.");

        // Temporary compute pipeline before the real one is compiled
        // This is slightly awkward but necessary because recreate_compute_pipeline needs `&mut self`
        let temp_compute_pipeline = {
             let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                 label: Some("Temp Initial Compute Shader"),
                 // source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&initial_compute_shader_source)), // Use placeholder instead
                 source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(placeholder_compute_shader_source)),
             });
             device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                 label: Some("Temp Initial Compute Pipeline"),
                 layout: Some(&compute_pipeline_layout),
                 module: &module,
                 entry_point: "main",
             })
        };


        let mut state = Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            grid_width: initial_grid_width,
            grid_height: initial_grid_height,
            grid_buffers,
            sim_param_buffer,
            rules_buffer,
            current_rules: game_rules,

            compute_shader_source: initial_compute_shader_source, // Store source
            compute_bind_group_layout,
            compute_pipeline_layout, // Store layout
            compute_pipeline: temp_compute_pipeline, // Store pipeline (will be replaced)
            compute_bind_groups,

            render_pipeline,
            render_bind_group_layout,
            render_bind_groups,
            render_param_buffer,
            frame_num: 0,
            zoom: initial_zoom,
            view_offset: initial_view_offset,
            is_right_mouse_pressed: false,
            is_left_mouse_pressed: false,
            last_mouse_pos: None,
            cursor_pos: None,
            egui_ctx,
            egui_winit_state,
            egui_renderer,
            menu_open: false,
            lucky_rule_enabled: false,
            brush_radius: 3,
            lucky_chance_percent: 10,
            // Cell counting state
            live_cell_count: None,
            last_count_update_time: None,
            // Initialize simulation speed to 60 steps per second
            simulation_speed: 60,
            last_update_time: Instant::now(),
            accumulated_time: 0.0,
            // FPS tracking
            frame_times: vec![0.0; 60],    // Track last 60 frames (1 second at 60fps)
            frame_time_index: 0,
            last_frame_time: Instant::now(),
            fps: 0.0,
            // Context menu state
            right_click_start_pos: None,
            right_drag_started: false,
            show_context_menu: false,
            context_menu_pos: None,
            cursor_mode: CursorMode::default(),
            show_submenu: false,
            submenu_parent: None,
            submenu_pos: None,
            // Drag handling state
            is_dragging: false,
            drag_start_pos: None,
            last_action_time: None,
            last_glider_time: None,
            last_paint_time: None,
            last_clear_time: None,
            last_random_time: None,
            last_lwss_time: None,
            last_pulsar_time: None,
            last_gosper_gun_time: None,
            last_pentadecathlon_time: None,
            last_simkin_gun_time: None,
            current_cell_color: CellColor::default(),
        };

        // Now compile the *real* initial pipeline
        state.recreate_compute_pipeline_from_source()
             .expect("Failed to compile initial compute shader");

        state
    }

    /// Compiles the WGSL source stored in `self.compute_shader_source` and
    /// replaces `self.compute_pipeline`.
    fn recreate_compute_pipeline_from_source(&mut self) -> Result<(), String> {
        log::info!("Compiling compute shader...");
        let shader_module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Dynamic Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&self.compute_shader_source)),
        });

        // Note: Shader compilation errors are not directly exposed in a user-friendly way by wgpu's create_shader_module.
        // Errors might be reported through logs or device loss if severe.
        // For more robust error handling, WGSL validation libraries (like naga) could be used beforehand.

        self.compute_pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Dynamic Compute Pipeline"),
            layout: Some(&self.compute_pipeline_layout), // Use stored layout
            module: &shader_module,
            entry_point: "main",
        });
        log::info!("Compute shader compiled successfully.");
        Ok(())
    }

    /// Loads new WGSL source code, attempts to compile it, and replaces the
    /// current compute pipeline if successful.
    pub fn load_new_compute_shader(&mut self, new_shader_source: String) -> Result<(), String> {
        self.compute_shader_source = new_shader_source;
        self.recreate_compute_pipeline_from_source() // Attempt recompilation
    }

    // Helper function to create grid buffers (kept internal to State)
    fn create_grid_buffers(device: &wgpu::Device, width: u32, height: u32) -> ([wgpu::Buffer; 2], wgpu::Buffer) {
        let grid_size = (width * height) as u64;
        let buffer_size = grid_size * std::mem::size_of::<f32>() as u64;

        let grid_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Grid Buffer 0"),
                size: buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Grid Buffer 1"),
                size: buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
        ];

        let sim_param_buffer = device.create_buffer(&wgpu::BufferDescriptor {
             label: Some("Simulation Parameters"),
             size: std::mem::size_of::<SimParams>() as u64,
             usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
             mapped_at_creation: false,
        });

        (grid_buffers, sim_param_buffer)
    }

    // Helper function to initialize one grid buffer (kept internal to State)
    fn initialize_grid_buffer(queue: &wgpu::Queue, buffer: &wgpu::Buffer, width: u32, height: u32) {
        let grid_size = (width * height) as usize;
        let mut initial_data = vec![0.0f32; grid_size];

        if width > 10 && height > 10 {
            // Place a glider pattern near the center
            let pattern_pos_x = width / 4;
            let pattern_pos_y = height / 4;

            // Use the Pattern enum from the rules module
            place_pattern_on_grid(&mut initial_data, width, height, &Pattern::Glider, pattern_pos_x, pattern_pos_y);

            // You could add more patterns or different ones based on parameters
            // For example:
            // place_pattern_on_grid(&mut initial_data, width, height, &Pattern::Blinker, width/2, height/2);
            place_pattern_on_grid(&mut initial_data, width, height, &Pattern::GosperGliderGun, width/5, height/2);
        }

        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&initial_data));
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            // Update grid dimensions
            self.grid_width = new_size.width;
            self.grid_height = new_size.height;

            // Recreate buffers with new size
            let (new_grid_buffers, new_sim_param_buffer) =
                Self::create_grid_buffers(&self.device, self.grid_width, self.grid_height);
            self.grid_buffers = new_grid_buffers;
            self.sim_param_buffer = new_sim_param_buffer;

            // Update uniform buffer content
            self.queue.write_buffer(&self.sim_param_buffer, 0, bytemuck::bytes_of(&SimParams {
                width: self.grid_width,
                height: self.grid_height,
                lucky_chance: self.lucky_chance_percent as f32 / 100.0,
                seed: self.frame_num as u32,
                enable_lucky_rule: if self.lucky_rule_enabled { 1 } else { 0 },
                _padding: [0; 3],
            }));

            // Re-initialize buffer 0 (clears state on resize)
            Self::initialize_grid_buffer(&self.queue, &self.grid_buffers[0], self.grid_width, self.grid_height);

            // Recreate bind groups using the functions from the modules
            // Note: The compute pipeline itself does *not* need to be recreated on resize
            self.compute_bind_groups = create_compute_bind_groups(
                &self.device, &self.compute_bind_group_layout, &self.grid_buffers,
                &self.sim_param_buffer, &self.rules_buffer
            );
             self.render_bind_groups = create_render_bind_groups(
                &self.device, &self.render_bind_group_layout, &self.grid_buffers, &self.sim_param_buffer, &self.render_param_buffer
            );

            // Reset frame counter to ensure correct initial buffer read
            self.frame_num = 0;
            // Reset view offset on resize to avoid confusion
            self.view_offset = [0.0, 0.0];
            self.zoom = MIN_ZOOM;
             self.queue.write_buffer(&self.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
                 zoom: self.zoom,
                 view_offset: self.view_offset,
                 _padding: 0.0,
             }));

            log::info!("Resized grid and reconfigured surface to: {}x{}", self.grid_width, self.grid_height);
        } else {
            log::warn!("Ignoring resize to zero dimensions: {}x{}", new_size.width, new_size.height);
        }
    }

    /// Change the Game of Life rules (parameterized approach, retained for compatibility/flexibility)
    pub fn change_rules(&mut self, rules: GameRules) {
        self.current_rules = rules;
        let shader_rules = ShaderGameRules::from(&self.current_rules);
        self.queue.write_buffer(&self.rules_buffer, 0, bytemuck::bytes_of(&shader_rules));
        log::info!("Game rules (uniform buffer) changed to: S{}-{}/B{}",
                   rules.survival_min, rules.survival_max, rules.birth_count);
        // Note: This only changes the uniform buffer. To swap the actual shader logic,
        // call `load_new_compute_shader` with the new WGSL source.
    }

    /// Run simulation step & render the grid state. Returns the surface texture for egui to draw on.
    pub fn update_and_render(&mut self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        // Update FPS calculation
        self.update_fps();
        
        // Update the simulation parameters with the current frame number
        self.queue.write_buffer(&self.sim_param_buffer, 0, bytemuck::bytes_of(&SimParams {
            width: self.grid_width,
            height: self.grid_height,
            lucky_chance: self.lucky_chance_percent as f32 / 100.0,
            seed: self.frame_num as u32,
            enable_lucky_rule: if self.lucky_rule_enabled { 1 } else { 0 },
            _padding: [0; 3],
        }));

        // Calculate how many simulation steps to run this frame
        let current_time = Instant::now();
        let elapsed_time = current_time.duration_since(self.last_update_time);
        self.accumulated_time += elapsed_time.as_secs_f32();
        self.last_update_time = current_time;
        
        // Determine number of steps to simulate
        let time_per_step = 1.0 / self.simulation_speed as f32;
        let mut steps_to_run = 0;
        
        // Count how many steps we need to run
        while self.accumulated_time >= time_per_step {
            self.accumulated_time -= time_per_step;
            steps_to_run += 1;
            
            // Limit maximum steps per frame to prevent freezing on big time jumps
            if steps_to_run >= 100 {
                self.accumulated_time = 0.0; // Reset to avoid huge backlog
                break;
            }
        }
        
        if steps_to_run > 0 {
            // Create a single command encoder for all steps
            let mut compute_encoder = self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { 
                    label: Some("Batched Compute Encoder") 
                });
            
            // Run multiple simulation steps with the same encoder
            for _ in 0..steps_to_run {
                // Track which buffer is input vs output
                let input_idx = self.frame_num % 2;
                let output_idx = (self.frame_num + 1) % 2;
                
                {
                    let mut compute_pass = compute_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Game of Life Compute Pass"),
                        timestamp_writes: None,
                    });
                    compute_pass.set_pipeline(&self.compute_pipeline);
                    compute_pass.set_bind_group(0, &self.compute_bind_groups[input_idx], &[]);
                    
                    let dispatch_x = (self.grid_width + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
                    let dispatch_y = (self.grid_height + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
                    compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
                }
                
                self.frame_num += 1;
            }
            
            // Submit all simulation steps at once
            self.queue.submit(Some(compute_encoder.finish()));
        }

        // --- Get Surface Texture (early exit on error) ---
        let output_frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost) => {
                log::warn!("Surface lost, recreating...");
                self.resize(self.size); // Reconfigure the surface
                // Return the error, the caller (main loop) should handle skipping the frame
                return Err(wgpu::SurfaceError::Lost);
            }
            Err(e) => {
                log::error!("Failed to acquire next swap chain texture: {:?}", e);
                return Err(e);
            }
        };

        // --- Render Pass ---
        let output_view = output_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut render_encoder = self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });
        {
            let mut render_pass = render_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render_pass.set_pipeline(&self.render_pipeline);
            // Use the output of the compute pass (which is frame_num % 2) as input for render pass
            render_pass.set_bind_group(0, &self.render_bind_groups[(self.frame_num + 1) % 2], &[]);
            render_pass.draw(0..3, 0..1); // Draw full-screen triangle
        }
        self.queue.submit(Some(render_encoder.finish()));
        // output_frame.present(); // DON'T present here, egui will do it later

        // Return the frame so egui can render to it
        Ok(output_frame)
    }

    /// Reads the current grid state back from the GPU and updates the live cell count.
    /// WARNING: This is a blocking operation and will stall the GPU pipeline!
    pub fn update_live_cell_count(&mut self) {
        // Buffer containing the latest simulation state (the one about to be rendered)
        let source_buffer_index = (self.frame_num + 1) % 2;
        let source_buffer = &self.grid_buffers[source_buffer_index];

        let buffer_size = (self.grid_width * self.grid_height * std::mem::size_of::<f32>() as u32) as wgpu::BufferAddress;

        // Create a staging buffer (CPU-visible) to copy the data into
        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cell Count Staging Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Create command encoder to copy data
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Cell Count Copy Encoder"),
        });

        // Copy data from GPU grid buffer to CPU staging buffer
        encoder.copy_buffer_to_buffer(
            source_buffer,       // Source GPU buffer
            0,                   // Source offset
            &staging_buffer,     // Destination CPU buffer
            0,                   // Destination offset
            buffer_size,         // Size
        );

        // Submit the copy command to the GPU queue
        self.queue.submit(Some(encoder.finish()));

        // Request mapping of the staging buffer
        let buffer_slice = staging_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel(); // Use a channel for async map result
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });

        // Poll the device Csync!! THIS WILL BLOCK until the GPU finishes the copy and mapping.
        self.device.poll(wgpu::Maintain::Wait);

        // Receive the mapping result
        match receiver.recv() {
            Ok(Ok(())) => {
                // Get the mapped data
                let data = buffer_slice.get_mapped_range();
                let cell_states: &[f32] = bytemuck::cast_slice(&data);

                // Count live cells (value > 0.5)
                let count = cell_states.iter().filter(|&&state| state > 0.5).count();

                // Update state
                self.live_cell_count = Some(count as u32);
                self.last_count_update_time = Some(Instant::now()); // Record update time

                // Drop the mapped view
                drop(data);
                // Unmap the buffer
                staging_buffer.unmap();
            }
            Ok(Err(e)) => {
                log::error!("Failed to map staging buffer for cell count: {:?}", e);
                self.live_cell_count = None; // Indicate error/unknown state
            }
            Err(e) => {
                 log::error!("Failed to receive cell count map result: {:?}", e);
                 self.live_cell_count = None;
            }
        }
    }

    pub fn paint_cell(&mut self, screen_pos: PhysicalPosition<f64>) {
        // Convert screen pos to grid coordinate under current zoom & offset
        let x_world = ((screen_pos.x as f32) + self.view_offset[0]) / self.zoom;
        let y_world = ((screen_pos.y as f32) + self.view_offset[1]) / self.zoom;

        let gx = x_world.floor() as i32;
        let gy = y_world.floor() as i32;
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        // Paint a square brush of size (2*R+1)^2
        let radius = self.brush_radius as i32;
        for by in -radius..=radius {
            for bx in -radius..=radius {
                let cx = gx + bx;
                let cy = gy + by;
                if cx < 0 || cy < 0 || cx >= self.grid_width as i32 || cy >= self.grid_height as i32 {
                    continue;
                }
                let idx = (cy as u32 * self.grid_width + cx as u32) as usize;
                let val: [f32;1] = [self.current_cell_color.to_value()];
                // Write to the *input* buffer for the *next* frame's compute pass
                self.queue.write_buffer(&self.grid_buffers[self.frame_num % 2], idx as u64 * 4, bytemuck::bytes_of(&val));
            }
        }
    }

    /// Update FPS calculation with the current frame time
    pub fn update_fps(&mut self) {
        let now = Instant::now();
        let frame_time = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        
        // Add frame time to the circular buffer
        self.frame_times[self.frame_time_index] = frame_time;
        self.frame_time_index = (self.frame_time_index + 1) % self.frame_times.len();
        
        // Calculate average frame time from all non-zero entries
        let mut total_time = 0.0;
        let mut count = 0;
        for &time in self.frame_times.iter() {
            if time > 0.0 {
                total_time += time;
                count += 1;
            }
        }
        
        if count > 0 {
            let avg_frame_time = total_time / count as f32;
            self.fps = 1.0 / avg_frame_time; // Convert to frames per second
        }
    }

    /// Convert a screen position to grid coordinates
    pub fn screen_to_grid(&self, screen_pos: PhysicalPosition<f64>) -> (i32, i32) {
        let x_world = ((screen_pos.x as f32) + self.view_offset[0]) / self.zoom;
        let y_world = ((screen_pos.y as f32) + self.view_offset[1]) / self.zoom;

        (x_world.floor() as i32, y_world.floor() as i32)
    }
    
    /// Place a glider at the specified screen position
    pub fn place_glider(&mut self, screen_pos: PhysicalPosition<f64>) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        
        // Skip if out of bounds
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        
        // Glider pattern cells relative to center
        let glider_cells = [
            (0, 1),
            (1, 2),
            (2, 0), (2, 1), (2, 2)
        ];
        
        // Place the glider cells
        for (dx, dy) in &glider_cells {
            self.set_cell_alive(gx + dx, gy + dy);
        }
        
        log::info!("Placed glider at grid position ({}, {})", gx, gy);
    }
    
    /// Place a lightweight spaceship at the specified screen position
    pub fn place_lwss(&mut self, screen_pos: PhysicalPosition<f64>) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        
        // Skip if out of bounds
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        
        // Lightweight spaceship pattern
        let lwss_cells = [
            (0, 1), (0, 3),
            (1, 0),
            (2, 0),
            (3, 0), (3, 3),
            (4, 0), (4, 1), (4, 2)
        ];
        
        // Place the cells
        for (dx, dy) in &lwss_cells {
            self.set_cell_alive(gx + dx, gy + dy);
        }
        
        log::info!("Placed lightweight spaceship at grid position ({}, {})", gx, gy);
    }
    
    /// Place a pulsar at the specified screen position
    pub fn place_pulsar(&mut self, screen_pos: PhysicalPosition<f64>) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        
        // Skip if out of bounds
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        
        // Pulsar pattern (period 3 oscillator)
        let pulsar_cells = [
            // Top horizontal lines
            (2, 0), (3, 0), (4, 0), (8, 0), (9, 0), (10, 0),
            // Top middle horizontal lines
            (2, 5), (3, 5), (4, 5), (8, 5), (9, 5), (10, 5),
            // Bottom middle horizontal lines
            (2, 7), (3, 7), (4, 7), (8, 7), (9, 7), (10, 7),
            // Bottom horizontal lines
            (2, 12), (3, 12), (4, 12), (8, 12), (9, 12), (10, 12),
            
            // Left vertical lines
            (0, 2), (0, 3), (0, 4), (0, 8), (0, 9), (0, 10),
            // Left middle vertical lines
            (5, 2), (5, 3), (5, 4), (5, 8), (5, 9), (5, 10),
            // Right middle vertical lines
            (7, 2), (7, 3), (7, 4), (7, 8), (7, 9), (7, 10),
            // Right vertical lines
            (12, 2), (12, 3), (12, 4), (12, 8), (12, 9), (12, 10),
        ];
        
        // Place the cells
        for (dx, dy) in &pulsar_cells {
            self.set_cell_alive(gx + dx, gy + dy);
        }
        
        log::info!("Placed pulsar at grid position ({}, {})", gx, gy);
    }
    
    /// Place a Gosper glider gun at the specified screen position
    pub fn place_gosper_glider_gun(&mut self, screen_pos: PhysicalPosition<f64>) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        
        // Skip if out of bounds
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        
        // Gosper glider gun pattern
        let gun_cells = [
            // Left block
            (1, 5), (1, 6),
            (2, 5), (2, 6),

            // Left ship
            (11, 5), (11, 6), (11, 7),
            (12, 4), (12, 8),
            (13, 3), (13, 9),
            (14, 3), (14, 9),
            (15, 6),
            (16, 4), (16, 8),
            (17, 5), (17, 6), (17, 7),
            (18, 6),

            // Right ship
            (21, 3), (21, 4), (21, 5),
            (22, 3), (22, 4), (22, 5),
            (23, 2), (23, 6),
            (25, 1), (25, 2), (25, 6), (25, 7),

            // Right block
            (35, 3), (35, 4),
            (36, 3), (36, 4)
        ];
        
        // Place the cells
        for (dx, dy) in &gun_cells {
            self.set_cell_alive(gx + dx, gy + dy);
        }
        
        log::info!("Placed Gosper glider gun at grid position ({}, {})", gx, gy);
    }
    
    /// Place a pentadecathlon (period 15 oscillator) at the specified screen position
    pub fn place_pentadecathlon(&mut self, screen_pos: PhysicalPosition<f64>) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        
        // Skip if out of bounds
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        
        // Pentadecathlon pattern
        let penta_cells = [
            (1, 0), 
            (2, 0), 
            (3, -1), (3, 1),
            (4, 0),
            (5, 0),
            (6, 0),
            (7, 0),
            (8, -1), (8, 1),
            (9, 0),
            (10, 0)
        ];
        
        // Place the cells
        for (dx, dy) in &penta_cells {
            self.set_cell_alive(gx + dx, gy + dy);
        }
        
        log::info!("Placed pentadecathlon at grid position ({}, {})", gx, gy);
    }
    
    /// Place a Simkin glider gun (smaller than Gosper) at the specified screen position
    pub fn place_simkin_glider_gun(&mut self, screen_pos: PhysicalPosition<f64>) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        
        // Skip if out of bounds
        if gx < 0 || gy < 0 || gx >= self.grid_width as i32 || gy >= self.grid_height as i32 {
            return;
        }
        
        // Simkin glider gun pattern
        let simkin_cells = [
            // Left blocks
            (0, 0), (0, 1), (1, 0), (1, 1),
            (4, 0), (4, 1), (5, 0), (5, 1),
            
            // Right side pattern 
            (10, 2), (10, 3), (11, 2), (11, 3),
            
            (12, 0), (13, 0), (12, 1), (13, 1),
            
            (14, 10), (14, 11), (15, 10), (15, 11),
            
            (16, 8), (16, 9), (17, 7), (18, 7),
            (17, 11), (18, 11), (19, 9), (19, 10),
            
            (20, 10),
            
            (21, 8),
            
            (22, 9), (22, 10), (22, 11),
            
            (24, 10), (24, 9), (24, 8),
            
            (24, 7), (25, 7),
            
            (26, 8), (26, 6),
            
            (27, 6), (27, 10),
            
            (28, 9)
        ];
        
        // Place the cells
        for (dx, dy) in &simkin_cells {
            self.set_cell_alive(gx + dx, gy + dy);
        }
        
        log::info!("Placed Simkin glider gun at grid position ({}, {})", gx, gy);
    }
    
    /// Helper function to set a cell to alive state
    fn set_cell_alive(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.grid_width as i32 || y >= self.grid_height as i32 {
            return; // Skip out of bounds cells
        }
        
        let idx = (y as u32 * self.grid_width + x as u32) as usize;
        let val: [f32;1] = [self.current_cell_color.to_value()];
        // Write to the *input* buffer for the *next* frame's compute pass
        self.queue.write_buffer(&self.grid_buffers[self.frame_num % 2], idx as u64 * 4, bytemuck::bytes_of(&val));
    }

    /// Clear an area around the specified screen position
    pub fn clear_area(&mut self, screen_pos: PhysicalPosition<f64>, radius: u32) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        let radius = radius as i32;
        
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                // Skip if distance is greater than radius (circular area)
                if dx*dx + dy*dy > radius*radius {
                    continue;
                }
                
                let cx = gx + dx;
                let cy = gy + dy;
                
                if cx < 0 || cy < 0 || cx >= self.grid_width as i32 || cy >= self.grid_height as i32 {
                    continue; // Skip out of bounds cells
                }
                
                let idx = (cy as u32 * self.grid_width + cx as u32) as usize;
                let val: [f32;1] = [0.0]; // Set to dead (0.0)
                self.queue.write_buffer(&self.grid_buffers[self.frame_num % 2], idx as u64 * 4, bytemuck::bytes_of(&val));
            }
        }
        
        log::info!("Cleared area with radius {} at grid position ({}, {})", radius, gx, gy);
    }
    
    /// Fill an area with random cells around the specified screen position
    pub fn random_fill(&mut self, screen_pos: PhysicalPosition<f64>, radius: u32, density: f32) {
        let (gx, gy) = self.screen_to_grid(screen_pos);
        let radius = radius as i32;
        
        // Use frame_num as a kind of seed for randomization
        let seed = self.frame_num as u32;
        
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                // Skip if distance is greater than radius (circular area)
                if dx*dx + dy*dy > radius*radius {
                    continue;
                }
                
                let cx = gx + dx;
                let cy = gy + dy;
                
                if cx < 0 || cy < 0 || cx >= self.grid_width as i32 || cy >= self.grid_height as i32 {
                    continue; // Skip out of bounds cells
                }
                
                // Simple deterministic random function based on coordinates and seed
                let random_val = {
                    let h1 = (cx as u32).wrapping_mul(17).wrapping_add((cy as u32).wrapping_mul(31));
                    let h2 = h1.wrapping_add(seed.wrapping_mul(43));
                    (h2 % 1000) as f32 / 1000.0
                };
                
                // Only fill some cells based on density
                if random_val < density {
                    let idx = (cy as u32 * self.grid_width + cx as u32) as usize;
                    let val: [f32;1] = [self.current_cell_color.to_value()]; // Set to alive (1.0)
                    self.queue.write_buffer(&self.grid_buffers[self.frame_num % 2], idx as u64 * 4, bytemuck::bytes_of(&val));
                }
            }
        }
        
        log::info!("Randomly filled area with radius {} and density {} at grid position ({}, {})", 
                  radius, density, gx, gy);
    }
} 