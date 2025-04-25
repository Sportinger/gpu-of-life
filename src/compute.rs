use bytemuck::{Pod, Zeroable};
use wgpu;
use std::num::NonZeroU64; // Needed for NonZeroU64
use crate::rules::GameRules as RustGameRules;

pub const WORKGROUP_SIZE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SimParams {
    pub width: u32,
    pub height: u32,
}

/// Shader-compatible representation of GameRules
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ShaderGameRules {
    pub survival_min: u32,
    pub survival_max: u32,
    pub birth_count: u32,
    pub _padding: u32, // Ensure 16-byte alignment
}

impl From<&RustGameRules> for ShaderGameRules {
    fn from(rules: &RustGameRules) -> Self {
        Self {
            survival_min: rules.survival_min,
            survival_max: rules.survival_max,
            birth_count: rules.birth_count,
            _padding: 0, // Required for memory alignment
        }
    }
}

// Helper function to create compute bind groups
pub fn create_compute_bind_groups(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    grid_buffers: &[wgpu::Buffer; 2],
    sim_param_buffer: &wgpu::Buffer,
    rules_buffer: &wgpu::Buffer
) -> [wgpu::BindGroup; 2] {
    [
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group 0"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[0].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: grid_buffers[1].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: rules_buffer.as_entire_binding() },
            ],
        }),
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group 1"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: sim_param_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: grid_buffers[1].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: grid_buffers[0].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: rules_buffer.as_entire_binding() },
            ],
        }),
    ]
} 