use wgpu::{
	BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingResource, BindingType,
	CommandEncoder, Device, RenderPipeline, Sampler, SamplerBindingType, ShaderModule,
	ShaderStages, TextureFormat, TextureSampleType, TextureView, TextureViewDimension,
};

use crate::renderer::wgpu::{
	wgpu_shaders,
	wgpu_utilities::{self, begin_render_pass, create_color_attachment},
};

fn create_texture_bind_group(
	device: &Device,
	layout: &BindGroupLayout,
	prev_view: &TextureView,
	target_view: &TextureView,
	sampler: &Sampler,
) -> BindGroup {
	device.create_bind_group(&wgpu::BindGroupDescriptor {
		layout: &layout,
		entries: &[
			BindGroupEntry {
				binding: 0,
				resource: BindingResource::TextureView(prev_view),
			},
			BindGroupEntry {
				binding: 1,
				resource: BindingResource::TextureView(target_view),
			},
			BindGroupEntry {
				binding: 2,
				resource: BindingResource::Sampler(&sampler),
			},
		],
		label: Some("transition_texture_bind_group"),
	})
}

pub struct WgpuTransitionRenderer {
	texture_bind_group_layout: BindGroupLayout,
	texture_bind_group: BindGroup,
	pipeline: RenderPipeline,
}

impl WgpuTransitionRenderer {
	pub fn new(
		device: &Device,
		sampler: &Sampler,
		initial_view: &TextureView,
		next_view: &TextureView,
		per_frame_data_bind_group_layout: &BindGroupLayout,
		vertex_shader: &ShaderModule,
		transition_shader: &ShaderModule,
		fragment_entry: &str,
		format: TextureFormat,
	) -> Self {
		let texture_bind_group_layout =
			device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
				entries: &[
					BindGroupLayoutEntry {
						binding: 0,
						visibility: ShaderStages::FRAGMENT,
						ty: BindingType::Texture {
							multisampled: false,
							sample_type: TextureSampleType::Float { filterable: true },
							view_dimension: TextureViewDimension::D2,
						},
						count: None,
					},
					BindGroupLayoutEntry {
						binding: 1,
						visibility: ShaderStages::FRAGMENT,
						ty: BindingType::Texture {
							multisampled: false,
							sample_type: TextureSampleType::Float { filterable: true },
							view_dimension: TextureViewDimension::D2,
						},
						count: None,
					},
					BindGroupLayoutEntry {
						binding: 2,
						visibility: ShaderStages::FRAGMENT,
						ty: BindingType::Sampler(SamplerBindingType::Filtering),
						count: None,
					},
				],
				label: Some("transition_texture_bind_group_layout"),
			});

		let texture_bind_group = create_texture_bind_group(
			device,
			&texture_bind_group_layout,
			initial_view,
			next_view,
			sampler,
		);

		let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
			label: Some("transition_pipeline_layout"),
			bind_group_layouts: &[
				&texture_bind_group_layout,
				&per_frame_data_bind_group_layout,
			],
			immediate_size: 0,
		});

		let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("transition_pipeline"),
			layout: Some(&pipeline_layout),
			vertex: wgpu::VertexState {
				module: &vertex_shader,
				entry_point: Some(wgpu_shaders::VS_ENTRY),
				buffers: &[],
				compilation_options: Default::default(),
			},
			fragment: Some(wgpu::FragmentState {
				module: &transition_shader,
				entry_point: Some(fragment_entry),
				targets: &[Some(wgpu::ColorTargetState {
					format,
					blend: Some(wgpu::BlendState::REPLACE),
					write_mask: wgpu::ColorWrites::ALL,
				})],
				compilation_options: Default::default(),
			}),
			primitive: wgpu::PrimitiveState::default(),
			depth_stencil: None,
			multisample: wgpu::MultisampleState::default(),
			multiview_mask: None,
			cache: None,
		});

		Self {
			texture_bind_group_layout,
			texture_bind_group,
			pipeline,
		}
	}

	pub fn update_textures(
		&mut self,
		device: &Device,
		prev_view: &TextureView,
		target_view: &TextureView,
		sampler: &Sampler,
	) {
		self.texture_bind_group = create_texture_bind_group(
			device,
			&self.texture_bind_group_layout,
			prev_view,
			target_view,
			sampler,
		)
	}

	fn render_transition(
		&self,
		encoder: &mut CommandEncoder,
		texture_bind_group: &BindGroup,
		dist_view: &TextureView,
		per_frame_data_bind_group: &BindGroup,
	) {
		let mut render_pass = begin_render_pass(
			encoder,
			create_color_attachment(dist_view),
			"transition_render_pass",
		);

		wgpu_utilities::render_pass(
			&mut render_pass,
			&self.pipeline,
			&texture_bind_group,
			per_frame_data_bind_group,
		);
	}

	pub fn transition<'tex>(
		&self,
		encoder: &mut CommandEncoder,
		dist_view: &TextureView,
		per_frame_data_bind_group: &BindGroup,
	) {
		self.render_transition(
			encoder,
			&self.texture_bind_group,
			dist_view,
			per_frame_data_bind_group,
		);
	}
}
