//! Construction of the macOS single-asset curl transport.
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub(crate) fn command(url: &str, path: &Path, offset: u64, size: u64) -> Command {
    let mut command = Command::new("/usr/bin/curl");
    command
        // curl only disables its default config if this is the first argument.
        .arg("--disable")
        // Do not allow inherited TLS session-key logging. Keep proxy and CA
        // environment variables for managed corporate networks.
        .env_remove("SSLKEYLOGFILE")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--write-out",
            "%{http_code}",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            "1800",
            "--speed-time",
            "30",
            "--speed-limit",
            "1024",
            "--continue-at",
        ])
        .arg(offset.to_string())
        .arg("--output")
        .arg(path)
        .arg("--max-filesize")
        .arg(size.to_string())
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_configuration_is_disabled_without_clearing_network_environment() {
        let command = command(
            "https://example.invalid/part",
            Path::new("part.partial"),
            42,
            100,
        );
        assert_eq!(command.get_program(), "/usr/bin/curl");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(args[0], "--disable");
        for pair in [
            ["--proto", "=https"],
            ["--proto-redir", "=https"],
            ["--continue-at", "42"],
            ["--max-filesize", "100"],
        ] {
            assert!(args.windows(2).any(|window| window == pair));
        }
        assert_eq!(args.last(), Some(&"https://example.invalid/part"));
        let env: Vec<_> = command.get_envs().collect();
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "SSLKEYLOGFILE");
        assert_eq!(env[0].1, None);
    }
}
