fn main() {
    // The .slint sources stay in the app crate's ui folder.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("../thrasher-app/ui/app.slint", config)
        .expect("compile ui/app.slint");
}
