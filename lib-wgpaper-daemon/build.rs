use std::env;
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    // Rebuild if the shader source changes
    println!("cargo:rerun-if-changed=../wgpaper-shaders/src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shader_crate_dir = manifest_dir.join("../wgpaper-shaders");

    // Use spirv-builder to compile the shader crate to SPIR-V.
    let result = spirv_builder::SpirvBuilder::new(shader_crate_dir, "spirv-unknown-vulkan1.1")
        .build()?;

    // spirv-builder gives us either a single module or a multi-module result.
    // For our crate (all entry points in one lib), we get a single .spv file.
    let spv_path = match result.module {
        spirv_builder::ModuleResult::SingleModule(path) => path,
        spirv_builder::ModuleResult::MultiModule(_modules) => {
            panic!("Expected a single SPIR-V module, got multiple. \
                    Make sure all entry points are in one file.");
        }
    };

    // Copy the .spv file to OUT_DIR so it can be included at compile time.
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let dest = out_dir.join("wgpaper_shaders.spv");
    std::fs::copy(&spv_path, &dest)?;

    println!("cargo:warning=Shader compiled: {} -> {}", spv_path.display(), dest.display());

    Ok(())
}
