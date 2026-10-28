fn main() {
    println!("cargo:rerun-if-changed=resources.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // GPUI's Windows backend loads icon resource 1 for its window class.
        // The same multi-resolution original asset is used by Explorer.
        embed_resource::compile("resources.rc", embed_resource::NONE)
            .manifest_required()
            .expect("VoicePaste Windows icon resource must compile");
    }
}
