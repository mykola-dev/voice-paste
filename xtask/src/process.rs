use anyhow::{Context, Result, bail};
use std::{collections::BTreeMap, ffi::OsString, path::Path, process::Command};

/// Every child, including MSVC/curl, inherits explicit build state and never opens a console.
pub struct Runner {
    pub root: std::path::PathBuf,
    pub env: BTreeMap<OsString, OsString>,
}

impl Runner {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.into(),
            env: BTreeMap::new(),
        }
    }

    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(&self.root).envs(&self.env);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        cmd
    }

    pub fn run(&self, cmd: &mut Command) -> Result<()> {
        let status = cmd.status().with_context(|| format!("Starting {cmd:?}"))?;
        if !status.success() {
            bail!("Command failed ({status}): {cmd:?}");
        }
        Ok(())
    }

    pub fn output(&self, cmd: &mut Command) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.output_bytes(cmd)?).into_owned())
    }

    pub fn output_bytes(&self, cmd: &mut Command) -> Result<Vec<u8>> {
        let output = cmd.output().with_context(|| format!("Starting {cmd:?}"))?;
        if !output.status.success() {
            bail!(
                "Command failed: {cmd:?}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(output.stdout)
    }

    pub fn cargo(&self, args: &[&str], release: bool, offline: bool) -> Result<()> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut cmd = self.command(cargo);
        cmd.args(args);
        if release {
            cmd.arg("--release");
        }
        if offline {
            cmd.arg("--offline");
        }
        self.run(&mut cmd)
    }
}
