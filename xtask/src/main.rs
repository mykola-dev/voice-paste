mod files;
mod layout;
mod native;
mod package;
mod process;
mod release;

use anyhow::{Context, Result, bail, ensure};
use process::Runner;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Options {
    action: String,
    release: bool,
    no_deploy: bool,
    offline: bool,
    check: bool,
    source: PathBuf,
    include: Option<PathBuf>,
    archive: Option<PathBuf>,
    tag: Option<String>,
}

fn options() -> Result<Options> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        action: args.next().unwrap_or_else(|| "help".into()),
        release: false,
        no_deploy: false,
        offline: false,
        check: false,
        source: layout::PREPARED_NATIVE.into(),
        include: None,
        archive: None,
        tag: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release" => options.release = true,
            "--no-deploy" => options.no_deploy = true,
            "--offline" => options.offline = true,
            "--check" => options.check = true,
            "--native-runtime-source" => {
                options.source = args.next().context("Missing native runtime folder")?.into()
            }
            "--ct2-include" => {
                options.include = Some(args.next().context("Missing CT2 include folder")?.into())
            }
            "--archive" => options.archive = Some(args.next().context("Missing ZIP path")?.into()),
            "--tag" => options.tag = Some(args.next().context("Missing release tag")?),
            _ => bail!("Unknown argument: {arg}. Run cargo xtask help"),
        }
    }
    ensure!(
        matches!(
            options.action.as_str(),
            "help"
                | "--help"
                | "-h"
                | "build"
                | "package"
                | "prepare"
                | "check"
                | "fmt"
                | "test"
                | "clippy"
                | "fetch"
                | "verify"
                | "release-notes"
        ),
        "Unknown task: {}",
        options.action
    );
    ensure!(
        options.tag.is_none() || options.action == "release-notes",
        "--tag applies to release-notes"
    );
    ensure!(
        !options.check || matches!(options.action.as_str(), "build" | "package"),
        "--check applies to build/package"
    );
    Ok(options)
}

#[cfg(windows)]
fn msvc(runner: &mut Runner) -> Result<PathBuf> {
    use std::os::windows::process::CommandExt;
    let installer = std::env::var_os("ProgramFiles(x86)").context("Missing ProgramFiles(x86)")?;
    let vswhere = Path::new(&installer).join("Microsoft Visual Studio/Installer/vswhere.exe");
    let vs = runner
        .output(runner.command(&vswhere).args([
            "-latest",
            "-utf8",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ]))
        .context(
            "Install Visual Studio C++ Build Tools and Windows SDK; xtask never installs SDKs",
        )?;
    let vs = PathBuf::from(vs.trim());
    ensure!(vs.is_absolute(), "MSVC x64 Build Tools not found");
    let batch = vs.join("VC/Auxiliary/Build/vcvars64.bat");
    let cmd_exe = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
    let mut cmd = runner.command(cmd_exe);
    cmd.args(["/d", "/u", "/s", "/c"])
        .raw_arg(format!("\"\"{}\" >nul && set\"", batch.display()));
    // cmd /u preserves non-ASCII profile/toolchain paths.
    let output = runner.output_bytes(&mut cmd)?;
    ensure!(output.len() % 2 == 0, "Invalid MSVC environment encoding");
    let environment = String::from_utf16(
        &output
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect::<Vec<_>>(),
    )?;
    for line in environment.lines() {
        if let Some((key, value)) = line.split_once('=')
            && !key.is_empty()
        {
            runner
                .env
                .insert(key.to_ascii_uppercase().into(), value.into());
        }
    }
    Ok(vs)
}

#[cfg(not(windows))]
fn msvc(_runner: &mut Runner) -> Result<PathBuf> {
    bail!(
        "Windows portable builds currently require a Windows x64 host with MSVC; no cross-compilation is implemented"
    )
}

fn cargo_task(runner: &Runner, action: &str, options: &Options) -> Result<()> {
    if action == "fmt" {
        runner.cargo(&["fmt", "--all", "--", "--check"], false, false)
    } else if action == "fetch" {
        runner.cargo(&["fetch", "--locked"], false, options.offline)
    } else {
        let mut args = cargo_workspace_args(action);
        if action == "clippy" {
            args.push("--all-targets");
        }
        // Trailing clippy arguments must follow cargo's profile/offline flags.
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut cmd = runner.command(cargo);
        cmd.args(args);
        if options.release {
            cmd.arg("--release");
        }
        if options.offline {
            cmd.arg("--offline");
        }
        if action == "clippy" {
            cmd.args(["--", "-D", "warnings"]);
        }
        runner.run(&mut cmd)
    }
}

fn cargo_workspace_args(action: &str) -> Vec<&str> {
    let mut args = vec![action, "--workspace", "--locked", "--jobs", "2"];
    if action == "build" {
        // Cargo already built the running xtask. Rebuilding that same EXE while
        // it is mapped on Windows fails with Access denied. It is not a product
        // binary; keep it in workspace test/clippy, exclude only nested builds.
        args.extend(["--exclude", "xtask"]);
    }
    args
}

fn run() -> Result<()> {
    let options = options()?;
    if options.action == "package" && std::env::var("GITHUB_REF_TYPE").as_deref() == Ok("tag") {
        ensure!(
            std::env::var("GITHUB_REF_NAME")? == format!("v{}", env!("CARGO_PKG_VERSION")),
            "Release tag must match workspace version v{}",
            env!("CARGO_PKG_VERSION")
        );
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("Missing repository root")?;
    let mut runner = Runner::new(root);
    match options.action.as_str() {
        "help" | "--help" | "-h" => {
            println!(
                "cargo xtask <build|package|prepare|check|fmt|test|clippy|fetch|verify|release-notes>\n\
                Options: --release --no-deploy --offline --check\n\
                Inputs: --native-runtime-source <folder> --ct2-include <folder>\n\
                Verify: cargo xtask verify --archive <zip>\n\
                Release notes: cargo xtask release-notes --tag vMAJOR.MINOR.PATCH\n\
                Outputs: build/debug or build/release; ZIP + SHA-256 in build/package; internals in build/cache.\n\
                Requires Rust; Windows builds also require MSVC/Windows SDK. No SDK installation or model downloads."
            );
            return Ok(());
        }
        "verify" => {
            return package::verify_archive(&files::absolute(
                root,
                options
                    .archive
                    .as_deref()
                    .context("verify requires --archive <zip>")?,
            ));
        }
        "release-notes" => {
            let notes = release::notes(
                &fs::read_to_string(root.join("Cargo.toml"))?,
                &fs::read_to_string(root.join("CHANGELOG.md"))?,
                options
                    .tag
                    .as_deref()
                    .context("release-notes requires --tag")?,
            )?;
            print!("{notes}");
            return Ok(());
        }
        "fmt" | "fetch" => return cargo_task(&runner, &options.action, &options),
        _ => (),
    }
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "Application builds currently support only Windows x64"
    );
    ensure!(
        std::env::var_os("CARGO_BUILD_TARGET").is_none(),
        "Unset CARGO_BUILD_TARGET: portable packaging uses native Windows x64 outputs, not a cross-target cache"
    );
    let vs = msvc(&mut runner)?;
    let source = files::absolute(root, &options.source);
    native::prepare(&runner, &source, &vs, options.offline)?;
    if options.action == "prepare" {
        return Ok(());
    }
    let include = options
        .include
        .as_ref()
        .map(|p| files::absolute(root, p))
        .unwrap_or_else(|| source.join("include"));
    ensure!(
        include.join("ctranslate2/translator.h").is_file(),
        "Matching CT2 headers missing"
    );
    let lib = root.join(layout::NATIVE_LIB);
    native::import_library(&runner, &source, &lib)?;
    runner
        .env
        .insert("VOICEPASTE_CT2_INCLUDE".into(), include.into_os_string());
    runner
        .env
        .insert("VOICEPASTE_CT2_LIB".into(), lib.into_os_string());
    let path = runner
        .env
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var_os("PATH"))
        .context("Missing PATH")?;
    let path =
        std::env::join_paths(std::iter::once(source.clone()).chain(std::env::split_paths(&path)))?;
    runner.env.insert("PATH".into(), path);
    // Keep the established cache/output contract regardless of caller environment.
    runner.env.insert(
        "CARGO_TARGET_DIR".into(),
        root.join(layout::TARGET).into_os_string(),
    );
    if options.check || options.action == "check" {
        for action in ["fmt", "test", "clippy"] {
            cargo_task(&runner, action, &options)?;
        }
        if options.action == "check" {
            return Ok(());
        }
    }
    let action = if options.action == "package" {
        "build"
    } else {
        &options.action
    };
    cargo_task(&runner, action, &options)?;
    if options.action == "package" || (options.action == "build" && !options.no_deploy) {
        fs::create_dir_all(root.join("build"))?;
        package::package(
            &runner,
            &source,
            options.release,
            !options.no_deploy,
            options.action == "package",
        )?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Build failed: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod task_tests {
    use super::*;

    #[test]
    fn nested_build_does_not_overwrite_running_xtask_but_checks_still_cover_it() {
        let build = cargo_workspace_args("build");
        assert!(build.windows(2).any(|args| args == ["--exclude", "xtask"]));
        for action in ["test", "clippy"] {
            let args = cargo_workspace_args(action);
            assert!(args.contains(&"--workspace"));
            assert!(!args.contains(&"--exclude"));
        }
    }
}
