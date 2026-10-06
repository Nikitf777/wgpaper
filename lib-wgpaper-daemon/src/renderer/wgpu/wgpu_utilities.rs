use std::ptr::NonNull;

use anyhow::Context;
use raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use wayland_client::{Connection, Proxy};
use wgpaper_config::{Background, ScalingMode};
use wgpu::{
	AddressMode, BindGroup, Color, CommandEncoder, CommandEncoderDescriptor, Device, FilterMode,
	Instance, LoadOp, MipmapFilterMode, Operations, Queue, RenderPass, RenderPassColorAttachment,
	RenderPassDescriptor, RenderPipeline, Sampler, SamplerDescriptor, StoreOp, Surface,
	SurfaceTargetUnsafe, TextureView,
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
			.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
				raw_display_handle,
				raw_window_handle,
			})
			.context("Failed to create surface")?
	})
}

/// Colour painted behind the wallpaper for the modes that letterbox it
/// (`Fit` / `Center`).  Modes that fill the screen ignore it.
pub fn bg_color_for(scaling_mode: &ScalingMode) -> csscolorparser::Color {
	match scaling_mode {
		ScalingMode::Fit { background } | ScalingMode::Center { background } => {
			if let Background::CssColor(color) = background {
				color.clone()
			} else {
				csscolorparser::Color::default()
			}
		}
		ScalingMode::Stretch | ScalingMode::Cover => csscolorparser::Color::default(),
	}
}

/// Give a freshly allocated texture defined contents without any CPU-side
/// staging buffer or upload, so it can immediately be sampled from or used as
/// a render target.
pub fn clear_view(device: &Device, queue: &Queue, view: &TextureView, color: Color) {
	let mut encoder = create_command_encoder(device, "clear_command_encoder");
	encoder.begin_render_pass(&RenderPassDescriptor {
		label: Some("clear_render_pass"),
		color_attachments: &[Some(RenderPassColorAttachment {
			view,
			ops: Operations {
				load: LoadOp::Clear(color),
				store: StoreOp::Store,
			},
			resolve_target: None,
			depth_slice: None,
		})],
		..Default::default()
	});
	queue.submit(Some(encoder.finish()));
}

pub fn create_sampler(device: &Device, address_mode: AddressMode) -> Sampler {
	device.create_sampler(&SamplerDescriptor {
		label: Some("sampler"),
		address_mode_u: address_mode,
		address_mode_v: address_mode,
		mag_filter: FilterMode::Linear,
		min_filter: FilterMode::Linear,
		mipmap_filter: MipmapFilterMode::Nearest,
		..Default::default()
	})
}

pub fn create_command_encoder(device: &Device, label: &str) -> CommandEncoder {
	device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) })
}

pub fn create_color_attachment<'tex>(view: &'tex TextureView) -> RenderPassColorAttachment<'tex> {
	RenderPassColorAttachment {
		view,
		ops: Operations {
			load: LoadOp::Clear(Color {
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
	render_pass.set_pipeline(pipeline);
	render_pass.set_bind_group(0, texture_bind_group, &[]);
	render_pass.set_bind_group(1, per_frame_data_bind_group, &[]);
	render_pass.draw(0..3, 0..1);
}
