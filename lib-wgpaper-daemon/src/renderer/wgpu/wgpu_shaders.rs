use std::borrow::Cow;

use wgpaper_config::{Background, ScalingMode};
use wgpu::{Device, ShaderModule, ShaderModuleDescriptor, ShaderSource};

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
				source: ShaderSource::Wgsl(Cow::Owned(src.to_string().into())),
			});
			SpvShader {
				module,
				entry_point: "fs_main",
			}
		}
		None => create_spv_module(device, "animation_shader", DEFAULT_TRANSITION_ENTRY),
	}
}
