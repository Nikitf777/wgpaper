use crate::transition::TransitionProgress;
use wgpu::{
	BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry,
	BindingType, Buffer, BufferAddress, BufferBindingType, BufferDescriptor, BufferSize,
	BufferUsages, Device, Queue, ShaderStages,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PerFrameDataUniform {
	virtual_screen_size: [f32; 2],
	screen_size: [f32; 2],
	texture_size: [f32; 2],
	virtual_screen_aspect: f32,
	screen_aspect: f32,
	texture_aspect: f32,
	// Two separate f32s (not [f32; 2] / Vec2) so this struct matches
	// the WGSL / SPIR-V ABI at the byte level without alignment gaps.
	progress_bezier: f32,
	progress_linear: f32,
	// Explicit padding: WGSL var<uniform> requires vec4 align 16,
	// so bg_color must start at offset 48, not 44.
	_pad_to_bg_color: [u8; 4],
	bg_color: [f32; 4],
	_padding: [u32; 53],
}

impl PerFrameDataUniform {
	fn new(
		global_screen_size: (f32, f32),
		screen_size: (f32, f32),
		texture_size: (f32, f32),
		progress: TransitionProgress,
		bg_color: wgpaper_config::Color,
	) -> Self {
		Self {
			virtual_screen_size: [global_screen_size.0, global_screen_size.1],
			screen_size: [screen_size.0, screen_size.1],
			texture_size: [texture_size.0, texture_size.1],
			virtual_screen_aspect: global_screen_size.0 / global_screen_size.1,
			screen_aspect: screen_size.0 / screen_size.1,
			texture_aspect: texture_size.0 / texture_size.1,
			progress_bezier: progress.progress_bezier,
			progress_linear: progress.progress_linear,
			_pad_to_bg_color: [0u8; 4],
			bg_color: unsafe { std::mem::transmute(bg_color) },
			_padding: [0u32; 53],
		}
	}

	fn transition_progress(&self) -> TransitionProgress {
		TransitionProgress {
			progress_bezier: self.progress_bezier,
			progress_linear: self.progress_linear,
		}
	}

	fn update_screen_size(&mut self, new_size: (f32, f32)) {
		self.screen_size = [new_size.0, new_size.1];
		self.screen_aspect = new_size.0 / new_size.1;
	}

	fn update_texture_size(&mut self, new_size: (f32, f32)) {
		self.texture_size = [new_size.0, new_size.1];
		self.texture_aspect = new_size.0 / new_size.1;
	}

	fn update_transition_progress(&mut self, new_progress: TransitionProgress) {
		self.progress_bezier = new_progress.progress_bezier;
		self.progress_linear = new_progress.progress_linear;
	}
}

fn write_per_frame_data(data: &PerFrameDataUniform, queue: &Queue, buffer: &Buffer) {
	queue.write_buffer(&buffer, 0, bytemuck::bytes_of(data));
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
				min_binding_size: BufferSize::new(256),
			},
			count: None,
		}],
		label: Some("per_frame_data_bind_group_layout"),
	})
}

pub struct PerFrameUniformManager {
	data: PerFrameDataUniform,
	buffer: Buffer,
	bind_group: BindGroup,
}

impl PerFrameUniformManager {
	/// Create a `PerFrameUniformManager` and return it together with the
	/// newly created bind-group layout.
	///
	/// Prefer [`with_layout`](Self::with_layout) when the layout is already
	/// available (e.g. from a shared device-level cache).
	pub fn new(
		device: &wgpu::Device,
		screen_size: (f32, f32),
		texture_size: (f32, f32),
		bg_color: wgpaper_config::Color,
	) -> (Self, BindGroupLayout) {
		let buffer = device.create_buffer(&BufferDescriptor {
			label: Some("per_frame_data_uniform_buffer"),
			size: std::mem::size_of::<PerFrameDataUniform>() as BufferAddress,
			usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});
		let data = PerFrameDataUniform::new(
			(screen_size.0 as f32, screen_size.1 as f32),
			(screen_size.0 as f32, screen_size.1 as f32),
			(texture_size.0 as f32, texture_size.1 as f32),
			TransitionProgress::reset(),
			bg_color,
		);

		let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			entries: &[BindGroupLayoutEntry {
				binding: 0,
				visibility: ShaderStages::FRAGMENT,
				ty: BindingType::Buffer {
					ty: BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size: BufferSize::new(256),
				},
				count: None,
			}],
			label: Some("per_frame_data_bind_group_layout"),
		});

		let bind_group = device.create_bind_group(&BindGroupDescriptor {
			layout: &bind_group_layout,
			entries: &[BindGroupEntry {
				binding: 0,
				resource: buffer.as_entire_binding(),
			}],
			label: Some("per_frame_data_bind_group"),
		});

		(
			Self {
				data,
				buffer,
				bind_group,
			},
			bind_group_layout,
		)
	}

	/// Create a `PerFrameUniformManager` reusing an existing layout.
	///
	/// This avoids creating a duplicate bind-group layout when the layout is
	/// already shared at the device level.
	pub fn with_layout(
		device: &wgpu::Device,
		bind_group_layout: &BindGroupLayout,
		screen_size: (f32, f32),
		texture_size: (f32, f32),
		bg_color: wgpaper_config::Color,
	) -> Self {
		let buffer = device.create_buffer(&BufferDescriptor {
			label: Some("per_frame_data_uniform_buffer"),
			size: std::mem::size_of::<PerFrameDataUniform>() as BufferAddress,
			usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});
		let data = PerFrameDataUniform::new(
			(screen_size.0 as f32, screen_size.1 as f32),
			(screen_size.0 as f32, screen_size.1 as f32),
			(texture_size.0 as f32, texture_size.1 as f32),
			TransitionProgress::reset(),
			bg_color,
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

	pub fn write_data(&self, queue: &Queue) {
		write_per_frame_data(&self.data, queue, &self.buffer);
	}

	pub fn transition_progress(&self) -> TransitionProgress {
		self.data.transition_progress()
	}

	pub fn bind_group(&self) -> &BindGroup {
		&self.bind_group
	}

	pub fn update_screen_size(&mut self, new_size: (f32, f32)) {
		self.data.update_screen_size(new_size);
	}

	pub fn update_texture_size(&mut self, new_size: (f32, f32)) {
		self.data.update_texture_size(new_size);
	}

	pub fn update_transition_progress(&mut self, new_progress: TransitionProgress) {
		self.data.update_transition_progress(new_progress);
	}
}
