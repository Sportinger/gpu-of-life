use wgpu::util::DeviceExt;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent, MouseScrollDelta, MouseButton, ElementState},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU64;

// Constants
const GRID_WIDTH: u32 = 256; // Increased size a bit
const GRID_HEIGHT: u32 = 256;
const WORKGROUP_SIZE: u32 = 8;
const GRID_SIZE: u64 = (GRID_WIDTH * GRID_HEIGHT) as u64;
const BUFFER_SIZE: u64 = GRID_SIZE * std::mem::size_of::<f32>() as u64;
const MIN_ZOOM: f32 = 1.0; // Min zoom is 1:1 pixel mapping
const MAX_ZOOM: f32 = 16.0; // Max zoom factor
const ZOOM_FACTOR_STEP: f32 = 1.2; // How much each wheel step zooms

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SimParams {
    width: u32,
    height: u32,
}

// Uniforms specific to rendering
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RenderParams {
    zoom: f32,
    view_offset: [f32; 2],
    _padding: f32, // Ensure 16-byte alignment (f32 + vec2<f32> + f32 = 4 + 8 + 4 = 16)
}

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,

    grid_width: u32,
    grid_height: u32,
    grid_buffers: [wgpu::Buffer; 2],
    sim_param_buffer: wgpu::Buffer,

    compute_pipeline: wgpu::ComputePipeline,
    compute_bind_group_layout: wgpu::BindGroupLayout,
    compute_bind_groups: [wgpu::BindGroup; 2],

    render_pipeline: wgpu::RenderPipeline,
    render_bind_group_layout: wgpu::BindGroupLayout,
    render_bind_groups: [wgpu::BindGroup; 2],
    render_param_buffer: wgpu::Buffer,

    frame_num: usize,
    zoom: f32,
    view_offset: [f32; 2], // Current view offset (in grid coordinates)
    is_right_mouse_pressed: bool,
    last_mouse_pos: Option<PhysicalPosition<f64>>,
    cursor_pos: Option<PhysicalPosition<f64>>, // For zoom centering
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        // Ensure size is not zero, necessary for buffer/texture creation
        let initial_grid_width = size.width.max(1);
        let initial_grid_height = size.height.max(1);

        log::info!("Initializing wgpu...");

        let instance = wgpu::Instance::default();

        // Create surface using Arc<Window>
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

        // --- Create Resources (Buffers, Pipelines, Bind Groups) ---
        let (grid_buffers, sim_param_buffer) = 
            Self::create_grid_buffers(&device, initial_grid_width, initial_grid_height);
        queue.write_buffer(&sim_param_buffer, 0, bytemuck::bytes_of(&SimParams { width: initial_grid_width, height: initial_grid_height }));
        Self::initialize_grid_buffer(&queue, &grid_buffers[0], initial_grid_width, initial_grid_height);

        // --- Create Render Resources ---
        let initial_zoom = 1.0;
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

        // Load shaders
        let compute_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../compute.wgsl").into()),
        });
        let render_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Render Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../render.wgsl").into()),
        });

        // Compute Pipeline
        let compute_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { 
            label: Some("Compute Bind Group Layout"),
            entries: &[
                // SimParams Uniform Buffer (Binding 0)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<SimParams>() as u64),
                    },
                    count: None,
                },
                // Input Grid Buffer (Binding 1)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None, 
                    },
                    count: None,
                },
                // Output Grid Buffer (Binding 2)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
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
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader_module,
            entry_point: "main",
        });
        let compute_bind_groups = Self::create_compute_bind_groups(
            &device, &compute_bind_group_layout, &grid_buffers, &sim_param_buffer
        );

        // Render Pipeline
        let render_bind_group_layout = Self::create_render_bind_group_layout(&device);
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
                buffers: &[], // No vertex buffers needed
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
        let render_bind_groups = Self::create_render_bind_groups(
            &device, &render_bind_group_layout, &grid_buffers, &sim_param_buffer, &render_param_buffer
        );

        log::info!("wgpu initialized successfully.");

        Self {
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
            compute_pipeline,
            compute_bind_group_layout,
            compute_bind_groups,
            render_pipeline,
            render_bind_group_layout,
            render_bind_groups,
            render_param_buffer,
            frame_num: 0,
            zoom: initial_zoom,
            view_offset: initial_view_offset,
            is_right_mouse_pressed: false,
            last_mouse_pos: None,
            cursor_pos: None, // Initialize cursor position
        }
    }

    // Helper function to create grid buffers
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

    // Helper function to initialize one grid buffer
    fn initialize_grid_buffer(queue: &wgpu::Queue, buffer: &wgpu::Buffer, width: u32, height: u32) {
        let grid_size = (width * height) as usize;
        let initial_data = {
            let mut data = vec![0.0f32; grid_size];
            if width > 4 && height > 4 { // Ensure space for glider
                 let glider_pos = (width / 4, height / 4);
                 let idx = |x, y| ((y * width + x) as usize).min(grid_size - 1);
                 data[idx(glider_pos.0, glider_pos.1 + 1)] = 1.0;
                 data[idx(glider_pos.0 + 1, glider_pos.1 + 2)] = 1.0;
                 data[idx(glider_pos.0 + 2, glider_pos.1)] = 1.0;
                 data[idx(glider_pos.0 + 2, glider_pos.1 + 1)] = 1.0;
                 data[idx(glider_pos.0 + 2, glider_pos.1 + 2)] = 1.0;
            }
            data
        };
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&initial_data));
    }

    // Helper function to create compute bind groups
    fn create_compute_bind_groups(
        device: &wgpu::Device, 
        layout: &wgpu::BindGroupLayout, 
        grid_buffers: &[wgpu::Buffer; 2], 
        sim_param_buffer: &wgpu::Buffer
    ) -> [wgpu::BindGroup; 2] {
        [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Compute Bind Group 0"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[0].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: grid_buffers[1].as_entire_binding() },
                ],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Compute Bind Group 1"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[1].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: grid_buffers[0].as_entire_binding() },
                ],
            }),
        ]
    }

    // Updated function signature
    fn create_render_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { 
            label: Some("Render Bind Group Layout"),
            entries: &[
                // SimParams Uniform (Binding 0)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<SimParams>() as u64),
                    },
                    count: None,
                },
                // Grid State Buffer (Binding 1)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None, 
                    },
                    count: None,
                },
                // RenderParams Uniform (Binding 2) - NEW
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(std::mem::size_of::<RenderParams>() as u64),
                    },
                    count: None,
                },
            ],
         })
    }

    // Updated function signature and body
    fn create_render_bind_groups(
        device: &wgpu::Device, 
        layout: &wgpu::BindGroupLayout, 
        grid_buffers: &[wgpu::Buffer; 2], 
        sim_param_buffer: &wgpu::Buffer,
        render_param_buffer: &wgpu::Buffer // NEW
    ) -> [wgpu::BindGroup; 2] {
        [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Render Bind Group 0"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[0].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: render_param_buffer.as_entire_binding() }, // NEW
                ],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Render Bind Group 1"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[1].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: render_param_buffer.as_entire_binding() }, // NEW
                ],
            }),
        ]
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
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
            }));

            // Re-initialize buffer 0 (clears state on resize)
            Self::initialize_grid_buffer(&self.queue, &self.grid_buffers[0], self.grid_width, self.grid_height);
            
            // Recreate bind groups
            self.compute_bind_groups = Self::create_compute_bind_groups(
                &self.device, &self.compute_bind_group_layout, &self.grid_buffers, &self.sim_param_buffer
            );
             self.render_bind_groups = Self::create_render_bind_groups(
                &self.device, &self.render_bind_group_layout, &self.grid_buffers, &self.sim_param_buffer, &self.render_param_buffer
            );

            // Reset frame counter to ensure correct initial buffer read
            self.frame_num = 0;

            log::info!("Resized grid and reconfigured surface to: {}x{}", self.grid_width, self.grid_height);
        } else {
            log::warn!("Ignoring resize to zero dimensions: {}x{}", new_size.width, new_size.height);
        }
    }

    fn update_and_render(&mut self) {
        // --- Compute Pass --- 
        let mut compute_encoder = self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Compute Encoder") });
        {
            let mut compute_pass = compute_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Game of Life Compute Pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.compute_pipeline);
            compute_pass.set_bind_group(0, &self.compute_bind_groups[self.frame_num % 2], &[]);
            let dispatch_x = (self.grid_width + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            let dispatch_y = (self.grid_height + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        } 
        self.queue.submit(Some(compute_encoder.finish()));

        // --- Render Pass --- 
        let output_frame = self.surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
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
            render_pass.set_bind_group(0, &self.render_bind_groups[(self.frame_num + 1) % 2], &[]); 
            render_pass.draw(0..3, 0..1); // Draw full-screen triangle
        }
        self.queue.submit(Some(render_encoder.finish()));
        output_frame.present();

        self.frame_num += 1;
    }

    fn handle_zoom(&mut self, delta: f32) {
        let old_zoom = self.zoom;
        let zoom_factor = if delta > 0.0 {
            ZOOM_FACTOR_STEP
        } else {
            1.0 / ZOOM_FACTOR_STEP
        };
        let mut new_zoom = old_zoom * zoom_factor;
        new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);

        // If zoom didn't change, do nothing
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            return;
        }

        let mut new_offset = self.view_offset;

        // Calculate offset adjustment based on cursor position
        if let Some(cursor_pos) = self.cursor_pos {
            let cursor_screen_x = cursor_pos.x as f32;
            let cursor_screen_y = cursor_pos.y as f32;

            // Formula derived: V_new = V_old + (factor - 1) * (C + V_old)
            // where factor = new_zoom / old_zoom
            let effective_factor = new_zoom / old_zoom;
            new_offset[0] += (effective_factor - 1.0) * (cursor_screen_x + self.view_offset[0]);
            new_offset[1] += (effective_factor - 1.0) * (cursor_screen_y + self.view_offset[1]);

        } else {
            // Optional: Zoom towards center if cursor is outside window?
            // For simplicity, we can just not adjust offset if cursor isn't known.
        }
        
        // If new zoom is 1.0, force offset back to 0,0
        if (new_zoom - MIN_ZOOM).abs() < f32::EPSILON {
             new_offset = [0.0, 0.0];
        }

        self.zoom = new_zoom;
        self.view_offset = new_offset;

        log::info!("Zoom: {:.2}, Offset: [{:.1}, {:.1}]", self.zoom, self.view_offset[0], self.view_offset[1]);

        // Update the render params uniform buffer
        self.queue.write_buffer(&self.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
            zoom: self.zoom,
            view_offset: self.view_offset,
            _padding: 0.0,
        }));
    }

    fn handle_mouse_input(&mut self, button: MouseButton, state: winit::event::ElementState) {
        if button == MouseButton::Right {
            let is_pressed = state == winit::event::ElementState::Pressed;
            self.is_right_mouse_pressed = is_pressed;
            if !is_pressed {
                // Reset last position when button is released
                self.last_mouse_pos = None;
            }
        }
    }

    fn handle_cursor_move(&mut self, position: PhysicalPosition<f64>) {
        // Update general cursor position
        self.cursor_pos = Some(position);

        // Handle panning delta (only if right button pressed)
        if self.is_right_mouse_pressed {
            if let Some(last_pos) = self.last_mouse_pos {
                let dx = position.x - last_pos.x;
                let dy = position.y - last_pos.y;
                if self.zoom > MIN_ZOOM {
                    self.view_offset[0] -= (dy / self.zoom as f64) as f32;
                    self.view_offset[1] -= (dx / self.zoom as f64) as f32;
                    self.queue.write_buffer(&self.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
                        zoom: self.zoom,
                        view_offset: self.view_offset,
                         _padding: 0.0,
                    }));
                } else {
                    // Reset offset if zoomed out
                    if self.view_offset != [0.0, 0.0] {
                        self.view_offset = [0.0, 0.0];
                        self.queue.write_buffer(&self.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
                            zoom: self.zoom,
                            view_offset: self.view_offset,
                             _padding: 0.0,
                        }));
                    }
                }
            }
            self.last_mouse_pos = Some(position);
        } else {
            self.last_mouse_pos = None;
        }
    }

    fn handle_cursor_left(&mut self) {
        self.cursor_pos = None;
        self.last_mouse_pos = None; // Also stop panning if cursor leaves
    }
}

// --- Main loop uses State --- 
async fn run(event_loop: EventLoop<()>, window: Arc<Window>) {
    let mut state = State::new(window).await;

    event_loop.run(move |event, window_target| {
        window_target.set_control_flow(ControlFlow::Poll);
        
        match event {
            Event::WindowEvent { window_id, event } if window_id == state.window.id() => match event {
                WindowEvent::CloseRequested => {
                    window_target.exit();
                }
                WindowEvent::Resized(new_size) => {
                    state.resize(new_size);
                }
                WindowEvent::MouseInput { state: element_state, button, .. } => {
                    state.handle_mouse_input(button, element_state);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    state.handle_cursor_move(position);
                }
                WindowEvent::CursorLeft { .. } => {
                    state.handle_cursor_left();
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    state.handle_zoom(scroll_amount);
                }
                WindowEvent::RedrawRequested => {
                    state.update_and_render();
                }
                _ => (),
            },
            Event::AboutToWait => { 
                state.window.request_redraw();
            }
            _ => ()
        }
    })
    .unwrap();
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(winit::window::WindowBuilder::new()
        .with_title("GPU Game of Life - Resizable")
        .with_inner_size(winit::dpi::LogicalSize::new(512, 512)) // Start with a default size
        .build(&event_loop)
        .unwrap());

    // Temporarily avoid segmentation fault on Linux Wayland
    // See: https://github.com/rust-windowing/winit/issues/2787
    #[cfg(target_os = "linux")]
    {
        use winit::platform::wayland::WindowBuilderExtWayland;
        let builder = winit::window::WindowBuilder::new();
        // Create a temporary window for the workaround; the main window is already created.
        let _temp_window = builder.with_name("winit", "winit").build(&event_loop).unwrap();
    }

    pollster::block_on(run(event_loop, window));
} 