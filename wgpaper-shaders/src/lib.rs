//! Rust-GPU shaders for wgpaper.
//!
//! All entry points are in a single crate so `spirv-builder` produces one
//! SPIR-V module. Downstream code (wgpaper-daemon) selects the desired
//! entry point by name when creating `wgpu::ShaderModule` objects.

#![cfg_attr(target_arch = "spirv", no_std)]

use spirv_std::glam::{vec2, Vec2, Vec4};
use spirv_std::image::Image2d;
use spirv_std::{spirv, Sampler};

// ── shared uniform data ───────────────────────────────────────────────
// Must match `PerFrameDataUniform` in `wgpu_uniforms.rs` byte-for-byte.
//
// NOTE: we use `Vec2` / `Vec4` instead of `[f32; 2]` / `[f32; 4]` because
// Vulkan uniform-buffer layout rules require array alignment ≥ 16, while
// the natural stride of `[f32; 2]` is only 4.  `Vec2` / `Vec4` are structs
// with proper alignment (8 / 16 bytes) and satisfy the standard rules.

#[repr(C)]
pub struct PerFrameDataUniform {
	pub virtual_screen_size: Vec2,
	pub screen_size: Vec2,
	pub texture_size: Vec2,
	pub virtual_screen_aspect: f32,
	pub screen_aspect: f32,
	pub texture_aspect: f32,
	// Two separate f32s (not Vec2) to keep the same byte layout as the
	// original WGSL ABI — custom WGSL transition shaders read these as
	// `progress_bezier: f32` and `progress_linear: f32`.
	pub progress_bezier: f32,
	pub progress_linear: f32,
	// Explicit padding: WGSL var<uniform> requires vec4 align 16, so
	// bg_color must start at offset 48, not 44.
	pub _pad_to_bg_color: u32,
	pub bg_color: Vec4,
}
// NOTE: only the fields above are read by the shader.  The host-side
// (Rust) buffer is larger (includes `_padding: [u32; 53]`) to satisfy
// wgpu's `min_binding_size: 256` constraint; those trailing bytes are
// simply ignored by the GPU.

// ── vertex shader (full‑screen triangle) ──────────────────────────────

#[spirv(vertex)]
pub fn vs_main(
	#[spirv(vertex_index)] vert_idx: i32,
	#[spirv(position)] out_pos: &mut Vec4,
	#[spirv(location = 0)] out_tex_coords: &mut Vec2,
) {
	let uv = vec2(((vert_idx << 1) & 2) as f32, (vert_idx & 2) as f32);
	let pos = 2.0 * uv - Vec2::ONE;
	*out_pos = pos.extend(0.0).extend(1.0);
	// Flip Y to match the original WGSL vertex shader's convention:
	// the old shader used `1.0 - (pos.y + 1.0) * 0.5`.
	*out_tex_coords = vec2(uv.x, 1.0 - uv.y);
}

// ── scaling fragment shaders ─────────────────────────────────────────
// Each takes one texture + sampler at group(0) and uniforms at group(1).

fn sample_centered_uv(tex_coords: Vec2, screen_size: Vec2, texture_size: Vec2) -> Vec2 {
	let scale = screen_size / texture_size;
	let offset = (screen_size - texture_size) * 0.5 / texture_size;
	tex_coords * scale - offset
}

#[spirv(fragment)]
#[allow(unused_variables)]
pub fn fs_stretch(
	#[spirv(descriptor_set = 0, binding = 0)] texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	*output = texture.sample(*tex_sampler, tex_coords);
}

#[spirv(fragment)]
pub fn fs_fit(
	#[spirv(descriptor_set = 0, binding = 0)] texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	let screen_size = uniforms.screen_size;
	let texture_size = uniforms.texture_size;
	let screen_aspect = screen_size.x / screen_size.y;
	let texture_aspect = texture_size.x / texture_size.y;

	let mut uv = tex_coords;
	if texture_aspect > screen_aspect {
		// Width is limiting – stretch vertically
		let ratio = texture_aspect / screen_aspect;
		uv = (uv - 0.5) * vec2(1.0, ratio) + 0.5;
	} else {
		// Height is limiting – stretch horizontally
		let ratio = screen_aspect / texture_aspect;
		uv = (uv - 0.5) * vec2(ratio, 1.0) + 0.5;
	}
	*output = texture.sample(*tex_sampler, uv);
}

#[spirv(fragment)]
pub fn fs_fit_bg(
	#[spirv(descriptor_set = 0, binding = 0)] texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	let screen_size = uniforms.screen_size;
	let texture_size = uniforms.texture_size;
	let screen_aspect = screen_size.x / screen_size.y;
	let texture_aspect = texture_size.x / texture_size.y;

	let mut uv = tex_coords;
	if texture_aspect > screen_aspect {
		let ratio = texture_aspect / screen_aspect;
		uv = (uv - 0.5) * vec2(1.0, ratio) + 0.5;
	} else {
		let ratio = screen_aspect / texture_aspect;
		uv = (uv - 0.5) * vec2(ratio, 1.0) + 0.5;
	}

	if uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 {
		*output = uniforms.bg_color;
	} else {
		*output = texture.sample(*tex_sampler, uv);
	}
}

#[spirv(fragment)]
pub fn fs_cover(
	#[spirv(descriptor_set = 0, binding = 0)] texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	let screen_size = uniforms.screen_size;
	let texture_size = uniforms.texture_size;
	let screen_aspect = screen_size.x / screen_size.y;
	let texture_aspect = texture_size.x / texture_size.y;

	let mut uv = tex_coords;
	if texture_aspect > screen_aspect {
		// Width is limiting – crop sides
		let ratio = screen_aspect / texture_aspect;
		uv = (uv - 0.5) * vec2(ratio, 1.0) + 0.5;
	} else {
		// Height is limiting – crop top/bottom
		let ratio = texture_aspect / screen_aspect;
		uv = (uv - 0.5) * vec2(1.0, ratio) + 0.5;
	}
	*output = texture.sample(*tex_sampler, uv);
}

#[spirv(fragment)]
pub fn fs_center(
	#[spirv(descriptor_set = 0, binding = 0)] texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	let screen_size = uniforms.screen_size;
	let texture_size = uniforms.texture_size;
	let uv = sample_centered_uv(tex_coords, screen_size, texture_size);
	*output = texture.sample(*tex_sampler, uv);
}

#[spirv(fragment)]
pub fn fs_center_bg(
	#[spirv(descriptor_set = 0, binding = 0)] texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	let screen_size = uniforms.screen_size;
	let texture_size = uniforms.texture_size;
	let uv = sample_centered_uv(tex_coords, screen_size, texture_size);

	if uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0 {
		*output = uniforms.bg_color;
	} else {
		*output = texture.sample(*tex_sampler, uv);
	}
}

// ── transition fragment shader ────────────────────────────────────────
// Two textures + one sampler at group(0), uniforms at group(1).

#[spirv(fragment)]
pub fn fs_default_transition(
	#[spirv(descriptor_set = 0, binding = 0)] prev_texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 1)] target_texture: &Image2d,
	#[spirv(descriptor_set = 0, binding = 2)] tex_sampler: &Sampler,
	#[spirv(descriptor_set = 1, binding = 0, uniform)] uniforms: &PerFrameDataUniform,
	#[spirv(location = 0)] tex_coords: Vec2,
	output: &mut Vec4,
) {
	let prev_color = prev_texture.sample(*tex_sampler, tex_coords);
	let target_color = target_texture.sample(*tex_sampler, tex_coords);
	let t = uniforms.progress_bezier; // progress_bezier
	*output = prev_color.lerp(target_color, t);
}
