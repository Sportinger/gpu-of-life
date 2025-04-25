use bytemuck::{Pod, Zeroable};
use wgpu;
use std::num::NonZeroU64;
use crate::compute::SimParams; // Need SimParams for layout definition

pub const MIN_ZOOM: f32 = 1.0; // Min zoom is 1:1 pixel mapping
pub const MAX_ZOOM: f32 = 16.0; // Max zoom factor
pub const ZOOM_FACTOR_STEP: f32 = 1.2; // How much each wheel step zooms

// Uniforms specific to rendering
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct RenderParams {
    pub zoom: f32,
    pub view_offset: [f32; 2],
    pub _padding: f32, // Ensure 16-byte alignment (f32 + vec2<f32> + f32 = 4 + 8 + 4 = 16)
}

pub fn create_render_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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
            // RenderParams Uniform (Binding 2)
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

pub fn create_render_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    grid_buffers: &[wgpu::Buffer; 2],
    sim_param_buffer: &wgpu::Buffer,
    render_param_buffer: &wgpu::Buffer
) -> [wgpu::BindGroup; 2] {
    [
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group 0"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[0].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: render_param_buffer.as_entire_binding() },
            ],
        }),
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Render Bind Group 1"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[1].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: render_param_buffer.as_entire_binding() },
            ],
        }),
    ]
} 