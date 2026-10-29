use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
};
use voicepaste_core::{SessionId, settings::TranscriptionOptions};

pub const VERSION: u32 = 3;
pub const MAX_MESSAGE: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generation(pub u64);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Initialize {
        version: u32,
        generation: Generation,
        model: PathBuf,
        audio_root: PathBuf,
        options: TranscriptionOptions,
    },
    Transcribe {
        generation: Generation,
        id: RequestId,
        session: SessionId,
        audio: PathBuf,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Ready {
        version: u32,
        generation: Generation,
        device: String,
        mels: usize,
        model_loading_us: u64,
    },
    Result {
        generation: Generation,
        id: RequestId,
        session: SessionId,
        text: String,
        measurement: voicepaste_core::statistics::Measurement,
    },
    Error {
        generation: Generation,
        id: Option<RequestId>,
        message: String,
    },
}

pub fn read<T: DeserializeOwned>(reader: &mut impl BufRead) -> Result<Option<T>, String> {
    let mut line = Vec::new();
    std::io::Read::take(reader, (MAX_MESSAGE + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if line.is_empty() {
        return Ok(None);
    }
    if line.len() > MAX_MESSAGE || line.last() != Some(&b'\n') {
        return Err("Oversized or unterminated IPC message".into());
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|e| format!("Invalid IPC message: {e}"))
}

pub fn write(writer: &mut impl Write, message: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if bytes.len() >= MAX_MESSAGE {
        return Err("IPC response too large".into());
    }
    writer
        .write_all(&bytes)
        .and_then(|()| writer.write_all(b"\n"))
        .and_then(|()| writer.flush())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_transports_one_real_model_load_measurement() {
        let response = Response::Ready {
            version: VERSION,
            generation: Generation(9),
            device: "cuda".into(),
            mels: 128,
            model_loading_us: 2_500_000,
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &response).unwrap();
        assert!(matches!(
            read::<Response>(&mut bytes.as_slice()).unwrap(),
            Some(Response::Ready {
                version: VERSION,
                generation: Generation(9),
                model_loading_us: 2_500_000,
                ..
            })
        ));
    }
    #[test]
    fn bounded_utf8_framing() {
        let value = Response::Result {
            generation: Generation(3),
            id: RequestId(2),
            session: SessionId(1),
            text: "Привіт, бро".into(),
            measurement: voicepaste_core::statistics::Measurement {
                inference_us: 1234,
                audio_samples: 16000,
                vad_us: Some(250),
                preparation_us: Some(500),
            },
        };
        let mut bytes = Vec::new();
        write(&mut bytes, &value).unwrap();
        assert!(
            matches!(read::<Response>(&mut bytes.as_slice()).unwrap(), Some(Response::Result { text, measurement, .. }) if text == "Привіт, бро" && measurement.inference_us == 1234 && measurement.audio_samples == 16000 && measurement.vad_us == Some(250) && measurement.preparation_us == Some(500))
        );
        // A legacy text-only result must not fabricate a zero-duration sample.
        assert!(read::<Response>(&mut b"{\"type\":\"result\",\"generation\":3,\"id\":2,\"session\":1,\"text\":\"hello\"}\n".as_slice()).is_err());
        assert!(read::<Request>(&mut b"{broken}\n".as_slice()).is_err());
        assert!(read::<Request>(&mut b"{\"type\":\"shutdown\"}".as_slice()).is_err());
        assert!(read::<Request>(&mut vec![b'x'; MAX_MESSAGE + 1].as_slice()).is_err());
    }
}
