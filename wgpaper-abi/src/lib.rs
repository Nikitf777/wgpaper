//! The uniform-block ABI shared by the host and every shader.
//!
//! This crate exists to kill a class of bug that is invisible until a picture
//! comes out wrong. The uniform block used by the transition and scaling
//! shaders used to be declared *three* times: once as a host-side struct with
//! `[f32; 2]` fields, once in the rust-gpu shader crate with `Vec2` fields,
//! and once more by hand in every user-provided WGSL file. Nothing checked
//! that the three agreed, and a mismatch is UB rather than a compile error.
//!
//! Now there is one declaration, [`uniform_abi!`], which generates the
//! `#[repr(C)]` struct, the WGSL text handed to `wgpu`, and a table of field
//! offsets. The layout is then pinned by `const` assertions, so a change that
//! would silently break the ABI fails the build instead.
//!
//! # Why `Vec2`/`Vec4` and not `[f32; 2]`/`[f32; 4]`
//!
//! The block lives in a Vulkan uniform buffer, where a `vec2<f32>` has an
//! alignment of 8 and a `vec4<f32>` an alignment of 16. glam's `Vec2`/`Vec4`
//! are `#[repr(C)]` wrappers over plain `f32`s carrying exactly those
//! alignments, so one declaration describes the same bytes to Rust, to
//! WGSL, and to the shader compiler. Sibling arrays do not: a `#[repr(C)]`
//! `[f32; 4]` field is only 4-aligned, which is why the old host struct needed
//! a hand-written `_pad_to_bg_color` field to reach offset 48.
//!
//! # Layout
//!
//! | Offset | Field                   | WGSL        |
//! |--------|-------------------------|-------------|
//! | 0      | `virtual_screen_size`   | `vec2<f32>` |
//! | 8      | `screen_size`           | `vec2<f32>` |
//! | 16     | `texture_size`          | `vec2<f32>` |
//! | 24     | `virtual_screen_aspect` | `f32`       |
//! | 28     | `screen_aspect`         | `f32`       |
//! | 32     | `texture_aspect`        | `f32`       |
//! | 36     | `progress_bezier`       | `f32`       |
//! | 40     | `progress_linear`       | `f32`       |
//! | 44     | `_pad_to_bg_color`      | `u32`       |
//! | 48     | `bg_color`              | `vec4<f32>` |
//!
//! Total size 64, alignment 16. The asserted constants below are what make
//! this table a guarantee rather than a comment.

#![cfg_attr(target_arch = "spirv", no_std)]

pub use glam;
pub use glam::{Vec2, Vec4};

use core::mem::{align_of, size_of};

/// How far along a transition animation is, in the two forms the shaders want.
///
/// This lives in the ABI crate because it *is* part of the wire format: it is
/// written into the uniform block as `progress_bezier` and `progress_linear`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionProgress {
	/// Eased progress, for effects that should accelerate and decelerate.
	pub progress_bezier: f32,
	/// Linear progress, for effects that need constant speed.
	pub progress_linear: f32,
}

impl TransitionProgress {
	/// The state a transition starts from.
	pub const RESET: Self = Self {
		progress_bezier: 0.0,
		progress_linear: 0.0,
	};

	/// The state a transition ends in.
	pub const FINISHED: Self = Self {
		progress_bezier: 1.0,
		progress_linear: 1.0,
	};

	pub const fn reset() -> Self {
		Self::RESET
	}

	pub const fn finished() -> Self {
		Self::FINISHED
	}

	pub fn is_finished(&self) -> bool {
		self.progress_bezier == 1.0 && self.progress_linear >= 1.0
	}
}

/// Declares the shared uniform block.
///
/// Each field is written once as `name: RustTy => "wgsl_ty"`. From that single
/// list this generates the `#[repr(C)]` struct, the WGSL declaration, and
/// [`FIELD_OFFSETS`](Self::FIELD_OFFSETS).
///
/// The struct declaration appears twice in the expansion — once for the type
/// and once inside `WGSL_TRANSITION_PRELUDE` — because `concat!` only accepts
/// literals and cannot splice in another `const`. Both copies are built from
/// the same tokens, so adding a field updates the Rust type and the WGSL
/// together.
macro_rules! uniform_abi {
	(
		$(#[$struct_attr:meta])*
		$name:ident {
			$(
				$(#[$field_attr:meta])*
				$field:ident : $ty:ty => $wgsl_ty:literal
			),* $(,)?
		}
	) => {
		$(#[$struct_attr])*
		#[repr(C)]
		#[derive(Debug, Clone, Copy, PartialEq)]
		pub struct $name {
			$(
				$(#[$field_attr])*
				pub $field : $ty,
			)*
		}

		impl $name {
			/// Size of the block in bytes.
			pub const SIZE: usize = ::core::mem::size_of::<$name>();

			/// Alignment of the block in bytes.
			pub const ALIGN: usize = ::core::mem::align_of::<$name>();

			/// Every field with its byte offset, in declaration order.
			pub const FIELD_OFFSETS: &'static [(&'static str, usize)] = &[
				$((stringify!($field), ::core::mem::offset_of!($name, $field)),)*
			];

			/// Every field with its byte size, in declaration order.
			pub const FIELD_SIZES: &'static [(&'static str, usize)] = &[
				$((stringify!($field), ::core::mem::size_of::<$ty>()),)*
			];

			/// The struct declaration, on its own.
			pub const WGSL_STRUCT_DECL: &'static str = concat!(
				"struct ", stringify!($name), " {\n",
				$(
					"    ", stringify!($field), ": ", $wgsl_ty, ",\n",
				)*
				"};\n",
			);

			/// The full prelude prepended to a user-provided transition shader.
			///
			/// It declares the uniform struct and every binding the transition
			/// renderer's bind group layout provides, so a custom shader only
			/// has to supply `fs_main`:
			///
			/// ```wgsl
			/// @fragment
			/// fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
			///     let prev = textureSample(prev_texture, tex_sampler, tex_coords);
			///     let next = textureSample(target_texture, tex_sampler, tex_coords);
			///     return mix(prev, next, per_frame.progress_bezier);
			/// }
			/// ```
			pub const WGSL_TRANSITION_PRELUDE: &'static str = concat!(
				"// ============================================================\n",
				"//  Generated by the `wgpaper-abi` crate. Do not edit.\n",
				"//\n",
				"//  These declarations are fixed by the transition renderer's bind\n",
				"//  group layout. Write your `fs_main` below this block; do not\n",
				"//  redeclare anything above it.\n",
				"// ============================================================\n",
				"\n",
				"struct ", stringify!($name), " {\n",
				$(
					"    ", stringify!($field), ": ", $wgsl_ty, ",\n",
				)*
				"};\n",
				"\n",
				"// group(0): the frame being transitioned away from, the frame being\n",
				"// transitioned to, and the sampler they share.\n",
				"@group(0) @binding(0) var prev_texture: texture_2d<f32>;\n",
				"@group(0) @binding(1) var target_texture: texture_2d<f32>;\n",
				"@group(0) @binding(2) var tex_sampler: sampler;\n",
				"\n",
				"// group(1): the per-frame uniform block.\n",
				"@group(1) @binding(0) var<uniform> per_frame: ", stringify!($name), ";\n",
			);
		}
	};
}

uniform_abi! {
	/// The per-frame uniform block every wgpaper shader can read.
	///
	/// Written by [`PerFrameUniformManager`](../../lib_wgpaper_daemon/renderer/wgpu/wgpu_uniforms/struct.PerFrameUniformManager.html)
	/// on the host and bound at group(1) binding(0) in every shader.
	PerFrameDataUniform {
		/// Size of the whole desktop in logical pixels, across all outputs.
		///
		/// Equal to `screen_size` for now: the single-canvas mode that would
		/// give this field a distinct value is not implemented yet, and
		/// `calculate_global_bounds` in `app/output.rs` is still unused.
		virtual_screen_size: Vec2 => "vec2<f32>",

		/// Size of the output this shader is rendering to, in pixels.
		screen_size: Vec2 => "vec2<f32>",

		/// Size of the source image, in pixels.
		texture_size: Vec2 => "vec2<f32>",

		/// `virtual_screen_size.x / virtual_screen_size.y`.
		virtual_screen_aspect: f32 => "f32",

		/// `screen_size.x / screen_size.y`.
		screen_aspect: f32 => "f32",

		/// `texture_size.x / texture_size.y`.
		texture_aspect: f32 => "f32",

		/// Eased transition progress, in `0.0..=1.0`.
		progress_bezier: f32 => "f32",

		/// Linear transition progress, in `0.0..=1.0`.
		progress_linear: f32 => "f32",

		/// Explicit padding so that `bg_color` lands on the 16-byte boundary
		/// a `vec4<f32>` requires in a uniform buffer.
		///
		/// This is a real field rather than implicit padding because
		/// `bytemuck::Pod` — which the host uses to hand the block to
		/// `wgpu::Queue::write_buffer` — forbids padding bytes.
		_pad_to_bg_color: u32 => "u32",

		/// Colour painted behind the wallpaper by the letterboxing scaling
		/// modes (`fit` / `center` with a solid background).
		bg_color: Vec4 => "vec4<f32>",
	}
}

impl PerFrameDataUniform {
	/// Builds a block from its constituent parts.
	pub const fn new(
		virtual_screen_size: Vec2,
		screen_size: Vec2,
		texture_size: Vec2,
		bg_color: Vec4,
		progress: TransitionProgress,
	) -> Self {
		Self {
			virtual_screen_size,
			screen_size,
			texture_size,
			virtual_screen_aspect: aspect_ratio(virtual_screen_size),
			screen_aspect: aspect_ratio(screen_size),
			texture_aspect: aspect_ratio(texture_size),
			progress_bezier: progress.progress_bezier,
			progress_linear: progress.progress_linear,
			_pad_to_bg_color: 0,
			bg_color,
		}
	}

	/// The transition progress currently encoded in this block.
	pub const fn transition_progress(&self) -> TransitionProgress {
		TransitionProgress {
			progress_bezier: self.progress_bezier,
			progress_linear: self.progress_linear,
		}
	}

	pub fn update_screen_size(&mut self, new_size: Vec2) {
		self.screen_size = new_size;
		self.screen_aspect = aspect_ratio(new_size);
	}

	pub fn update_texture_size(&mut self, new_size: Vec2) {
		self.texture_size = new_size;
		self.texture_aspect = aspect_ratio(new_size);
	}

	pub fn update_transition_progress(&mut self, new_progress: TransitionProgress) {
		self.progress_bezier = new_progress.progress_bezier;
		self.progress_linear = new_progress.progress_linear;
	}
}

const fn aspect_ratio(size: Vec2) -> f32 {
	size.x / size.y
}

// The ABI guarantee. These are `const`, so they are checked every time this
// crate is compiled — including by `spirv-builder` on the shader target, which
// is exactly where a silent layout change would be hardest to notice.
//
// The `scalar-math` feature of glam is the hazard this guards against: it
// removes `Vec4`'s `repr(align(16))`, which would drop `bg_color` from offset
// 48 to 44 and desynchronise the block from the WGSL and SPIR-V views of it.
const _: () = {
	use core::mem::offset_of;

	assert!(size_of::<PerFrameDataUniform>() == 64);
	assert!(align_of::<PerFrameDataUniform>() == 16);

	assert!(offset_of!(PerFrameDataUniform, virtual_screen_size) == 0);
	assert!(offset_of!(PerFrameDataUniform, screen_size) == 8);
	assert!(offset_of!(PerFrameDataUniform, texture_size) == 16);
	assert!(offset_of!(PerFrameDataUniform, virtual_screen_aspect) == 24);
	assert!(offset_of!(PerFrameDataUniform, screen_aspect) == 28);
	assert!(offset_of!(PerFrameDataUniform, texture_aspect) == 32);
	assert!(offset_of!(PerFrameDataUniform, progress_bezier) == 36);
	assert!(offset_of!(PerFrameDataUniform, progress_linear) == 40);
	assert!(offset_of!(PerFrameDataUniform, _pad_to_bg_color) == 44);

	// The one the `scalar-math` feature would break: without `Vec4`'s
	// `repr(align(16))` this lands on 44 and desynchronises the block from
	// the WGSL and SPIR-V views of it.
	assert!(offset_of!(PerFrameDataUniform, bg_color) == 48);

	// The trailing field must end exactly at the block size, otherwise the
	// struct has implicit tail padding and is not `Pod`.
	assert!(offset_of!(PerFrameDataUniform, bg_color) + size_of::<Vec4>() == 64);
};

// The block is uploaded with `bytemuck::bytes_of`, which requires the type to
// be free of padding bytes. Every field is `Pod`, the struct is `repr(C)`, and
// the assertion above confirms the fields tile the block exactly, with
// `_pad_to_bg_color` making the otherwise-unrepresentable gap explicit.
//
// These impls cannot be derived: glam only implements `bytemuck` traits for
// `Vec2`/`Vec4` when its own `bytemuck` feature is on, and enabling that
// feature for the shader build is not worth the coupling.
#[cfg(not(target_arch = "spirv"))]
unsafe impl bytemuck::Pod for PerFrameDataUniform {}
#[cfg(not(target_arch = "spirv"))]
unsafe impl bytemuck::Zeroable for PerFrameDataUniform {}

#[cfg(all(test, not(target_arch = "spirv")))]
mod tests {
	use super::*;

	/// The offsets the WGSL and the shaders assume. Duplicated from the module
	/// docs on purpose: the `const` assertions in the parent module already
	/// pin the layout, and this keeps the tests honest about intent.
	const EXPECTED: &[(&str, usize)] = &[
		("virtual_screen_size", 0),
		("screen_size", 8),
		("texture_size", 16),
		("virtual_screen_aspect", 24),
		("screen_aspect", 28),
		("texture_aspect", 32),
		("progress_bezier", 36),
		("progress_linear", 40),
		("_pad_to_bg_color", 44),
		("bg_color", 48),
	];

	#[test]
	fn field_offsets_match_the_documented_layout() {
		assert_eq!(PerFrameDataUniform::FIELD_OFFSETS, EXPECTED);
	}

	#[test]
	fn block_is_64_bytes_and_16_aligned() {
		assert_eq!(PerFrameDataUniform::SIZE, 64);
		assert_eq!(PerFrameDataUniform::ALIGN, 16);
	}

	#[test]
	fn block_has_no_implicit_padding() {
		// `Pod` is unsound if the struct has padding bytes, because
		// `bytes_of` would then read uninitialised memory. Confirm the
		// fields tile the block exactly.
		let mut cursor = 0;
		for ((name, offset), (_, size)) in PerFrameDataUniform::FIELD_OFFSETS
			.iter()
			.zip(PerFrameDataUniform::FIELD_SIZES)
		{
			assert_eq!(
				*offset, cursor,
				"gap before field `{name}` at offset {offset}"
			);
			cursor = offset + size;
		}
		assert_eq!(cursor, PerFrameDataUniform::SIZE);
	}

	#[test]
	fn aspects_are_derived_from_the_sizes() {
		let data = PerFrameDataUniform::new(
			Vec2::new(3840.0, 1080.0),
			Vec2::new(1920.0, 1080.0),
			Vec2::new(800.0, 600.0),
			Vec4::new(0.0, 0.0, 0.0, 1.0),
			TransitionProgress::finished(),
		);

		assert_eq!(data.virtual_screen_aspect, 3840.0 / 1080.0);
		assert_eq!(data.screen_aspect, 1920.0 / 1080.0);
		assert_eq!(data.texture_aspect, 800.0 / 600.0);
	}

	#[test]
	fn size_updates_keep_the_aspect_in_sync() {
		let mut data = PerFrameDataUniform::new(
			Vec2::new(100.0, 100.0),
			Vec2::new(1920.0, 1080.0),
			Vec2::new(800.0, 600.0),
			Vec4::ZERO,
			TransitionProgress::reset(),
		);

		data.update_screen_size(Vec2::new(800.0, 600.0));
		assert_eq!(data.screen_aspect, 800.0 / 600.0);

		data.update_texture_size(Vec2::new(100.0, 50.0));
		assert_eq!(data.texture_aspect, 2.0);

		data.update_transition_progress(TransitionProgress::finished());
		assert!(data.transition_progress().is_finished());
	}

	#[test]
	fn prelude_declares_every_field() {
		let prelude = PerFrameDataUniform::WGSL_TRANSITION_PRELUDE;
		for (name, _) in PerFrameDataUniform::FIELD_OFFSETS {
			assert!(
				prelude.contains(&format!("{name}:")),
				"prelude is missing field `{name}`"
			);
		}
	}

	#[test]
	fn prelude_declares_the_bindings_the_renderer_provides() {
		let prelude = PerFrameDataUniform::WGSL_TRANSITION_PRELUDE;

		// group(0): prev texture, target texture, sampler.
		assert!(prelude.contains("@group(0) @binding(0) var prev_texture"));
		assert!(prelude.contains("@group(0) @binding(1) var target_texture"));
		assert!(prelude.contains("@group(0) @binding(2) var tex_sampler"));

		// group(1): the uniform block.
		assert!(prelude.contains("@group(1) @binding(0) var<uniform> per_frame"));
	}

	#[test]
	fn prelude_is_valid_wgsl_by_construction() {
		// Cheap structural checks; a real parse needs a shader compiler.
		let prelude = PerFrameDataUniform::WGSL_TRANSITION_PRELUDE;

		assert!(prelude.contains("struct PerFrameDataUniform {"));
		assert_eq!(prelude.matches('{').count(), prelude.matches('}').count());
		assert!(!prelude.contains('\t'), "prelude must be space-indented");
	}
}
