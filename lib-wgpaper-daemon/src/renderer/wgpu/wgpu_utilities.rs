use wgpu::{CommandEncoder, RenderPass, RenderPassColorAttachment, TextureView};

pub(super) fn create_color_attachment<'tex>(
	view: &'tex TextureView,
) -> RenderPassColorAttachment<'tex> {
	RenderPassColorAttachment {
		view: view,
		ops: wgpu::Operations {
			load: wgpu::LoadOp::Clear(wgpu::Color {
				r: 0.1,
				g: 0.2,
				b: 0.3,
				a: 1.0,
			}),
			store: wgpu::StoreOp::Store,
		},
		resolve_target: None,
		depth_slice: None,
	}
}

pub(super) fn begin_render_pass<'tex>(
	encoder: &'tex mut CommandEncoder,
	color_attachment: RenderPassColorAttachment<'tex>,
	label: &str,
) -> RenderPass<'tex> {
	encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
		label: Some(label),
		color_attachments: &[Some(color_attachment)],
		..Default::default()
	})
}
