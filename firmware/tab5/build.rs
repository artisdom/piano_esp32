fn main() {
    embuild::espidf::sysenv::output();

    // Glyphs are pre-rendered for the software renderer (no fonts on the
    // device). SDF stores one distance field per glyph, scaled at draw time.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
        .with_sdf_fonts(true);
    slint_build::compile_with_config("ui/app.slint", config).expect("slint build failed");
}
