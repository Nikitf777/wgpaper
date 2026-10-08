//! Host-side ownership of the shared uniform block.
//!
//! The block itself is declared once, in `wgpaper-abi`. This module owns the
//! GPU-side buffer and the bind group, and forwards every mutation to the ABI
//! type so the host can never disagree with the shaders about the layout.

use crate::transition::TransitionProgress;
use wgpaper_abi::{PerFrameDataUniform, Vec2, Vec4};
use wgpu::{
	BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry,
	BindingType, Buffer, BufferAddress, BufferBindingType, BufferDescriptor, BufferSize,
	BufferUsages, Device, Queue, ShaderStages,
};

fn write_per_frame_data(data: &PerFrameDataUniform, queue: &Queue, buffer: &Buffer) {
	queue.write_buffer(buffer, 0, bytemuck::bytes_of(data));
}

/// Create the bind-group layout for per-frame uniforms (no device reference captured).
pub fn per_frame_bind_group_layout(device: &Device) -> BindGroupLayout {
	device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
		entries: &[BindGroupLayoutEntry {
			binding: 0,
			visibility: ShaderStages::FRAGMENT,
			ty: BindingType::Buffer {
				ty: BufferBindingType::Uniform,
				has_dynamic_offset: false,
				// Sourced from the ABI crate, so the minimum a shader can
				// declare always matches the struct the host writes.
				min_binding_size: BufferSize::new(PerFrameDataUniform::SIZE as BufferAddress),
			},
			count: None,
		}],
		label: Some("per_frame_data_bind_group_layout"),
	})
}

fn create_uniform_buffer(device: &Device) -> Buffer {
	device.create_buffer(&BufferDescriptor {
		label: Some("per_frame_data_uniform_buffer"),
		size: PerFrameDataUniform::SIZE as BufferAddress,
		usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
		mapped_at_creation: false,
	})
}

/// Owns one surface's uniform buffer and bind group.
///
/// The struct is *not* padded out to the old 256-byte minimum binding size.
/// The block is 64 bytes and is now allocated at exactly its own size, with
/// `min_binding_size` derived from the same constant, so the two can no longer
/// drift apart.
pub struct PerFrameUniformManager {
	data: PerFrameDataUniform,
	buffer: Buffer,
	bind_group: BindGroup,
}

impl PerFrameUniformManager {
	/// Create a `PerFrameUniformManager`, creating its own bind-group layout.
	///
	/// Prefer [`with_layout`](Self::with_layout) when the layout is already
	/// available (e.g. from a shared device-level cache).
	#[allow(dead_code)]
	pub fn new(
		device: &wgpu::Device,
		screen_size: Vec2,
		texture_size: Vec2,
		bg_color: Vec4,
	) -> (Self, BindGroupLayout) {
		let bind_group_layout = per_frame_bind_group_layout(device);
		let manager = Self::with_layout(
			device,
			&bind_group_layout,
			screen_size,
			texture_size,
			bg_color,
		);
		(manager, bind_group_layout)
	}

	/// Create a `PerFrameUniformManager` reusing an existing layout.
	///
	/// This avoids creating a duplicate bind-group layout when the layout is
	/// already shared at the device level.
	pub fn with_layout(
		device: &wgpu::Device,
		bind_group_layout: &BindGroupLayout,
		screen_size: Vec2,
		texture_size: Vec2,
		bg_color: Vec4,
	) -> Self {
		let buffer = create_uniform_buffer(device);

		// `virtual_screen_size` is currently the same value as `screen_size`:
		// the single-canvas mode that would distinguish them is not
		// implemented. See the field's docs in `wgpaper-abi`.
		let data = PerFrameDataUniform::new(
			screen_size,
			screen_size,
			texture_size,
			bg_color,
			TransitionProgress::reset(),
		);

		let bind_group = device.create_bind_group(&BindGroupDescriptor {
			layout: bind_group_layout,
			entries: &[BindGroupEntry {
				binding: 0,
				resource: buffer.as_entire_binding(),
			}],
			label: Some("per_frame_data_bind_group"),
		});

		Self {
			data,
			buffer,
			bind_group,
		}
	}

	/// Push the in-memory block to the GPU.
	///
	/// This is the only thing that makes a mutation visible to a shader, so it
	/// must follow every `update_*` call.
	pub fn write_data(&self, queue: &Queue) {
		write_per_frame_data(&self.data, queue, &self.buffer);
	}

	pub fn bind_group(&self) -> &BindGroup {
		&self.bind_group
	}

	pub fn update_screen_size(&mut self, new_size: Vec2) {
		self.data.update_screen_size(new_size);
	}

	pub fn update_texture_size(&mut self, new_size: Vec2) {
		self.data.update_texture_size(new_size);
	}

	pub fn update_transition_progress(&mut self, new_progress: TransitionProgress) {
		self.data.update_transition_progress(new_progress);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The old code declared `min_binding_size: 256` and carried 53 `u32`s of
	/// trailing padding to satisfy it. The block is only 64 bytes, and the
	/// layout now derives its minimum from the struct, so nothing needs to
	/// reconcile a magic number any more.
	#[test]
	fn binding_size_is_derived_from_the_block() {
		assert_eq!(PerFrameDataUniform::SIZE, 64);
		assert_eq!(
			BufferSize::new(PerFrameDataUniform::SIZE as BufferAddress),
			BufferSize::new(64)
		);
	}
}
