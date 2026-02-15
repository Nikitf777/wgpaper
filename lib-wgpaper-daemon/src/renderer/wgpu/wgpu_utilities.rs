use wgpaper_config::{Background, ScalingMode};
use wgpu::{
	AddressMode, CommandEncoder, LoadOp, Operations, RenderPass, RenderPassColorAttachment,
	RenderPassDescriptor, StoreOp, TextureView,
};

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
