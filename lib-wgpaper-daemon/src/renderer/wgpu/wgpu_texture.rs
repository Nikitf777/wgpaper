use wgpu::{
	Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
	TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
	TextureViewDescriptor,
};

use crate::image_wrapper::ImageWrapper;

pub struct WgpuTexture {
	#[allow(unused)]
	pub texture: Texture,
	pub view: TextureView,
}

impl WgpuTexture {
	pub fn from_image(
		device: &Device,
		queue: &Queue,
		image: &ImageWrapper,
		label: &str,
		format: TextureFormat,
	) -> anyhow::Result<Self> {
		Self::from_rgba8_with_format(
			device,
			&queue,
			image.dimensions(),
			image.as_slice(),
			label,
			format,
		)
	}

	pub fn from_rgba8_with_format(
		device: &Device,
		queue: &Queue,
		size: (u32, u32),
		rgba: &[u8],
		label: &str,
		format: TextureFormat,
	) -> anyhow::Result<Self> {
		let extend = Extent3d {
			width: size.0,
			height: size.1,
			depth_or_array_layers: 1,
		};

		let texture = device.create_texture(&TextureDescriptor {
			label: Some(label),
			size: extend,
			mip_level_count: 1,
			sample_count: 1,
			dimension: TextureDimension::D2,
			format,
			usage: TextureUsages::RENDER_ATTACHMENT
				| TextureUsages::TEXTURE_BINDING
				| TextureUsages::COPY_SRC
				| TextureUsages::COPY_DST,
			view_formats: &[],
		});

		let (bytes_per_row, upload_data) = match format {
			TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => {
				(Some(4 * size.0), rgba.to_vec())
			}
			TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
				let mut bgra = Vec::with_capacity(rgba.len());
				for chunk in rgba.chunks_exact(4) {
					bgra.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
				}
				(Some(4 * size.0), bgra)
			}
			_ => (Some(4 * size.0), rgba.to_vec()),
		};

		queue.write_texture(
			TexelCopyTextureInfo {
				texture: &texture,
				mip_level: 0,
				origin: Origin3d::ZERO,
				aspect: TextureAspect::All,
			},
			&upload_data,
			TexelCopyBufferLayout {
				offset: 0,
				bytes_per_row,
				rows_per_image: Some(size.1),
			},
			extend,
		);

		let view = texture.create_view(&TextureViewDescriptor::default());
		Ok(Self { texture, view })
	}
}
