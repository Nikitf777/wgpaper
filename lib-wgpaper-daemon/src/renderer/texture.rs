use anyhow::*;
use image::GenericImageView;

pub struct Texture {
	#[allow(unused)]
	pub texture: wgpu::Texture,
	pub view: wgpu::TextureView,
}

impl Texture {
	pub fn from_bytes(
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		bytes: &[u8],
		label: &str,
	) -> Result<Self> {
		let img = image::load_from_memory(bytes)?;
		Self::from_image(device, queue, &img, Some(label))
	}

	pub fn from_image(
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		img: &image::DynamicImage,
		label: Option<&str>,
	) -> Result<Self> {
		let rgba = img.to_rgba8();
		let dimensions = img.dimensions();

		let size = wgpu::Extent3d {
			width: dimensions.0,
			height: dimensions.1,
			depth_or_array_layers: 1,
		};
		let format = wgpu::TextureFormat::Rgba8UnormSrgb;
		let texture = device.create_texture(&wgpu::TextureDescriptor {
			label,
			size,
			mip_level_count: 1,
			sample_count: 1,
			dimension: wgpu::TextureDimension::D2,
			format,
			usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
			view_formats: &[],
		});

		queue.write_texture(
			wgpu::TexelCopyTextureInfo {
				aspect: wgpu::TextureAspect::All,
				texture: &texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
			},
			&rgba,
			wgpu::TexelCopyBufferLayout {
				offset: 0,
				bytes_per_row: Some(4 * dimensions.0),
				rows_per_image: Some(dimensions.1),
			},
			size,
		);

		let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

		Ok(Self { texture, view })
	}

	pub fn from_bytes_with_format(
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		bytes: &[u8],
		label: &str,
		format: wgpu::TextureFormat,
	) -> anyhow::Result<Self> {
		// Decode image (using image crate)
		let img = image::load_from_memory(bytes)?;
		let rgba = img.into_rgba8();
		let dimensions = rgba.dimensions();

		Self::from_rgba8_with_format(device, queue, dimensions, &rgba, label, format)
	}

	pub fn from_rgba8_with_format(
		device: &wgpu::Device,
		queue: &wgpu::Queue,
		size: (u32, u32),
		rgba: &[u8],
		label: &str,
		format: wgpu::TextureFormat, // ← Surface format
	) -> anyhow::Result<Self> {
		let extend = wgpu::Extent3d {
			width: size.0,
			height: size.1,
			depth_or_array_layers: 1,
		};

		// CRITICAL: Use surface format for compatibility
		let texture = device.create_texture(&wgpu::TextureDescriptor {
			label: Some(label),
			size: extend,
			mip_level_count: 1,
			sample_count: 1,
			dimension: wgpu::TextureDimension::D2,
			format,
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT
				| wgpu::TextureUsages::TEXTURE_BINDING
				| wgpu::TextureUsages::COPY_SRC
				| wgpu::TextureUsages::COPY_DST,
			view_formats: &[],
		});

		// Handle format conversion during upload if needed
		let (bytes_per_row, upload_data) = match format {
			wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
				(Some(4 * size.0), rgba.to_vec())
			}
			wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
				// Convert RGBA → BGRA on CPU (cheap for initialization)
				let mut bgra = Vec::with_capacity(rgba.len());
				for chunk in rgba.chunks_exact(4) {
					bgra.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
				}
				(Some(4 * size.0), bgra)
			}
			_ => {
				// Fallback: use RGBA upload and let GPU handle conversion via shader
				(Some(4 * size.0), rgba.to_vec())
			}
		};

		queue.write_texture(
			wgpu::TexelCopyTextureInfo {
				texture: &texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
				aspect: wgpu::TextureAspect::All,
			},
			&upload_data,
			wgpu::TexelCopyBufferLayout {
				offset: 0,
				bytes_per_row,
				rows_per_image: Some(size.1),
			},
			extend,
		);

		let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
		Ok(Self { texture, view })
	}
}
