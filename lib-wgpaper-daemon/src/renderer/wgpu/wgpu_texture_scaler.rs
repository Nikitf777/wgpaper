use wgpaper_config::ScalingMode;
use wgpu::{
	BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
	BindingResource, BindingType, BlendState, ColorTargetState, ColorWrites, CommandEncoder,
	Device, MultisampleState, PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPipeline,
	Sampler, SamplerBindingType, ShaderModule, ShaderStages, TextureFormat, TextureSampleType,
	TextureView, TextureViewDimension,
};

use crate::renderer::wgpu::{
	wgpu_shaders::{self, create_scaling_fragment_shader},
	wgpu_utilities::{self, begin_render_pass, create_color_attachment, create_command_encoder},
};

pub struct WgpuTextureScaler {
	texture_bind_group_layout: BindGroupLayout,
	pipeline: RenderPipeline,
}

impl WgpuTextureScaler {
	pub fn new(
		device: &Device,
		per_frame_data_bind_group_layout: &BindGroupLayout,
		vertex_shader: &ShaderModule,
		scaling_mode: ScalingMode,
		format: TextureFormat,
	) -> Self {
		let layout =
			device.create_bind_group_layout(&BindGroupLayoutDescriptor {
				entries: &[
					BindGroupLayoutEntry {
						binding: 0,
						visibility: ShaderStages::FRAGMENT,
						ty: BindingType::Texture {
							sample_type: TextureSampleType::Float { filterable: true },
							view_dimension: TextureViewDimension::D2,
							multisampled: false,
						},
						count: None,
					},
					wgpu::BindGroupLayoutEntry {
						binding: 1,
						visibility: ShaderStages::FRAGMENT,
						ty: BindingType::Sampler(SamplerBindingType::Filtering),
						count: None,
					},
				],
				label: Some("scaling_bind_group_layout"),
			});

		let scaling_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
			label: Some("scaling_pipeline_layout"),
			bind_group_layouts: &[
				&layout,
				&per_frame_data_bind_group_layout,
			],
			immediate_size: 0,
		});

		let frag = create_scaling_fragment_shader(&device, &scaling_mode);

		let scaling_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("scaling_pipeline"),
			layout: Some(&scaling_pipeline_layout),
			vertex: wgpu::VertexState {
				module: &vertex_shader,
				entry_point: Some(wgpu_shaders::VS_ENTRY),
				buffers: &[],
				compilation_options: Default::default(),
			},
			fragment: Some(wgpu::FragmentState {
				module: &frag.module,
				entry_point: Some(frag.entry_point),
				targets: &[Some(ColorTargetState {
					format,
					blend: Some(BlendState::REPLACE),
					write_mask: ColorWrites::ALL,
				})],
				compilation_options: Default::default(),
			}),
			primitive: PrimitiveState::default(),
			depth_stencil: None,
			multisample: MultisampleState::default(),
			multiview_mask: None,
			cache: None,
		});

		Self {
			texture_bind_group_layout: layout,
			pipeline: scaling_pipeline,
		}
	}

	fn render_scale(
		&self,
		encoder: &mut CommandEncoder,
		texture_bind_group: &BindGroup,
		dist_view: &TextureView,
		per_frame_data_bind_group: &BindGroup,
	) {
		let mut render_pass = begin_render_pass(
			encoder,
			create_color_attachment(dist_view),
			"scaling_render_pass",
		);

		wgpu_utilities::render_pass(
			&mut render_pass,
			&self.pipeline,
			&texture_bind_group,
			per_frame_data_bind_group,
		);
	}

	pub fn scale<'tex>(
		&self,
		device: &Device,
		queue: &Queue,
		sampler: &Sampler,
		scr_view: &TextureView,
		dist_view: &TextureView,
		per_frame_data_bind_group: &BindGroup,
	) {
		let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &self.texture_bind_group_layout,
			entries: &[
				BindGroupEntry {
					binding: 0,
					resource: BindingResource::TextureView(scr_view),
				},
				BindGroupEntry {
					binding: 1,
					resource: BindingResource::Sampler(&sampler),
				},
			],
			label: Some("scaling_bind_group"),
		});

		let mut encoder = create_command_encoder(device, "scaling_command_encoder");

		self.render_scale(
			&mut encoder,
			&texture_bind_group,
			dist_view,
			per_frame_data_bind_group,
		);

		queue.submit(Some(encoder.finish()));
	}
}
