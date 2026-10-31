fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=native/bridge.cpp");
    println!("cargo:rerun-if-env-changed=VOICEPASTE_CT2_INCLUDE");
    println!("cargo:rerun-if-env-changed=VOICEPASTE_CT2_LIB");
    let include = std::env::var("VOICEPASTE_CT2_INCLUDE")
        .expect("Use cargo xtask build: matching CT2 4.7.1 headers required");
    let library = std::env::var("VOICEPASTE_CT2_LIB")
        .expect("Use cargo xtask build: CT2 import library required");
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .flag("/EHsc")
        .opt_level(2)
        .include(include)
        .file("native/bridge.cpp")
        .compile("voicepaste_ct2_bridge");
    println!("cargo:rustc-link-search=native={library}");
    println!("cargo:rustc-link-lib=dylib=ctranslate2");
}
