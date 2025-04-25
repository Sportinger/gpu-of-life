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
const GRID_WIDTH: u32 = 1024;
const GRID_HEIGHT: u32 = 1024;

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
                            
                            // Perform the potentially blocking update
                            if should_update_count {
                                log::info!("Updating live cell count (GPU readback)...");
                                state.update_live_cell_count();
                                log::info!("Cell count update finished.");
                            }
                        } else {
                            // When menu is closed, don't perform any cell counting
                            // This avoids expensive GPU readbacks when not needed
                            state.live_cell_count = None;
                            state.last_count_update_time = None;
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
                                fill: egui::Color32::from_rgba_unmultiplied(25, 25, 25, 204), // Dark grey, 80% opaque (20% transparent)
                                ..egui::Frame::side_top_panel(&state.egui_ctx.style())
                            };

                            egui::SidePanel::left("side_panel")
                                .frame(panel_frame) // Apply the custom frame
                                .resizable(true)
                                .default_width(200.0)
                                .show(&state.egui_ctx, |ui| {
                                ui.heading("Simulation Settings");
                                ui.separator();
                                
                                ui.label(format!("Zoom: {:.2}", state.zoom));
                                ui.label(format!("Offset: [{:.1}, {:.1}]", state.view_offset[0], state.view_offset[1]));
                                
                                // Add button for setting zoom to 1:1 pixel mapping
                                let already_at_min_zoom = (state.zoom - crate::render::MIN_ZOOM).abs() < 0.01;
                                if ui.add_enabled(!already_at_min_zoom, egui::Button::new("Reset to 1:1 Pixel Mapping")).clicked() {
                                    // Set zoom directly
                                    let old_zoom = state.zoom;
                                    state.zoom = crate::render::MIN_ZOOM;
                                    
                                    // Adjust view offset to keep center point
                                    let center_x = state.size.width as f32 / 2.0;
                                    let center_y = state.size.height as f32 / 2.0;
                                    
                                    // Calculate world coordinate at center before zoom
                                    let world_x = (center_x + state.view_offset[0]) / old_zoom;
                                    let world_y = (center_y + state.view_offset[1]) / old_zoom;
                                    
                                    // Calculate offset after zoom
                                    state.view_offset[0] = world_x * state.zoom - center_x;
                                    state.view_offset[1] = world_y * state.zoom - center_y;
                                    
                                    // Update GPU buffer
                                    state.queue.write_buffer(&state.render_param_buffer, 0, bytemuck::bytes_of(&crate::render::RenderParams {
                                        zoom: state.zoom,
                                        view_offset: state.view_offset,
                                        _padding: 0.0,
                                    }));
                                }
                                
                                if already_at_min_zoom {
                                    ui.label("Already at 1:1 pixel mapping (one pixel = one cell)");
                                }
                                
                                ui.label(format!("Grid: {}x{}", state.grid_width, state.grid_height));
                                ui.label(format!("Frame: {}", state.frame_num));
                                // Display live cell count
                                ui.label(format!("Live Cells: {}",
                                    state.live_cell_count.map_or_else(|| "N/A".to_string(), |count| count.to_string())
                                ));
                                // Display current FPS
                                let fps_text = format!("FPS: {:.1}", state.fps);
                                let fps_color = if state.fps > 100.0 {
                                    egui::Color32::GREEN // High FPS (good)
                                } else if state.fps > 30.0 {
                                    egui::Color32::YELLOW // Medium FPS (acceptable)
                                } else {
                                    egui::Color32::RED // Low FPS (potential issues)
                                };
                                ui.colored_label(fps_color, fps_text);
                                
                                // Detect if FPS is close to monitor refresh rate (likely vsync limited)
                                if state.fps > 115.0 && state.fps < 125.0 {
                                    ui.label("⚠️ FPS appears limited by 120Hz refresh rate");
                                } else if state.fps > 55.0 && state.fps < 65.0 {
                                    ui.label("⚠️ FPS appears limited by 60Hz refresh rate");
                                }

                                ui.separator();
                                ui.add(egui::Slider::new(&mut state.brush_radius, 0..=20).text("Brush Radius"));
                                ui.separator();

                                // Add cell color selection to the main menu
                                ui.label("Cell Color:");
                                ui.horizontal(|ui| {
                                    // Display current color as a colored circle
                                    let current_color = match state.current_cell_color {
                                        crate::state::CellColor::White => egui::Color32::WHITE,
                                        crate::state::CellColor::Red => egui::Color32::RED,
                                        crate::state::CellColor::Green => egui::Color32::GREEN,
                                        crate::state::CellColor::Blue => egui::Color32::from_rgb(0, 120, 255),
                                        crate::state::CellColor::Yellow => egui::Color32::YELLOW,
                                        crate::state::CellColor::Purple => egui::Color32::from_rgb(200, 100, 255),
                                    };
                                    
                                    // Show a color indicator
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                    ui.painter().circle_filled(
                                        rect.center(), 
                                        10.0, 
                                        current_color
                                    );
                                    
                                    ui.label(format!("Current: {}", match state.current_cell_color {
                                        crate::state::CellColor::White => "White",
                                        crate::state::CellColor::Red => "Red",
                                        crate::state::CellColor::Green => "Green",
                                        crate::state::CellColor::Blue => "Blue",
                                        crate::state::CellColor::Yellow => "Yellow",
                                        crate::state::CellColor::Purple => "Purple",
                                    }));
                                });
                                
                                // Add color buttons in a grid
                                ui.horizontal(|ui| {
                                    if ui.button("White").clicked() {
                                        state.current_cell_color = crate::state::CellColor::White;
                                    }
                                    if ui.button("Red").clicked() {
                                        state.current_cell_color = crate::state::CellColor::Red;
                                    }
                                    if ui.button("Green").clicked() {
                                        state.current_cell_color = crate::state::CellColor::Green;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    if ui.button("Blue").clicked() {
                                        state.current_cell_color = crate::state::CellColor::Blue;
                                    }
                                    if ui.button("Yellow").clicked() {
                                        state.current_cell_color = crate::state::CellColor::Yellow;
                                    }
                                    if ui.button("Purple").clicked() {
                                        state.current_cell_color = crate::state::CellColor::Purple;
                                    }
                                });
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

                                // Add simulation speed slider
                                ui.label("Simulation Speed:");
                                ui.add(egui::Slider::new(&mut state.simulation_speed, 1..=100_000)
                                    .text("Steps/second")
                                    .logarithmic(true)
                                    .custom_formatter(|val, _| {
                                        if val <= 1.0 {
                                            "1 step/sec".to_string()
                                        } else if val >= 100_000.0 {
                                            "100K steps/sec".to_string()
                                        } else if val >= 10_000.0 {
                                            format!("{:.0}K steps/sec", val / 1000.0)
                                        } else if val >= 1000.0 {
                                            format!("{:.1}K steps/sec", val / 1000.0)
                                        } else {
                                            format!("{:.0} steps/sec", val)
                                        }
                                    }));
                            });
                        }
                        // --- End UI Definition ---

                        // Context menu (if shown)
                        if state.show_context_menu {
                            if let Some(pos) = state.context_menu_pos {
                                // Convert position to egui coordinates
                                let screen_pos = egui::pos2(pos.x as f32, pos.y as f32);
                                
                                // Store user actions to perform after UI rendering
                                let mut new_cursor_mode = None;
                                let mut show_submenu_for = None;
                                
                                egui::Area::new(egui::Id::new("context_menu"))
                                    .movable(false)
                                    .order(egui::Order::Foreground)
                                    .fixed_pos(screen_pos)
                                    .show(&state.egui_ctx, |ui| {
                                        // Create a frame for the context menu
                                        egui::Frame::popup(&state.egui_ctx.style())
                                            .fill(egui::Color32::from_rgba_unmultiplied(25, 25, 25, 204)) // 80% opaque (20% transparent)
                                            .show(ui, |ui| {
                                                ui.set_min_width(150.0); // Set minimum width
                                                
                                                // Menu options with right-click handling for submenu
                                                let paint_response = ui.button("Paint Cells (Default)");
                                                if paint_response.clicked() { // Left-click
                                                    new_cursor_mode = Some(crate::state::CursorMode::Paint);
                                                }
                                                if paint_response.secondary_clicked() { // Right-click
                                                    show_submenu_for = Some("paint".to_string());
                                                }
                                                
                                                let glider_response = ui.button("Place Glider");
                                                if glider_response.clicked() {
                                                    new_cursor_mode = Some(crate::state::CursorMode::PlaceGlider);
                                                }
                                                if glider_response.secondary_clicked() {
                                                    show_submenu_for = Some("glider".to_string());
                                                }
                                                
                                                let clear_response = ui.button("Clear Area (15px radius)");
                                                if clear_response.clicked() {
                                                    new_cursor_mode = Some(crate::state::CursorMode::ClearArea);
                                                }
                                                if clear_response.secondary_clicked() {
                                                    show_submenu_for = Some("clear".to_string());
                                                }
                                                
                                                let random_response = ui.button("Random Fill (20px radius)");
                                                if random_response.clicked() {
                                                    new_cursor_mode = Some(crate::state::CursorMode::RandomFill);
                                                }
                                                if random_response.secondary_clicked() {
                                                    show_submenu_for = Some("random".to_string());
                                                }
                                            });
                                    });
                                
                                // Handle cursor mode changes or submenu display
                                if let Some(mode) = new_cursor_mode {
                                    state.cursor_mode = mode;
                                    state.show_context_menu = false;
                                    state.show_submenu = false;
                                    log::info!("Cursor mode changed to: {:?}", mode);
                                } else if let Some(option) = show_submenu_for {
                                    // Get the position for the submenu (near the parent option)
                                    state.submenu_parent = Some(option.clone());
                                    state.show_submenu = true;
                                    state.submenu_pos = Some(pos);
                                    log::info!("Showing submenu for: {}", option);
                                }
                            }
                        }
                        
                        // Submenu (if shown)
                        if state.show_submenu {
                            if let Some(pos) = state.submenu_pos {
                                // Define a width for the submenu, depending on the parent type
                                let submenu_width = match state.submenu_parent.as_ref().map(|s| s.as_str()) {
                                    Some("glider") => 220.0, // Wider for glider submenu (has longer options)
                                    Some("paint") => 220.0, // Wider for paint submenu (has more options)
                                    _ => 150.0,
                                };
                                
                                // Check if the submenu would go off-screen on the right side
                                let window_width = state.size.width as f32;
                                let submenu_right_edge = pos.x as f32 + 150.0 + submenu_width;
                                let would_be_offscreen = submenu_right_edge > window_width;
                                let offscreen_percent = if would_be_offscreen {
                                    (submenu_right_edge - window_width) / submenu_width * 100.0
                                } else {
                                    0.0
                                };
                                
                                // If more than 10% would be off-screen, position on the left
                                let submenu_pos = if offscreen_percent > 10.0 {
                                    // Position on the left side (offset by submenu width + some padding)
                                    egui::pos2((pos.x as f32 - submenu_width - 10.0), pos.y as f32)
                                } else {
                                    // Position on the right side as before
                                    egui::pos2((pos.x + 150.0) as f32, pos.y as f32)
                                };
                                
                                egui::Area::new(egui::Id::new("submenu"))
                                    .movable(false)
                                    .order(egui::Order::Foreground)
                                    .fixed_pos(submenu_pos)
                                    .show(&state.egui_ctx, |ui| {
                                        // Create a frame for the submenu
                                        egui::Frame::popup(&state.egui_ctx.style())
                                            .fill(egui::Color32::from_rgba_unmultiplied(25, 25, 25, 204)) // 80% opaque (20% transparent)
                                            .show(ui, |ui| {
                                                // Set exact width based on content
                                                ui.set_max_width(submenu_width);
                                                
                                                // Display a header showing which option this submenu is for
                                                if let Some(parent) = &state.submenu_parent {
                                                    // Capitalize first letter of parent
                                                    let capitalized = parent.chars().next()
                                                        .map(|c| c.to_uppercase().collect::<String>())
                                                        .unwrap_or_default() + &parent[1..];
                                                    
                                                    ui.heading(format!("{} Options", capitalized));
                                                    ui.separator();
                                                }
                                                
                                                // Different submenu options based on the parent
                                                if let Some(parent) = &state.submenu_parent {
                                                    match parent.as_str() {
                                                        "glider" => {
                                                            // Show different structure placement options
                                                            if ui.button("Standard Glider").clicked() {
                                                                state.cursor_mode = crate::state::CursorMode::PlaceGlider;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Standard Glider");
                                                            }
                                                            
                                                            if ui.button("Lightweight Spaceship").clicked() {
                                                                state.cursor_mode = crate::state::CursorMode::PlaceLWSS;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Lightweight Spaceship");
                                                            }
                                                            
                                                            if ui.button("Pulsar (Period 3)").clicked() {
                                                                state.cursor_mode = crate::state::CursorMode::PlacePulsar;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Pulsar");
                                                            }
                                                            
                                                            if ui.button("Pentadecathlon (Period 15)").clicked() {
                                                                state.cursor_mode = crate::state::CursorMode::PlacePentadecathlon;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Pentadecathlon");
                                                            }
                                                            
                                                            if ui.button("Gosper Glider Gun").clicked() {
                                                                state.cursor_mode = crate::state::CursorMode::PlaceGosperGun;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Gosper Glider Gun");
                                                            }
                                                            
                                                            if ui.button("Simkin Glider Gun").clicked() {
                                                                state.cursor_mode = crate::state::CursorMode::PlaceSimkinGun;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Simkin Glider Gun");
                                                            }
                                                        },
                                                        "paint" => {
                                                            // Show color selection options
                                                            ui.heading("Cell Color Options");
                                                            ui.separator();
                                                            
                                                            // White color option
                                                            if ui.add(egui::Button::new(
                                                                egui::RichText::new("White")
                                                                    .color(egui::Color32::WHITE)
                                                                    .background_color(egui::Color32::from_rgba_premultiplied(50, 50, 50, 200))
                                                            )).clicked() {
                                                                state.current_cell_color = crate::state::CellColor::White;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected White cell color");
                                                            }
                                                            
                                                            // Red color option
                                                            if ui.add(egui::Button::new(
                                                                egui::RichText::new("Red")
                                                                    .color(egui::Color32::RED)
                                                                    .background_color(egui::Color32::from_rgba_premultiplied(50, 50, 50, 200))
                                                            )).clicked() {
                                                                state.current_cell_color = crate::state::CellColor::Red;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Red cell color");
                                                            }
                                                            
                                                            // Green color option
                                                            if ui.add(egui::Button::new(
                                                                egui::RichText::new("Green")
                                                                    .color(egui::Color32::GREEN)
                                                                    .background_color(egui::Color32::from_rgba_premultiplied(50, 50, 50, 200))
                                                            )).clicked() {
                                                                state.current_cell_color = crate::state::CellColor::Green;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Green cell color");
                                                            }
                                                            
                                                            // Blue color option
                                                            if ui.add(egui::Button::new(
                                                                egui::RichText::new("Blue")
                                                                    .color(egui::Color32::from_rgb(0, 120, 255))
                                                                    .background_color(egui::Color32::from_rgba_premultiplied(50, 50, 50, 200))
                                                            )).clicked() {
                                                                state.current_cell_color = crate::state::CellColor::Blue;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Blue cell color");
                                                            }
                                                            
                                                            // Yellow color option
                                                            if ui.add(egui::Button::new(
                                                                egui::RichText::new("Yellow")
                                                                    .color(egui::Color32::YELLOW)
                                                                    .background_color(egui::Color32::from_rgba_premultiplied(50, 50, 50, 200))
                                                            )).clicked() {
                                                                state.current_cell_color = crate::state::CellColor::Yellow;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Yellow cell color");
                                                            }
                                                            
                                                            // Purple color option
                                                            if ui.add(egui::Button::new(
                                                                egui::RichText::new("Purple")
                                                                    .color(egui::Color32::from_rgb(200, 100, 255))
                                                                    .background_color(egui::Color32::from_rgba_premultiplied(50, 50, 50, 200))
                                                            )).clicked() {
                                                                state.current_cell_color = crate::state::CellColor::Purple;
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                                log::info!("Selected Purple cell color");
                                                            }
                                                        },
                                                        // Add other submenu parent options...
                                                        _ => {
                                                            // Generic submenu options for other parent items
                                                            if ui.button("Submenu Option 1").clicked() {
                                                                log::info!("Submenu option 1 selected");
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                            }
                                                            
                                                            if ui.button("Submenu Option 2").clicked() {
                                                                log::info!("Submenu option 2 selected");
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                            }
                                                            
                                                            if ui.button("Submenu Option 3").clicked() {
                                                                log::info!("Submenu option 3 selected");
                                                                state.show_submenu = false;
                                                                state.show_context_menu = false;
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                    });
                            }
                        }

                        // Cursor Mode Indicator
                        if let Some(cursor_pos) = state.cursor_pos {
                            use crate::state::CursorMode;
                            
                            // Don't show cursor indicator when context menu is open
                            if !state.show_context_menu {
                                let cursor_screen_pos = egui::pos2(cursor_pos.x as f32, cursor_pos.y as f32);
                                
                                egui::Area::new(egui::Id::new("cursor_indicator"))
                                    .movable(false)
                                    .order(egui::Order::Foreground)
                                    .fixed_pos(cursor_screen_pos + egui::vec2(15.0, 15.0)) // Offset from actual cursor
                                    .show(&state.egui_ctx, |ui| {
                                        // Different indicators based on mode
                                        match state.cursor_mode {
                                            CursorMode::Paint => {
                                                // Default mode, no special indicator
                                                // Show the current color alongside the cursor
                                                let color_text = match state.current_cell_color {
                                                    crate::state::CellColor::White => "White",
                                                    crate::state::CellColor::Red => "Red",
                                                    crate::state::CellColor::Green => "Green",
                                                    crate::state::CellColor::Blue => "Blue",
                                                    crate::state::CellColor::Yellow => "Yellow",
                                                    crate::state::CellColor::Purple => "Purple",
                                                };
                                                
                                                let color = match state.current_cell_color {
                                                    crate::state::CellColor::White => egui::Color32::WHITE,
                                                    crate::state::CellColor::Red => egui::Color32::RED,
                                                    crate::state::CellColor::Green => egui::Color32::GREEN,
                                                    crate::state::CellColor::Blue => egui::Color32::from_rgb(0, 120, 255),
                                                    crate::state::CellColor::Yellow => egui::Color32::YELLOW,
                                                    crate::state::CellColor::Purple => egui::Color32::from_rgb(200, 100, 255),
                                                };
                                                
                                                ui.label(egui::RichText::new(format!("🖌 Color: {}", color_text))
                                                    .color(color)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::PlaceGlider => {
                                                ui.label(egui::RichText::new("🚀 Glider").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::PlaceLWSS => {
                                                ui.label(egui::RichText::new("🚀 Lightweight Spaceship").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::PlacePulsar => {
                                                ui.label(egui::RichText::new("🔄 Pulsar").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::PlaceGosperGun => {
                                                ui.label(egui::RichText::new("🔫 Gosper Gun").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::PlacePentadecathlon => {
                                                ui.label(egui::RichText::new("🔄 Pentadecathlon").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::PlaceSimkinGun => {
                                                ui.label(egui::RichText::new("🔫 Simkin Gun").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::ClearArea => {
                                                ui.label(egui::RichText::new("🧹 Clear").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                            CursorMode::RandomFill => {
                                                ui.label(egui::RichText::new("🎲 Random").color(egui::Color32::WHITE)
                                                    .background_color(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200)));
                                            },
                                        }
                                    });
                            }
                        }

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