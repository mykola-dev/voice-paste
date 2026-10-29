//! Runnable profiles are public outputs; everything else belongs to the build cache.
pub const CACHE: &str = "build/cache";
pub const PACKAGE: &str = "build/package";
pub const TARGET: &str = "build/cache/target";
pub const NATIVE_LIB: &str = "build/cache/native";
pub const PREPARED_NATIVE: &str = "build/cache/prepared-native";
pub const UPSTREAM_NATIVE: &str = "build/cache/upstream-native";

pub fn profile(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_and_launcher_use_the_canonical_layout() {
        assert!(
            include_str!("../../.cargo/config.toml")
                .contains(&format!("target-dir = \"{TARGET}\""))
        );
        let launcher = include_str!("../../build-portable-windows.bat");
        assert!(launcher.contains("build\\cache\\rust-tools\\cargo"));
        assert!(!launcher.contains("build\\rust-migration-tools"));
        assert_eq!(profile(false), "debug");
        assert_eq!(profile(true), "release");
        assert_eq!(PACKAGE, "build/package");
        assert!(
            include_str!("../../.github/workflows/release-windows.yml")
                .contains("build/package/*.zip")
        );
        for path in [TARGET, NATIVE_LIB, PREPARED_NATIVE, UPSTREAM_NATIVE] {
            assert!(path.starts_with(&format!("{CACHE}/")));
        }
    }
}
