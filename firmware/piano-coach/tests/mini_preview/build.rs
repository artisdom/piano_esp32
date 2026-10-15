fn main() {
    // Any board's glyph list works for the preview (the round board's is the
    // smaller one; its names cover the preview's sample titles).
    let board = std::fs::canonicalize("../../boards/2424s012").unwrap();
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
        .with_sdf_fonts(true)
        .with_library_paths([("board".to_string(), board)].into());
    slint_build::compile_with_config("../../ui/mini/app.slint", config).unwrap();
}
