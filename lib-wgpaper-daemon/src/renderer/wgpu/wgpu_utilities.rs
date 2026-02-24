use std::ptr::NonNull;

use anyhow::Context;
use raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use wayland_client::{Connection, Proxy};
use wgpaper_config::{Background, ScalingMode};
use wgpu::{
	AddressMode, BindGroup, CommandEncoder, Device, Instance, LoadOp, Operations, RenderPass,
	RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, Sampler, StoreOp, Surface,
	TextureView,
};

pub fn create_surface<'a>(
	instance: &Instance,
	connection: &Connection,
	layer_surface: &LayerSurface,
) -> anyhow::Result<Surface<'a>> {
	let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
		NonNull::new(connection.backend().display_ptr() as *mut _).unwrap(),
	));
	let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
		NonNull::new(layer_surface.wl_surface().id().as_ptr() as *mut _).unwrap(),
	));

	Ok(unsafe {
		instance
			.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
				raw_display_handle,
				raw_window_handle,
			})
			.context("Failed to create surface")?
	})
}

pub fn get_address_mode_and_bg_color(
	scaling_mode: &ScalingMode,
) -> (AddressMode, csscolorparser::Color) {
	match scaling_mode {
		ScalingMode::Fit { background } | ScalingMode::Center { background } => {
			if background == &Background::Repeat {
				(AddressMode::Repeat, csscolorparser::Color::default())
			} else {
				(
					AddressMode::MirrorRepeat,
					if let Background::CssColor(color) = background {
						color.clone()
					} else {
						csscolorparser::Color::default()
					},
				)
			}
		}
		ScalingMode::Stretch | ScalingMode::Cover => {
			(AddressMode::MirrorRepeat, csscolorparser::Color::default())
		}
	}
}

pub fn create_sampler(device: &Device, address_mode: AddressMode) -> Sampler {
	device.create_sampler(&wgpu::SamplerDescriptor {
		label: Some("sampler"),
		address_mode_u: address_mode,
		address_mode_v: address_mode,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::MipmapFilterMode::Nearest,
		..Default::default()
	})
}

pub fn create_command_encoder(device: &Device, label: &str) -> CommandEncoder {
	device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) })
}

pub fn create_color_attachment<'tex>(view: &'tex TextureView) -> RenderPassColorAttachment<'tex> {
	RenderPassColorAttachment {
		view: view,
		ops: Operations {
			load: LoadOp::Clear(wgpu::Color {
				r: 0.1,
				g: 0.2,
				b: 0.3,
				a: 1.0,
			}),
			store: StoreOp::Store,
		},
		resolve_target: None,
		depth_slice: None,
	}
}

pub fn begin_render_pass<'tex>(
	encoder: &'tex mut CommandEncoder,
	color_attachment: RenderPassColorAttachment<'tex>,
	label: &str,
) -> RenderPass<'tex> {
	encoder.begin_render_pass(&RenderPassDescriptor {
		label: Some(label),
		color_attachments: &[Some(color_attachment)],
		..Default::default()
	})
}

pub fn render_pass<'tex>(
	render_pass: &mut RenderPass<'tex>,
	pipeline: &RenderPipeline,
	texture_bind_group: &BindGroup,
	per_frame_data_bind_group: &BindGroup,
) {
	render_pass.set_pipeline(&pipeline);
	render_pass.set_bind_group(0, texture_bind_group, &[]);
	render_pass.set_bind_group(1, per_frame_data_bind_group, &[]);
	render_pass.draw(0..3, 0..1);
}
