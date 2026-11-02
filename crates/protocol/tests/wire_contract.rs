use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use voicepaste_core::{
    SessionId,
    settings::{Device, Language, TranscriptionOptions},
    statistics::Measurement,
};
use voicepaste_protocol::{Generation, Request, RequestId, Response, VERSION, read, write};

// These fixtures pin JSON structure, not object key ordering or whitespace.
const INITIALIZE: &str = r#"{
    "type":"initialize", "version":3, "generation":17,
    "model":"models/large", "audio_root":"audio",
    "options":{"device":"cudaAuto", "languageMode":"bilingual", "enableVad":true,
               "customInitialPrompt":"Привіт, 世界 🎙️", "beamSize":5}
}"#;
const TRANSCRIBE: &str = r#"{
    "type":"transcribe", "generation":17, "id":42, "session":99, "audio":"audio/clip.wav"
}"#;
const READY: &str = r#"{
    "type":"ready", "version":3, "generation":17, "device":"cuda", "mels":128,
    "model_loading_us":2500000
}"#;
const RESULT: &str = r#"{
    "type":"result", "generation":17, "id":42, "session":99, "text":"Привіт, 世界!",
    "measurement":{"inference_us":1234, "audio_samples":16000, "vad_us":250,
                   "preparation_us":500}
}"#;

fn assert_fixture<T: Serialize + DeserializeOwned>(message: &T, fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let mut bytes = Vec::new();
    write(&mut bytes, message).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);

    let mut framed = serde_json::to_vec(&expected).unwrap();
    framed.push(b'\n');
    let decoded: T = read(&mut framed.as_slice()).unwrap().unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
}

fn initialize(device: Device, language_mode: Language) -> Request {
    Request::Initialize {
        version: VERSION,
        generation: Generation(17),
        model: "models/large".into(),
        audio_root: "audio".into(),
        options: TranscriptionOptions {
            device,
            language_mode,
            enable_vad: true,
            custom_initial_prompt: "Привіт, 世界 🎙️".into(),
            beam_size: 5,
        },
    }
}

#[test]
fn pinned_request_and_response_fixtures() {
    assert_eq!(VERSION, 3);
    assert_fixture(
        &initialize(Device::CudaAuto, Language::Bilingual),
        INITIALIZE,
    );
    assert_fixture(
        &Request::Transcribe {
            generation: Generation(17),
            id: RequestId(42),
            session: SessionId(99),
            audio: "audio/clip.wav".into(),
        },
        TRANSCRIBE,
    );
    assert_fixture(
        &Response::Ready {
            version: VERSION,
            generation: Generation(17),
            device: "cuda".into(),
            mels: 128,
            model_loading_us: 2_500_000,
        },
        READY,
    );
    assert_fixture(
        &Response::Result {
            generation: Generation(17),
            id: RequestId(42),
            session: SessionId(99),
            text: "Привіт, 世界!".into(),
            measurement: Measurement {
                inference_us: 1234,
                audio_samples: 16000,
                vad_us: Some(250),
                preparation_us: Some(500),
            },
        },
        RESULT,
    );
}

#[test]
fn initialization_pins_all_device_and_language_spellings() {
    for (device, device_wire) in [
        (Device::Cpu, "cpu"),
        (Device::CudaOnly, "cudaOnly"),
        (Device::CudaAuto, "cudaAuto"),
        (Device::MetalOnly, "metalOnly"),
    ] {
        for (language, language_wire) in [
            (Language::Auto, "auto"),
            (Language::En, "en"),
            (Language::Ua, "ua"),
            (Language::Bilingual, "bilingual"),
        ] {
            let mut expected: Value = serde_json::from_str(INITIALIZE).unwrap();
            expected["options"]["device"] = json!(device_wire);
            expected["options"]["languageMode"] = json!(language_wire);
            assert_fixture(&initialize(device, language), &expected.to_string());
        }
    }
}

#[test]
fn result_pins_absent_optional_measurements_as_null() {
    let mut fixture: Value = serde_json::from_str(RESULT).unwrap();
    fixture["measurement"]["vad_us"] = Value::Null;
    fixture["measurement"]["preparation_us"] = Value::Null;
    assert_fixture(
        &Response::Result {
            generation: Generation(17),
            id: RequestId(42),
            session: SessionId(99),
            text: "Привіт, 世界!".into(),
            measurement: Measurement {
                inference_us: 1234,
                audio_samples: 16000,
                vad_us: None,
                preparation_us: None,
            },
        },
        &fixture.to_string(),
    );
}

#[test]
fn malformed_messages_fail_to_parse_but_versions_are_checked_elsewhere() {
    for malformed in [
        "{broken}\n",
        "{\"type\":\"unknown\"}\n",
        "{\"type\":\"transcribe\",\"generation\":17,\"id\":42,\"session\":99,\"audio\":\"audio/clip.wav\",\"extra\":true}\n",
        "{\"type\":\"transcribe\",\"generation\":17}\n",
        "{\"type\":\"shutdown\"}",
    ] {
        assert!(
            read::<Request>(&mut malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
    let mut bad_result: Value = serde_json::from_str(RESULT).unwrap();
    bad_result["measurement"]["unexpected"] = json!(1);
    let framed = format!("{bad_result}\n");
    assert!(read::<Response>(&mut framed.as_bytes()).is_err());

    // Parsing preserves the peer's version; the host/worker decide compatibility.
    let mut mismatched: Value = serde_json::from_str(INITIALIZE).unwrap();
    mismatched["version"] = json!(VERSION + 1);
    let framed = format!("{mismatched}\n");
    assert!(
        matches!(read::<Request>(&mut framed.as_bytes()).unwrap(), Some(Request::Initialize { version, .. }) if version == VERSION + 1)
    );
    let mut mismatched: Value = serde_json::from_str(READY).unwrap();
    mismatched["version"] = json!(VERSION + 1);
    let framed = format!("{mismatched}\n");
    assert!(
        matches!(read::<Response>(&mut framed.as_bytes()).unwrap(), Some(Response::Ready { version, .. }) if version == VERSION + 1)
    );
}
