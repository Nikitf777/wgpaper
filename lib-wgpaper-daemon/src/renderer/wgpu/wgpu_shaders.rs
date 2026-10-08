use std::borrow::Cow;

use wgpaper_abi::PerFrameDataUniform;
use wgpaper_config::{Background, ScalingMode};
use wgpu::{Device, ShaderModule, ShaderModuleDescriptor, ShaderSource};

// Only used by the tests below, which parse the generated WGSL with the same
// front end `wgpu` uses.
#[cfg(test)]
use wgpu::naga;

/// Compiled SPIR-V blob produced by `build.rs` using `spirv-builder`.
static SHADER_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wgpaper_shaders.spv"));

/// Convert the raw SPIR-V bytes to `Cow<[u32]>` for use with
/// `ShaderSource::SpirV`, handling alignment safely.
///
/// `include_bytes!` data may not be 4-byte-aligned, so we use
/// `wgpu::util::make_spirv_raw` which copies if necessary.
fn make_spv_source() -> Cow<'static, [u32]> {
	wgpu::util::make_spirv_raw(SHADER_SPV)
}

/// Create a SPIR-V shader module + entry point name.
pub struct SpvShader {
	pub module: ShaderModule,
	pub entry_point: &'static str,
}

/// Create a SPIR-V shader module for the given entry point.
pub fn create_spv_module(device: &Device, label: &str, entry_point: &'static str) -> SpvShader {
	let module = device.create_shader_module(ShaderModuleDescriptor {
		label: Some(label),
		source: ShaderSource::SpirV(make_spv_source()),
	});
	SpvShader {
		module,
		entry_point,
	}
}

// ── vertex shader ─────────────────────────────────────────────────────

/// Vertex shader: full-screen triangle.
pub const VS_ENTRY: &str = "vs_main";

// ── scaling fragment shaders ─────────────────────────────────────────

/// Return the fragment entry-point and a human-readable label for a scaling mode.
pub fn scaling_shader_info(mode: &ScalingMode) -> (&'static str, &'static str) {
	match mode {
		ScalingMode::Fit { background } => {
			if matches!(background, Background::AutoColor | Background::CssColor(_)) {
				("fs_fit_bg", "scaling_fragment_shader_fit_bg_color")
			} else {
				("fs_fit", "scaling_fragment_shader_fit")
			}
		}
		ScalingMode::Center { background } => {
			if matches!(background, Background::AutoColor | Background::CssColor(_)) {
				("fs_center_bg", "scaling_fragment_shader_center_bg_color")
			} else {
				("fs_center", "scaling_fragment_shader_center")
			}
		}
		ScalingMode::Stretch => ("fs_stretch", "scaling_fragment_shader_stretch"),
		ScalingMode::Cover => ("fs_cover", "scaling_fragment_shader_cover"),
	}
}

/// Create a scaling fragment shader module.  Returns the module and the
/// entry point name that should be used when building the pipeline.
pub fn create_scaling_fragment_shader(device: &Device, mode: &ScalingMode) -> SpvShader {
	let (entry_point, label) = scaling_shader_info(mode);
	create_spv_module(device, label, entry_point)
}

// ── transition fragment shader ───────────────────────────────────────

/// Entry point for the default cross-fade transition.
pub const DEFAULT_TRANSITION_ENTRY: &str = "fs_default_transition";

/// The fragment entry point a custom WGSL transition shader must expose.
pub const CUSTOM_TRANSITION_ENTRY: &str = "fs_main";

/// Create the transition (animation) fragment shader module.
///
/// The compiled SPIR-V blob contains `fs_default_transition`.  If
/// `shader_source` is provided it is parsed as WGSL (for user-provided
/// custom shaders).
pub fn create_animation_shader(device: &Device, shader_source: Option<&str>) -> SpvShader {
	match shader_source {
		Some(src) => {
			let module = device.create_shader_module(ShaderModuleDescriptor {
				label: Some("custom_animation_shader"),
				source: ShaderSource::Wgsl(Cow::Owned(prepend_prelude(src))),
			});
			SpvShader {
				module,
				entry_point: CUSTOM_TRANSITION_ENTRY,
			}
		}
		None => create_spv_module(device, "animation_shader", DEFAULT_TRANSITION_ENTRY),
	}
}

/// Put the generated ABI declarations in front of a user's shader source.
///
/// The prelude carries the uniform struct and every binding the transition
/// renderer's bind group layout provides, both generated from the single
/// declaration in `wgpaper-abi`. Users therefore never hand-write the struct,
/// which is what previously let the three copies of the layout drift apart.
fn prepend_prelude(user_source: &str) -> String {
	let mut source = String::with_capacity(
		PerFrameDataUniform::WGSL_TRANSITION_PRELUDE.len() + user_source.len() + 1,
	);
	source.push_str(PerFrameDataUniform::WGSL_TRANSITION_PRELUDE);
	source.push('\n');
	source.push_str(user_source);
	source
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn prelude_precedes_user_source() {
		let user_source = "fn fs_main() {}";
		let composed = prepend_prelude(user_source);
		let prelude_len = PerFrameDataUniform::WGSL_TRANSITION_PRELUDE.len();

		assert!(composed.starts_with(PerFrameDataUniform::WGSL_TRANSITION_PRELUDE));
		assert!(composed.ends_with(user_source));

		// The user's code must come after every generated declaration.
		let uniform_at = composed.find("var<uniform> per_frame").unwrap();
		assert!(uniform_at < prelude_len);
		assert!(composed[prelude_len..].contains(user_source));
	}

	#[test]
	fn custom_shaders_use_the_fs_main_entry_point() {
		assert_eq!(CUSTOM_TRANSITION_ENTRY, "fs_main");
	}

	/// Parse `source` as WGSL and return the error message on failure.
	fn wgsl_error(source: &str) -> Option<String> {
		let mut validator = naga::valid::Validator::new(
			naga::valid::ValidationFlags::all(),
			naga::valid::Capabilities::empty(),
		);
		match naga::front::wgsl::parse_str(source) {
			Err(err) => Some(err.emit_to_string(source)),
			Ok(module) => validator.validate(&module).err().map(|err| err.to_string()),
		}
	}

	#[test]
	fn generated_prelude_is_valid_wgsl() {
		// The prelude alone is not a complete shader, so append an entry point
		// that actually uses the bindings it declares.
		let source = prepend_prelude(SAMPLE_TRANSITION_SHADER);
		if let Some(err) = wgsl_error(&source) {
			panic!("generated prelude is not valid WGSL:\n{err}\n--- source ---\n{source}");
		}
	}

	#[test]
	fn prelude_exposes_the_expected_entry_points() {
		let module = naga::front::wgsl::parse_str(&prepend_prelude(SAMPLE_TRANSITION_SHADER))
			.expect("sample shader should parse");

		let names: Vec<&str> = module
			.entry_points
			.iter()
			.map(|ep| ep.name.as_str())
			.collect();

		assert_eq!(names, [CUSTOM_TRANSITION_ENTRY]);
	}

	/// A transition shader written the way a user now writes one: no struct, no
	/// bindings, just the entry point.
	const SAMPLE_TRANSITION_SHADER: &str = r#"
@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
	let prev = textureSample(prev_texture, tex_sampler, tex_coords);
	let next = textureSample(target_texture, tex_sampler, tex_coords);
	return mix(prev, next, per_frame.progress_bezier);
}
"#;

	/// The core ABI check: every field offset that the WGSL front end computes
	/// must equal the offset the Rust struct actually has.
	///
	/// This is the assertion that was impossible to write before the layout was
	/// generated from a single declaration. Parsing proves the WGSL is valid;
	/// it does not prove the WGSL and the Rust agree, because each is
	/// internally consistent on its own. Comparing naga's computed offsets to
	/// `offset_of!` is what actually pins the two together.
	#[test]
	fn wgsl_field_offsets_match_the_rust_struct() {
		let module = naga::front::wgsl::parse_str(&prepend_prelude(SAMPLE_TRANSITION_SHADER))
			.expect("sample shader should parse");

		let uniform_ty = module
			.global_variables
			.iter()
			.find(|(_, var)| var.name.as_deref() == Some("per_frame"))
			.map(|(_, var)| var.ty)
			.expect("prelude should declare `per_frame`");

		let naga::TypeInner::Struct { members, .. } = &module.types[uniform_ty].inner else {
			panic!("`per_frame` should be bound to a struct");
		};

		let rust_offsets = PerFrameDataUniform::FIELD_OFFSETS;
		let rust_sizes = PerFrameDataUniform::FIELD_SIZES;
		assert_eq!(
			members.len(),
			rust_offsets.len(),
			"WGSL struct and Rust struct have a different number of fields"
		);

		for ((member, (rust_name, rust_offset)), (_, rust_size)) in
			members.iter().zip(rust_offsets).zip(rust_sizes)
		{
			assert_eq!(
				member.name.as_deref(),
				Some(*rust_name),
				"field names are out of order between WGSL and Rust"
			);

			let (wgsl_size, _) = wgsl_type_layout(&module.types[member.ty].inner);
			assert_eq!(
				member.offset as usize, *rust_offset,
				"`{rust_name}` is at offset {} in WGSL but {rust_offset} in Rust",
				member.offset
			);
			assert_eq!(
				wgsl_size, *rust_size,
				"`{rust_name}` is {wgsl_size} bytes in WGSL but {rust_size} in Rust"
			);
		}
	}

	/// Size and alignment of the WGSL types this block uses, per the WGSL
	/// memory-layout rules that `var<uniform>` obeys.
	///
	/// Needed because offsets alone are not enough: `vec3<f32>` and
	/// `vec4<f32>` are both 16-aligned and can therefore sit at the same
	/// offset, so a wrong component count would slip past an offset-only check.
	fn wgsl_type_layout(inner: &naga::TypeInner) -> (usize, usize) {
		use naga::{ScalarKind, TypeInner, VectorSize};

		match inner {
			TypeInner::Scalar(scalar) => match scalar.kind {
				ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Float => (4, 4),
				other => panic!("unsupported 4-byte scalar in the uniform block: {other:?}"),
			},
			TypeInner::Vector { size, .. } => match size {
				VectorSize::Bi => (8, 8),
				VectorSize::Tri => (12, 16),
				VectorSize::Quad => (16, 16),
				other => panic!("unsupported vector in the uniform block: {other:?}"),
			},
			other => panic!("unsupported type in the uniform block: {other:?}"),
		}
	}
}
