use crate::state::State;
use crate::render::{RenderParams, MIN_ZOOM, MAX_ZOOM, ZOOM_FACTOR_STEP};
use winit::{
    dpi::PhysicalPosition,
    event::{MouseButton, ElementState},
};

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

        // Adjust offset based on cursor position to zoom towards the cursor
        // V_new = V_old + (factor - 1) * (C + V_old / Z_old)
        // Note: Screen coords C are relative to window, view_offset V is grid coords.
        // Convert screen coords to "world" coords (relative to grid origin at current zoom)
        let world_x = (cursor_screen_x - state.size.width as f32 / 2.0) / old_zoom + state.view_offset[0];
        let world_y = (cursor_screen_y - state.size.height as f32 / 2.0) / old_zoom + state.view_offset[1];

        // Calculate where the new view offset should be to keep the world point under the cursor
        new_offset[0] = world_x - (cursor_screen_x - state.size.width as f32 / 2.0) / new_zoom;
        new_offset[1] = world_y - (cursor_screen_y - state.size.height as f32 / 2.0) / new_zoom;

    } // If cursor is outside window, zoom towards center (no offset change needed)

    // Clamp offset if zoom gets very close to MIN_ZOOM
    if (new_zoom - MIN_ZOOM).abs() < f32::EPSILON {
         new_offset = [0.0, 0.0];
    }

    state.zoom = new_zoom;
    state.view_offset = new_offset;

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
        state.is_right_mouse_pressed = is_pressed;
        if !is_pressed {
            state.last_mouse_pos = None;
        }
    }
}

pub fn handle_cursor_move(state: &mut State, position: PhysicalPosition<f64>) {
    state.cursor_pos = Some(position);

    if state.is_right_mouse_pressed {
        if let Some(last_pos) = state.last_mouse_pos {
            // Calculate delta in screen coordinates
            let dx_screen = position.x - last_pos.x;
            let dy_screen = position.y - last_pos.y;

            // Convert screen delta to grid delta based on current zoom
            // Panning should feel like dragging the grid, so movement is inversely proportional to zoom.
            let dx_grid = dx_screen / state.zoom as f64;
            let dy_grid = dy_screen / state.zoom as f64;

            // Map mouse movement to view offset in a way that vertical drag pans vertically and horizontal drag pans horizontally.
            // Horizontal drag updates x offset (index 0)
            state.view_offset[0] -= dx_grid as f32;

            // Vertical drag updates y offset (index 1)
            state.view_offset[1] -= dy_grid as f32;

            // Clamp offset (optional, maybe based on grid boundaries if needed)
            // let max_offset_x = (state.grid_width as f32 * (1.0 - 1.0 / state.zoom)) / 2.0;
            // let max_offset_y = (state.grid_height as f32 * (1.0 - 1.0 / state.zoom)) / 2.0;
            // state.view_offset[0] = state.view_offset[0].clamp(-max_offset_x, max_offset_x);
            // state.view_offset[1] = state.view_offset[1].clamp(-max_offset_y, max_offset_y);


            state.queue.write_buffer(&state.render_param_buffer, 0, bytemuck::bytes_of(&RenderParams {
                zoom: state.zoom,
                view_offset: state.view_offset,
                _padding: 0.0,
            }));

        }
        state.last_mouse_pos = Some(position);
    } else {
        state.last_mouse_pos = None;
    }
}

pub fn handle_cursor_left(state: &mut State) {
    state.cursor_pos = None;
    // Don't reset is_right_mouse_pressed here, only reset last_mouse_pos
    // This allows dragging to continue even if cursor momentarily leaves and re-enters
    state.last_mouse_pos = None;
} 