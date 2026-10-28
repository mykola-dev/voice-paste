mod engine;
mod frontend;
mod pipeline;
mod vad;
use std::{io, path::PathBuf};
use voicepaste_protocol::{Request, Response, VERSION, read, write};

fn run() -> Result<(), String> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let Some(Request::Initialize {
        version,
        generation,
        model,
        audio_root,
        options,
    }) = read(&mut input)?
    else {
        return Err("Initialize must be the first worker message".into());
    };
    if version != VERSION {
        return Err("Unsupported IPC version".into());
    }
    let audio_root = audio_root.canonicalize().map_err(|e| e.to_string())?;
    let runtime: PathBuf = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Worker has no executable directory")?
        .to_owned();
    let mut pipeline = match pipeline::Pipeline::open(&model, &runtime, options) {
        Ok(pipeline) => pipeline,
        Err(message) => {
            write(
                &mut output,
                &Response::Error {
                    generation,
                    id: None,
                    message,
                },
            )?;
            return Ok(());
        }
    };
    write(
        &mut output,
        &Response::Ready {
            version: VERSION,
            generation,
            device: pipeline.engine.device.into(),
            mels: pipeline.engine.mels,
        },
    )?;
    while let Some(request) = read(&mut input)? {
        match request {
            Request::Shutdown => break,
            Request::Transcribe {
                generation: requested_generation,
                id,
                session,
                audio,
            } => {
                if requested_generation != generation {
                    return Err("Worker generation mismatch".into());
                }
                let result = audio
                    .canonicalize()
                    .map_err(|e| e.to_string())
                    .and_then(|audio| {
                        if !audio.starts_with(&audio_root) || audio == audio_root {
                            return Err("Audio path is outside owned session data".into());
                        }
                        pipeline.transcribe(&audio)
                    });
                let response = match result {
                    Ok(text) => Response::Result {
                        generation,
                        id,
                        session,
                        text,
                    },
                    Err(message) => Response::Error {
                        generation,
                        id: Some(id),
                        message,
                    },
                };
                write(&mut output, &response)?;
            }
            Request::Initialize { .. } => return Err("Worker is already initialized".into()),
        }
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Native worker: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
