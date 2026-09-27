fn main() {
    println!("cargo:rerun-if-changed=assets/icons/flix.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    winresource::WindowsResource::new()
        .set_icon("assets/icons/flix.ico")
        .compile()
        .expect("Windows resources must compile");
}
