//! Shared CLI routing, model preparation and application composition scenario.
#[cfg(target_os = "windows")]
#[path = "adapters/platform/windows/entry.rs"]
mod native;
#[cfg(target_os = "macos")]
use crate::adapters::platform as native;
use crate::{
    adapters::{models::Preparation, storage::Storage, worker::Worker},
    runtime::Runtime,
};
use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

/// Preserve existing command-line contracts as data, not separate workflows.
pub(crate) struct CliPolicy {
    pub check_message: Option<&'static str>,
    pub max_files: Option<usize>,
    pub download_progress: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Resident,
    Settings,
    Capture,
    Check,
    Download,
    Files(Vec<PathBuf>),
}

fn parse(args: Vec<OsString>, policy: &CliPolicy) -> Result<Mode, String> {
    if args.is_empty() {
        return Ok(Mode::Resident);
    }
    if args == ["--settings"] {
        return Ok(Mode::Settings);
    }
    if args == ["--capture-preview"] {
        return Ok(Mode::Capture);
    }
    if args == ["--download-model"] {
        return Ok(Mode::Download);
    }
    if args == ["--check"] && policy.check_message.is_some() {
        return Ok(Mode::Check);
    }
    if args[0] == "--transcribe-file"
        && args.len() > 1
        && policy.max_files.is_none_or(|max| args.len() - 1 <= max)
    {
        return Ok(Mode::Files(
            args.into_iter().skip(1).map(PathBuf::from).collect(),
        ));
    }
    Err(format!(
        "Usage: VoicePaste [{}--settings | --download-model | --capture-preview | --transcribe-file <wav>]",
        if policy.check_message.is_some() {
            "--check | "
        } else {
            ""
        }
    ))
}

fn prepare(storage: &Storage, download: bool, progress: bool) -> Result<(), String> {
    let mut job =
        Preparation::start_with_download(storage.root.clone(), &storage.settings.model, download)?;
    loop {
        if let Some(result) = job.poll() {
            let directory = result?;
            if progress {
                println!("Model ready: {}", directory.display());
            }
            return Ok(());
        }
        if progress {
            println!("{}", job.progress.snapshot());
        }
        std::thread::sleep(Duration::from_millis(if progress { 250 } else { 50 }));
    }
}

pub(crate) fn run(
    validate_settings: fn(&voicepaste_core::settings::Settings) -> Result<(), String>,
) -> Result<(), String> {
    let policy = native::CLI_POLICY;
    let mode = parse(std::env::args_os().skip(1).collect(), &policy)?;
    let storage = Arc::new(native::open_storage()?);
    match mode {
        Mode::Check => {
            println!("{}", policy.check_message.ok_or("Check is unavailable")?);
            Ok(())
        }
        Mode::Download => prepare(&storage, true, policy.download_progress),
        Mode::Files(files) => {
            // The explicit file route never downloads. Verify the same pinned
            // artifacts before native inference on every backend.
            prepare(&storage, false, false)?;
            let mut worker = Worker::new(storage.clone());
            for file in files {
                println!("{}", worker.transcribe_file(&file, &storage.settings)?);
            }
            Ok(())
        }
        Mode::Resident | Mode::Settings | Mode::Capture => {
            let settings_only = mode == Mode::Settings;
            let capture = mode == Mode::Capture;
            let runtime =
                Runtime::start(storage.clone(), capture || settings_only, validate_settings)?;
            let result = native::run_session(runtime, storage.clone(), capture, settings_only);
            if let Err(error) = &result {
                storage.log(error);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn command_contracts_share_routing_and_preserve_explicit_argument_limits() {
        for policy in [
            CliPolicy {
                check_message: Some("valid"),
                max_files: None,
                download_progress: true,
            },
            CliPolicy {
                check_message: None,
                max_files: Some(1),
                download_progress: false,
            },
        ] {
            assert_eq!(parse(vec![], &policy).unwrap(), Mode::Resident);
            assert_eq!(
                parse(args(&["--settings"]), &policy).unwrap(),
                Mode::Settings
            );
            assert_eq!(
                parse(args(&["--capture-preview"]), &policy).unwrap(),
                Mode::Capture
            );
            assert_eq!(
                parse(args(&["--download-model"]), &policy).unwrap(),
                Mode::Download
            );
            assert_eq!(
                parse(args(&["--check"]), &policy).is_ok(),
                policy.check_message.is_some()
            );
            assert_eq!(
                parse(args(&["--transcribe-file", "audio Україна.wav"]), &policy).unwrap(),
                Mode::Files(vec![PathBuf::from("audio Україна.wav")])
            );
            assert_eq!(
                parse(args(&["--transcribe-file", "one.wav", "two.wav"]), &policy).is_ok(),
                policy.max_files.is_none()
            );
            for invalid in [
                &["--transcribe-file"][..],
                &["--settings", "extra"],
                &["--unknown"],
            ] {
                assert!(parse(args(invalid), &policy).is_err());
            }
        }
    }
}
