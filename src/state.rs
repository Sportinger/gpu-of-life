use crate::compute::{SimParams, WORKGROUP_SIZE, create_compute_bind_groups, ShaderGameRules};
use crate::render::{RenderParams, MIN_ZOOM, create_render_bind_group_layout, create_render_bind_groups};
use crate::rules::{Pattern, place_pattern_on_grid, GameRules};
use wgpu::util::DeviceExt;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    window::Window,
};
use std::sync::Arc;
use std::num::NonZeroU64;
use std::borrow::Cow; // Needed for ShaderSource

// GUI Imports
use egui_winit::State as EguiWinitState;
use egui_wgpu::Renderer as EguiWgpuRenderer;
use egui::Context as EguiContext;

const BRUSH_RADIUS: i32 = 3; // radius in grid cells

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

    // GUI state
    pub egui_ctx: EguiContext,
    pub egui_winit_state: EguiWinitState,
    pub egui_renderer: EguiWgpuRenderer,
    pub menu_open: bool,
    pub lucky_rule_enabled: bool,
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
            present_mode: surface_caps.present_modes[0],
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
            seed: 0,
            enable_lucky_rule: 0,
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

        // Load initial compute shader source
        let initial_compute_shader_source = include_str!("rules/conway_classic.wgsl").to_string();

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
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<SimParams>() as u64),
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
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<ShaderGameRules>() as u64),
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
                 source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&initial_compute_shader_source)),
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
                seed: self.frame_num as u32,
                enable_lucky_rule: if self.lucky_rule_enabled { 1 } else { 0 },
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
        // Update the simulation parameters with the current frame number
        self.queue.write_buffer(&self.sim_param_buffer, 0, bytemuck::bytes_of(&SimParams {
            width: self.grid_width,
            height: self.grid_height,
            seed: self.frame_num as u32,
            enable_lucky_rule: if self.lucky_rule_enabled { 1 } else { 0 },
        }));

        // --- Compute Pass ---
        let mut compute_encoder = self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Compute Encoder") });
        {
            let mut compute_pass = compute_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Game of Life Compute Pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.compute_pipeline); // Use the current pipeline
            compute_pass.set_bind_group(0, &self.compute_bind_groups[self.frame_num % 2], &[]);
            let dispatch_x = (self.grid_width + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            let dispatch_y = (self.grid_height + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
        self.queue.submit(Some(compute_encoder.finish()));

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

        self.frame_num += 1;

        // Return the frame so egui can render to it
        Ok(output_frame)
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
        for by in -BRUSH_RADIUS..=BRUSH_RADIUS {
            for bx in -BRUSH_RADIUS..=BRUSH_RADIUS {
                let cx = gx + bx;
                let cy = gy + by;
                if cx < 0 || cy < 0 || cx >= self.grid_width as i32 || cy >= self.grid_height as i32 {
                    continue;
                }
                let idx = (cy as u32 * self.grid_width + cx as u32) as usize;
                let val: [f32;1] = [1.0];
                // Write to the *input* buffer for the *next* frame's compute pass
                self.queue.write_buffer(&self.grid_buffers[self.frame_num % 2], idx as u64 * 4, bytemuck::bytes_of(&val));
            }
        }
    }
} 