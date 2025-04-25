// Declare modules directly in the binary crate root
pub mod state;
pub mod compute;
pub mod render;
pub mod input;
pub mod rules;

// Use types/functions from the declared modules
use crate::state::State;

use winit::{
    event::{Event, WindowEvent, MouseScrollDelta},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use std::sync::Arc;

// GUI Imports
use egui;
use std::time::{Instant, Duration}; // Import time types

// Constants
const GRID_WIDTH: u32 = 256;
const GRID_HEIGHT: u32 = 256;

async fn run(event_loop: EventLoop<()>, window: Arc<Window>) {
    let mut state = State::new(window).await;

    event_loop.run(move |event, window_target| {
        // Pass winit events to egui_winit - MOVED INSIDE WindowEvent arm

        window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id, ref event } // Use `ref event` here
                if window_id == state.window.id() =>
            {
                // Pass window-specific events to egui_winit FIRST
                let response = state.egui_winit_state.on_window_event(&state.window, event);

                if response.repaint {
                    state.window.request_redraw();
                }

                // If egui consumed the event, skip further processing for this event
                // unless it was a Resize event, which the game needs to handle regardless.
                let consumed_by_egui = response.consumed && !matches!(event, WindowEvent::Resized(_));

                if consumed_by_egui {
                     return;
                }

                // Now match on the event for game logic if egui didn't consume it
                match event {
                WindowEvent::CloseRequested => {
                    window_target.exit();
                }
                WindowEvent::Resized(new_size) => {
                        state.resize(*new_size); // Resize must happen even if egui uses it
                        // egui also needs to know about the resize for its layout
                        // state.egui_renderer.resize() // This isn't needed, handled by screen descriptor
                }
                WindowEvent::MouseInput { state: element_state, button, .. } => {
                        input::handle_mouse_input(&mut state, *button, *element_state);
                }
                WindowEvent::CursorMoved { position, .. } => {
                        input::handle_cursor_move(&mut state, *position);
                }
                WindowEvent::CursorLeft { .. } => {
                    input::handle_cursor_left(&mut state);
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                            MouseScrollDelta::LineDelta(_, y) => *y,
                        MouseScrollDelta::PixelDelta(pos) => {
                            if pos.y.abs() > 0.0 {
                                    (pos.y / 20.0) as f32
                            } else {
                                0.0
                            }
                        },
                    };
                    if scroll_amount != 0.0 {
                        input::handle_zoom(&mut state, scroll_amount);
                    }
                }
                WindowEvent::RedrawRequested => {
                        // Run game simulation and rendering.
                        // This now returns the frame to draw egui on, or an error.
                        let game_render_result = state.update_and_render();

                        let output_frame = match game_render_result {
                            Ok(frame) => frame,
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::OutOfMemory) => {
                                // Surface Lost or OOM: Logged in update_and_render.
                                // resize() was called internally if Lost.
                                // We should skip rendering this frame and request redraw.
                                log::warn!("Skipping frame due to surface error.");
                                state.window.request_redraw();
                                return;
                            }
                            Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Outdated) => {
                                // Temporary errors. Log and skip frame, request redraw.
                                log::warn!("Skipping frame due to surface {:?}", game_render_result.unwrap_err());
                                state.window.request_redraw();
                                return;
                            }
                        };

                        // --- Update Cell Count (throttled) ---
                        let mut should_update_count = false;
                        if state.menu_open {
                            match state.last_count_update_time {
                                Some(last_update) => {
                                    if Instant::now().duration_since(last_update) > Duration::from_secs(1) {
                                        should_update_count = true;
                                    }
                                }
                                None => {
                                    // No count yet, update immediately when menu opens
                                    should_update_count = true;
                                }
                            }
                        }
                        // Reset count if menu closed
                        // if !state.menu_open {
                        //     state.live_cell_count = None;
                        //     state.last_count_update_time = None;
                        // }
                        // Perform the potentially blocking update
                        if should_update_count {
                            log::info!("Updating live cell count (GPU readback)...");
                            state.update_live_cell_count();
                            log::info!("Cell count update finished.");
                        }

                        // --- Egui Frame and UI Definition ---
                        let output_view = output_frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

                        // Begin egui frame
                        let raw_input = state.egui_winit_state.take_egui_input(&state.window);
                        state.egui_ctx.begin_frame(raw_input);

                        // --- Define the UI ---
                        // Remove the TopBottomPanel
                        // egui::TopBottomPanel::top("top_panel").show(&state.egui_ctx, |ui| { ... });

                        // Use an Area for the menu button, positioned top-left
                        egui::Area::new(egui::Id::new("menu_button_area"))
                            .anchor(egui::Align2::LEFT_TOP, egui::vec2(5.0, 5.0)) // Anchor top-left, offset slightly
                            .show(&state.egui_ctx, |ui| {
                                // Use only the icon, button frame should be transparent by default in an Area
                                if ui.button("☰").clicked() { // Just the icon
                                    state.menu_open = !state.menu_open;
                                }
                            });

                        if state.menu_open {
                            // Define a frame with a semi-transparent background
                            let panel_frame = egui::Frame {
                                fill: egui::Color32::from_rgba_unmultiplied(25, 25, 25, 100), // Dark grey, ~40% opaque
                                ..egui::Frame::side_top_panel(&state.egui_ctx.style())
                            };

                            egui::SidePanel::left("side_panel")
                                .frame(panel_frame) // Apply the custom frame
                                .resizable(true)
                                .default_width(200.0)
                                .show(&state.egui_ctx, |ui| {
                                ui.heading("Simulation Settings");
                                ui.separator();
                                ui.label("Rule Presets:");
                                if ui.button("Conway's Classic").clicked() {
                                    // TODO: Implement rule changing based on loaded shaders
                                    // state.change_rules(GameRules::conway());
                                    log::info!("Conway button clicked (TODO: load shader)");
                                }
                                if ui.button("HighLife").clicked() {
                                    // TODO: state.change_rules(GameRules::highlife());
                                    log::info!("HighLife button clicked (TODO: load shader)");
                                }
                                if ui.button("Day & Night").clicked() {
                                    // TODO: state.change_rules(GameRules::day_and_night());
                                    log::info!("Day & Night button clicked (TODO: load shader)");
                                }
                                ui.separator();
                                ui.label(format!("Zoom: {:.2}", state.zoom));
                                ui.label(format!("Offset: [{:.1}, {:.1}]", state.view_offset[0], state.view_offset[1]));
                                ui.label(format!("Grid: {}x{}", state.grid_width, state.grid_height));
                                ui.label(format!("Frame: {}", state.frame_num));
                                // Display live cell count
                                ui.label(format!("Live Cells: {}",
                                    state.live_cell_count.map_or_else(|| "N/A".to_string(), |count| count.to_string())
                                ));

                                ui.separator();
                                ui.add(egui::Slider::new(&mut state.brush_radius, 0..=20).text("Brush Radius"));
                                ui.separator();

                                ui.checkbox(&mut state.lucky_rule_enabled, "Enable Lucky Red Cells");
                                ui.separator();

                                // Slider for lucky chance percentage (0-100)
                                // Only has effect if the checkbox above is enabled (checked in shader)
                                ui.add_enabled(
                                    state.lucky_rule_enabled, // Only enable slider if rule is enabled
                                    egui::Slider::new(&mut state.lucky_chance_percent, 0..=100).text("Lucky Chance %")
                                );
                                ui.separator();

                                ui.label("Load Custom Shader Rule:");
                                if ui.button("Load from file...").clicked() {
                                    // TODO: Implement file loading logic
                                    log::info!("Load Shader button clicked (TODO)");
                                }
                            });
                        }
                        // --- End UI Definition ---

                        // End egui frame
                        let full_output = state.egui_ctx.end_frame();
                        let paint_jobs = state.egui_ctx.tessellate(full_output.shapes, state.window.scale_factor() as f32);
                        let screen_descriptor = egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [state.config.width, state.config.height],
                            pixels_per_point: state.window.scale_factor() as f32,
                        };

                        // Upload egui data to GPU
                        let mut encoder = state.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("egui Encoder"),
                        });
                        for (id, image_delta) in &full_output.textures_delta.set {
                            state.egui_renderer.update_texture(&state.device, &state.queue, *id, image_delta);
                        }
                        let _tdelta = state.egui_renderer.update_buffers(
                            &state.device,
                            &state.queue,
                            &mut encoder,
                            &paint_jobs,
                            &screen_descriptor,
                        );
                        state.egui_winit_state.handle_platform_output(
                            &state.window,
                            full_output.platform_output,
                        );

                        // Render egui
                        {
                            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("egui Render Pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &output_view, // Render egui ON TOP of the game texture
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load, // Load the existing game render
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            });

                            state.egui_renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
                        }

                        // Free texture delta
                        for id in &full_output.textures_delta.free {
                            state.egui_renderer.free_texture(id);
                        }

                        // Submit egui command buffer
                        state.queue.submit(Some(encoder.finish()));
                        output_frame.present(); // Present the final frame with game + egui overlay
                    }
                    _ => (),
                }
            }
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

    let initial_size = winit::dpi::LogicalSize::new(GRID_WIDTH as f64, GRID_HEIGHT as f64);

    let window = Arc::new(winit::window::WindowBuilder::new()
        .with_title("GPU Game of Life - Refactored")
        .with_inner_size(initial_size)
        .build(&event_loop)
        .unwrap());

    #[cfg(target_os = "linux")]
    {
        // Wayland workaround (commented out)
        // use winit::platform::wayland::WindowBuilderExtWayland;
        // let builder = winit::window::WindowBuilder::new();
        // let _temp_window = builder.with_name("winit", "winit").build(&event_loop).unwrap();
    }

    pollster::block_on(run(event_loop, window));
} 