use crate::state::State;
use crate::render::{RenderParams, MIN_ZOOM, MAX_ZOOM, ZOOM_FACTOR_STEP};
use winit::{
    dpi::PhysicalPosition,
    event::{MouseButton, ElementState},
};

// Track if this is a click or drag
const DRAG_THRESHOLD: f64 = 3.0; // Pixels of movement before considered a drag

pub fn handle_zoom(state: &mut State, delta: f32) {
    let old_zoom = state.zoom;
    let zoom_factor = if delta > 0.0 {
        ZOOM_FACTOR_STEP
    } else {
        1.0 / ZOOM_FACTOR_STEP
    };
    let mut new_zoom = old_zoom * zoom_factor;
    new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);

    if (new_zoom - old_zoom).abs() < f32::EPSILON {
        return;
    }

    let mut new_offset = state.view_offset;

    if let Some(cursor_pos) = state.cursor_pos {
        let cursor_screen_x = cursor_pos.x as f32;
        let cursor_screen_y = cursor_pos.y as f32;

        // 1. Calculate world coordinate under cursor BEFORE zoom
        let world_x = (cursor_screen_x + state.view_offset[0]) / old_zoom;
        let world_y = (cursor_screen_y + state.view_offset[1]) / old_zoom;

        // 2. Calculate the required offset AFTER zoom to keep the world point under the cursor
        new_offset[0] = world_x * new_zoom - cursor_screen_x;
        new_offset[1] = world_y * new_zoom - cursor_screen_y;

    } else {
        // Optional: Fallback behavior if cursor is not in window (e.g., zoom towards center)
        // Currently does nothing, keeping the previous offset which effectively centers zoom on (0,0) world space.
        // Or, could calculate center screen coords and use those like the formula above.
        // For simplicity, we'll keep the current behavior: zoom towards origin if cursor is outside.
    }

    state.zoom = new_zoom;
    state.view_offset = new_offset;
    clamp_offset(state);

    log::info!("Zoom: {:.2}, Offset: [{:.1}, {:.1}]", state.zoom, state.view_offset[0], state.view_offset[1]);

    state.queue.write_buffer(&state.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
        zoom: state.zoom,
        view_offset: state.view_offset,
        _padding: 0.0,
    }));
}

pub fn handle_mouse_input(state: &mut State, button: MouseButton, element_state: ElementState) {
    if button == MouseButton::Right {
        let is_pressed = element_state == ElementState::Pressed;
        
        if is_pressed {
            // Starting a potential right click or drag
            state.is_right_mouse_pressed = true;
            state.right_click_start_pos = state.cursor_pos;
            state.right_drag_started = false;
        } else {
            // Released right mouse button
            if state.is_right_mouse_pressed && !state.right_drag_started && state.cursor_pos.is_some() {
                // This was a click (not a drag)
                // Only trigger context menu if not already showing one
                if !state.show_context_menu && !state.show_submenu {
                    state.show_context_menu = true;
                    state.context_menu_pos = state.cursor_pos;
                    log::info!("Context menu triggered at {:?}", state.context_menu_pos);
                }
            }
            
            state.is_right_mouse_pressed = false;
            state.right_click_start_pos = None;
        }
    } else if button == MouseButton::Left {
        let is_pressed = element_state == ElementState::Pressed;
        state.is_left_mouse_pressed = is_pressed;
        
        if is_pressed {
            // When mouse is first pressed, store the current position
            // and timestamp for later drag detection and speed calculation
            state.drag_start_pos = state.cursor_pos;
            state.last_mouse_pos = state.cursor_pos;
            state.last_action_time = Some(std::time::Instant::now());
            state.is_dragging = false;
            
            // Handle the initial click placement
            if let Some(pos) = state.cursor_pos {
                apply_cursor_mode_action(state, pos, false); // Not dragging yet
            }
        } else {
            // Mouse button released, reset dragging state
            state.is_dragging = false;
            state.drag_start_pos = None;
            state.last_action_time = None;
        }
    }
}

pub fn handle_cursor_move(state: &mut State, position: PhysicalPosition<f64>) {
    state.cursor_pos = Some(position);

    if state.is_right_mouse_pressed {
        // Right mouse dragging for panning (existing code)
        // Check if this is the start of a drag
        if !state.right_drag_started {
            if let Some(start_pos) = state.right_click_start_pos {
                let dx = position.x - start_pos.x;
                let dy = position.y - start_pos.y;
                let distance_squared = dx * dx + dy * dy;
                
                if distance_squared > DRAG_THRESHOLD * DRAG_THRESHOLD {
                    state.right_drag_started = true;
                    state.show_context_menu = false; // Cancel any potential context menu
                    log::info!("Right drag started");
                }
            }
        }
        
        // Only pan if we're in drag mode
        if state.right_drag_started {
            if let Some(last_pos) = state.last_mouse_pos {
                let dx_screen = position.x - last_pos.x;
                let dy_screen = position.y - last_pos.y;

                // Map mouse movement (screen delta) directly to view offset for consistent panning speed.
                // Subtracting the screen delta makes the view move with the cursor drag.
                state.view_offset[0] -= dx_screen as f32;
                state.view_offset[1] -= dy_screen as f32;

                // Ensure we don't pan outside the grid
                clamp_offset(state);

                state.queue.write_buffer(&state.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
                    zoom: state.zoom,
                    view_offset: state.view_offset,
                    _padding: 0.0,
                }));
            }
        }
        
        state.last_mouse_pos = Some(position);
    } else if state.is_left_mouse_pressed {
        // Left mouse button is pressed and moving = drag action
        if let Some(start_pos) = state.drag_start_pos {
            let dx = position.x - start_pos.x;
            let dy = position.y - start_pos.y;
            let distance_squared = dx * dx + dy * dy;
            
            // Check if we've moved enough to consider this a drag
            if !state.is_dragging && distance_squared > DRAG_THRESHOLD * DRAG_THRESHOLD {
                state.is_dragging = true;
                log::info!("Left drag started");
            }
            
            // If dragging, calculate speed and apply action with speed factor
            if state.is_dragging {
                if let Some(last_pos) = state.last_mouse_pos {
                    let dx = position.x - last_pos.x;
                    let dy = position.y - last_pos.y;
                    let distance = (dx * dx + dy * dy).sqrt();
                    
                    // Calculate time since last action
                    let now = std::time::Instant::now();
                    let elapsed = if let Some(last_time) = state.last_action_time {
                        now.duration_since(last_time).as_secs_f64()
                    } else {
                        0.016 // Default to ~60fps timing if no last time
                    };
                    
                    // Speed in pixels per second
                    let speed = if elapsed > 0.0 { distance / elapsed } else { 0.0 };
                    
                    // Apply action with speed factor
                    apply_cursor_mode_action(state, position, true);
                    
                    // Update last action time
                    state.last_action_time = Some(now);
                }
            }
        }
        
        state.last_mouse_pos = Some(position);
    } else {
        state.last_mouse_pos = None;
    }
}

/// Apply the appropriate action based on the current cursor mode
fn apply_cursor_mode_action(state: &mut State, position: PhysicalPosition<f64>, is_dragging: bool) {
    use crate::state::CursorMode;
    
    // Calculate speed if we're dragging
    let mut drag_speed = 0.0;
    if is_dragging {
        if let (Some(last_pos), Some(last_time)) = (state.last_mouse_pos, state.last_action_time) {
            let dx = position.x - last_pos.x;
            let dy = position.y - last_pos.y;
            let distance = (dx * dx + dy * dy).sqrt();
            
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_time).as_secs_f64();
            
            // Speed in pixels per second
            drag_speed = if elapsed > 0.0 { distance / elapsed } else { 0.0 };
        }
    }
    
    // Always perform action on click (not dragging)
    if !is_dragging {
        perform_action(state, position, state.cursor_mode);
        return;
    }
    
    // Get timings based on the tool
    let now = std::time::Instant::now();
    
    // Get the appropriate timing field and rates based on cursor mode
    let should_perform = match state.cursor_mode {
        CursorMode::Paint => {
            if let Some(last_time) = state.last_paint_time {
                calculate_should_perform(last_time, now, drag_speed)
            } else {
                true
            }
        },
        CursorMode::PlaceGlider => {
            if let Some(last_time) = state.last_glider_time {
                calculate_should_perform(last_time, now, drag_speed)
            } else {
                true
            }
        },
        CursorMode::ClearArea => {
            if let Some(last_time) = state.last_clear_time {
                calculate_should_perform(last_time, now, drag_speed)
            } else {
                true
            }
        },
        CursorMode::RandomFill => {
            if let Some(last_time) = state.last_random_time {
                calculate_should_perform(last_time, now, drag_speed)
            } else {
                true
            }
        },
    };
    
    if should_perform {
        perform_action(state, position, state.cursor_mode);
        
        // Update the appropriate timing field
        match state.cursor_mode {
            CursorMode::Paint => state.last_paint_time = Some(now),
            CursorMode::PlaceGlider => state.last_glider_time = Some(now),
            CursorMode::ClearArea => state.last_clear_time = Some(now),
            CursorMode::RandomFill => state.last_random_time = Some(now),
        }
        
        // Log speed and action for debugging
        if drag_speed > 100.0 {
            log::info!("Action at speed: {:.1} pixels/sec, mode: {:?}", drag_speed, state.cursor_mode);
        }
    }
}

/// Helper function to calculate if an action should be performed based on speed and timing
fn calculate_should_perform(last_time: std::time::Instant, now: std::time::Instant, drag_speed: f64) -> bool {
    // Min and max spawn rates
    let min_spawn_rate = 5.0;
    let max_spawn_rate = 1000.0;
    
    // Min delay at max spawn rate, max delay at min spawn rate
    let min_delay = 1.0 / max_spawn_rate; // e.g., 0.001s for 1000/sec
    let max_delay = 1.0 / min_spawn_rate; // e.g., 0.2s for 5/sec
    
    // Map speed to delay (higher speed = lower delay)
    // Speed threshold: below 10 pixels/sec = min rate, above 500 = max rate
    let delay = if drag_speed <= 10.0 {
        max_delay
    } else if drag_speed >= 500.0 {
        min_delay
    } else {
        // Linear interpolation between max and min delay
        let speed_factor = (drag_speed - 10.0) / 490.0;
        max_delay - (speed_factor * (max_delay - min_delay))
    };
    
    // Check if enough time has passed since last action
    now.duration_since(last_time).as_secs_f64() >= delay
}

/// Perform the actual action for the given cursor mode
fn perform_action(state: &mut State, position: PhysicalPosition<f64>, mode: crate::state::CursorMode) {
    use crate::state::CursorMode;
    
    match mode {
        CursorMode::Paint => {
            state.paint_cell(position);
        },
        CursorMode::PlaceGlider => {
            state.place_glider(position);
        },
        CursorMode::ClearArea => {
            state.clear_area(position, 15);
        },
        CursorMode::RandomFill => {
            state.random_fill(position, 20, 0.4);
        },
    }
}

pub fn handle_cursor_left(state: &mut State) {
    state.cursor_pos = None;
    // Don't reset is_right_mouse_pressed here, only reset last_mouse_pos
    // This allows dragging to continue even if cursor momentarily leaves and re-enters
    state.last_mouse_pos = None;
}

// Clamp view_offset so the visible area never moves outside the grid
fn clamp_offset(state: &mut State) {
    let max_x = (state.grid_width as f32 * state.zoom) - state.size.width as f32;
    let max_y = (state.grid_height as f32 * state.zoom) - state.size.height as f32;

    // If the grid is smaller than the window along an axis, limit stays 0
    let max_x = max_x.max(0.0);
    let max_y = max_y.max(0.0);

    state.view_offset[0] = state.view_offset[0].clamp(0.0, max_x);
    state.view_offset[1] = state.view_offset[1].clamp(0.0, max_y);
}

// Set zoom to an exact value
pub fn set_exact_zoom(state: &mut State, new_zoom: f32) {
    let old_zoom = state.zoom;
    
    // Clamp to valid zoom range
    let new_zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    
    if (new_zoom - old_zoom).abs() < f32::EPSILON {
        return; // No change needed
    }
    
    // Focus zoom on center of screen
    let center_x = state.size.width as f32 / 2.0;
    let center_y = state.size.height as f32 / 2.0;
    
    // Calculate world coordinate at center BEFORE zoom
    let world_x = (center_x + state.view_offset[0]) / old_zoom;
    let world_y = (center_y + state.view_offset[1]) / old_zoom;
    
    // Calculate the required offset AFTER zoom to keep the world point at center
    state.view_offset[0] = world_x * new_zoom - center_x;
    state.view_offset[1] = world_y * new_zoom - center_y;
    
    state.zoom = new_zoom;
    clamp_offset(state);
    
    log::info!("Zoom set to exactly: {:.2}, Offset: [{:.1}, {:.1}]", 
               state.zoom, state.view_offset[0], state.view_offset[1]);
    
    state.queue.write_buffer(&state.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
        zoom: state.zoom,
        view_offset: state.view_offset,
        _padding: 0.0,
    }));
} 