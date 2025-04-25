// Declare modules directly in the binary crate root
pub mod state;
pub mod compute;
pub mod render;
pub mod input;
pub mod rules;

// Use types/functions from the declared modules
use crate::state::State;

use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent, MouseScrollDelta, MouseButton, ElementState},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use bytemuck::{Pod, Zeroable};
use std::num::NonZeroU64;

// Constants
const GRID_WIDTH: u32 = 256;
const GRID_HEIGHT: u32 = 256;

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
                    input::handle_mouse_input(&mut state, button, element_state);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    input::handle_cursor_move(&mut state, position);
                }
                WindowEvent::CursorLeft { .. } => {
                    input::handle_cursor_left(&mut state);
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => {
                            if pos.y.abs() > 0.0 {
                                (pos.y / 20.0) as f32 // Arbitrary scaling
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