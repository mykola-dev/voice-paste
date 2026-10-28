use crate::{
    engine::{Encoded, Engine, Generation},
    frontend,
    vad::Vad,
};
use flate2::{Compression, write::ZlibEncoder};
use std::{collections::BTreeSet, io::Write, path::Path};
use tokenizers::Tokenizer;
use voicepaste_core::settings::{Language, TranscriptionOptions};

pub struct Pipeline {
    pub engine: Engine,
    tokenizer: Tokens,
    vad: Option<Vad>,
    options: TranscriptionOptions,
}

impl Pipeline {
    pub fn open(
        model: &Path,
        runtime: &Path,
        options: TranscriptionOptions,
    ) -> Result<Self, String> {
        if !(1..=10).contains(&options.beam_size) {
            return Err("Beam size must be 1..10".into());
        }
        let tokenizer = Tokens::open(&model.join("tokenizer.json"), options.language_mode)?;
        let vad = if options.enable_vad {
            Some(Vad::open(
                &runtime.join("silero_vad_v6.onnx"),
                &runtime.join("onnxruntime.dll"),
            )?)
        } else {
            None
        };
        let engine = Engine::open(model, options.device)?;
        Ok(Self {
            engine,
            tokenizer,
            vad,
            options,
        })
    }

    pub fn transcribe(&mut self, path: &Path) -> Result<String, String> {
        let mut audio = frontend::read_wav(path)?;
        if audio.len() < 2 {
            return Ok(String::new());
        }
        if let Some(vad) = &mut self.vad {
            audio = vad.filter(&audio)?;
        }
        if audio.len() < 2 {
            return Ok(String::new());
        }
        let features = frontend::features(&audio, self.engine.mels)?;
        drop(audio);
        let mut seek = 0;
        let mut language = match self.options.language_mode {
            Language::En => Some("<|en|>".to_string()),
            Language::Ua => Some("<|uk|>".to_string()),
            _ => None,
        };
        let mut history = self.tokenizer.encode(&initial_prompt(&self.options))?;
        let mut segments = Vec::new();
        while seek < features.content_frames() {
            let size = frontend::WINDOW.min(features.content_frames() - seek);
            let encoded = self.engine.encode(&features.window(seek, size))?;
            if language.is_none() {
                let detected = encoded.language()?;
                language = Some(
                    if self.options.language_mode == Language::Bilingual && detected == "<|ru|>" {
                        "<|uk|>".into()
                    } else {
                        detected
                    },
                );
            }
            let prompt = self
                .tokenizer
                .prompt(&history, language.as_deref().ok_or("Missing language")?)?;
            let (generated, average, temperature) = self.generate(&encoded, &prompt)?;
            if generated.no_speech > 0.6 && average <= -1.0 {
                seek += size;
                continue;
            }
            let (chunks, advance) =
                split_timestamps(&generated.ids, self.tokenizer.timestamp, size)?;
            for chunk in chunks {
                let text = self.tokenizer.decode(chunk)?;
                if !text.trim().is_empty() {
                    segments.push(text.trim().to_owned());
                    history.extend_from_slice(chunk);
                }
            }
            seek += advance;
            if temperature > 0.5 {
                history.clear();
            }
            // Only the most recent 223 tokens can enter the next prompt.
            if history.len() > 223 {
                history.drain(..history.len() - 223);
            }
        }
        let text = segments.join(" ");
        Ok(
            if matches!(
                self.options.language_mode,
                Language::Ua | Language::Bilingual
            ) {
                ukrainian_cleanup(&text)
            } else {
                text
            },
        )
    }

    fn generate(
        &self,
        encoded: &Encoded<'_>,
        prompt: &[u32],
    ) -> Result<(Generation, f32, f32), String> {
        let mut best = None;
        let mut best_below_compression = None;
        for temperature in [0.0, 0.2, 0.4, 0.6, 0.8, 1.0] {
            let result = encoded.generate(
                prompt,
                &self.tokenizer.suppressed,
                self.options.beam_size,
                temperature,
            )?;
            let average = result.score * result.ids.len() as f32 / (result.ids.len() + 1) as f32;
            let text = self.tokenizer.decode(&result.ids)?;
            let ratio = compression_ratio(text.trim())?;
            let fallback =
                (ratio > 2.4 || average < -1.0) && !(result.no_speech > 0.6 && average < -1.0);
            if !fallback {
                return Ok((result, average, temperature));
            }
            let candidate = (result, average, temperature);
            if ratio <= 2.4
                && best_below_compression
                    .as_ref()
                    .is_none_or(|(_, score, _)| average > *score)
            {
                best_below_compression = Some((
                    Generation {
                        ids: candidate.0.ids.clone(),
                        score: candidate.0.score,
                        no_speech: candidate.0.no_speech,
                    },
                    average,
                    temperature,
                ));
            }
            if best.as_ref().is_none_or(|(_, score, _)| average > *score) {
                best = Some(candidate);
            }
        }
        let (result, average, _) = best_below_compression.or(best).ok_or("No decoder result")?;
        Ok((result, average, 1.0))
    }
}

fn compression_ratio(text: &str) -> Result<f32, String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let length = encoder.finish().map_err(|e| e.to_string())?.len();
    Ok(text.len() as f32 / length as f32)
}

fn initial_prompt(options: &TranscriptionOptions) -> String {
    let custom = options.custom_initial_prompt.trim();
    let bilingual = "English and Ukrainian. Transcribe in these languages. Англійська та українська мови. ґ, є, і, ї.";
    let combined = if options.language_mode == Language::Bilingual {
        if custom.is_empty() {
            bilingual.to_owned()
        } else {
            format!("{bilingual} {custom}")
        }
    } else {
        custom.to_owned()
    };
    if combined.is_empty() {
        combined
    } else {
        format!(" {combined}")
    }
}

fn ukrainian_cleanup(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'ы' => 'и',
            'Ы' => 'И',
            'э' | 'ё' => 'е',
            'Э' | 'Ё' => 'Е',
            'ъ' | 'Ъ' => '\'',
            c => c,
        })
        .collect()
}

fn split_timestamps(
    ids: &[u32],
    timestamp: u32,
    size: usize,
) -> Result<(Vec<&[u32]>, usize), String> {
    let single_ending =
        ids.len() >= 2 && ids[ids.len() - 2] < timestamp && ids[ids.len() - 1] >= timestamp;
    let mut boundaries: Vec<_> = (1..ids.len())
        .filter(|&i| ids[i] >= timestamp && ids[i - 1] >= timestamp)
        .collect();
    if boundaries.is_empty() {
        return Ok((vec![ids], size));
    }
    if single_ending {
        boundaries.push(ids.len());
    }
    let mut start = 0;
    let mut chunks = Vec::new();
    for end in boundaries {
        chunks.push(&ids[start..end]);
        start = end;
    }
    let advance = if single_ending {
        size
    } else {
        (ids[start - 1] - timestamp) as usize * 2
    };
    if advance == 0 || advance > size {
        return Err("Invalid decoder timestamp seek; audio was not silently skipped".into());
    }
    Ok((chunks, advance))
}

struct Tokens {
    tokenizer: Tokenizer,
    eot: u32,
    timestamp: u32,
    suppressed: Vec<i32>,
}
impl Tokens {
    fn open(path: &Path, language: Language) -> Result<Self, String> {
        let tokenizer = Tokenizer::from_file(path).map_err(|e| e.to_string())?;
        let eot = tokenizer
            .token_to_id("<|endoftext|>")
            .ok_or("Missing end-of-text token")?;
        let timestamp = tokenizer
            .token_to_id("<|notimestamps|>")
            .ok_or("Missing timestamp token")?
            + 1;
        let mut this = Self {
            tokenizer,
            eot,
            timestamp,
            suppressed: Vec::new(),
        };
        let mut suppressed = BTreeSet::new();
        for text in [" -", " '"] {
            if let Some(&id) = this.encode(text)?.first() {
                suppressed.insert(id as i32);
            }
        }
        let symbols = "\"#()*+/:;<=>@[\\]^_`{|}~「」『』"
            .chars()
            .map(|c| c.to_string())
            .chain(
                "<< >> <<< >>> -- --- -( -[ (' (\" (( )) ((( ))) [[ ]] {{ }} ♪♪ ♪♪♪"
                    .split_whitespace()
                    .map(str::to_owned),
            )
            .chain("♩♪♫♬♭♮♯".chars().map(|c| c.to_string()));
        for symbol in symbols {
            for text in [symbol.clone(), format!(" {symbol}")] {
                let ids = this.encode(&text)?;
                if (ids.len() == 1 || "♩♪♫♬♭♮♯".contains(&symbol))
                    && let Some(&id) = ids.first()
                {
                    suppressed.insert(id as i32);
                }
            }
        }
        for special in [
            "<|transcribe|>",
            "<|translate|>",
            "<|startoftranscript|>",
            "<|startofprev|>",
            "<|startoflm|>",
        ] {
            suppressed.insert(this.id(special)? as i32);
        }
        let no_speech = this
            .tokenizer
            .token_to_id("<|nospeech|>")
            .or_else(|| this.tokenizer.token_to_id("<|nocaptions|>"))
            .ok_or("Missing no-speech tokenizer token")?;
        suppressed.insert(no_speech as i32);
        if matches!(language, Language::Ua | Language::Bilingual) {
            for id in 0..this.tokenizer.get_vocab_size(true) as u32 {
                let text = this
                    .tokenizer
                    .decode(&[id], true)
                    .map_err(|e| e.to_string())?;
                if text.chars().any(|c| "ыэъёЫЭЪЁ".contains(c)) {
                    suppressed.insert(id as i32);
                }
            }
        }
        this.suppressed = suppressed.into_iter().collect();
        Ok(this)
    }
    fn id(&self, name: &str) -> Result<u32, String> {
        self.tokenizer
            .token_to_id(name)
            .ok_or_else(|| format!("Missing tokenizer token {name}"))
    }
    fn encode(&self, text: &str) -> Result<Vec<u32>, String> {
        self.tokenizer
            .encode(text, false)
            .map(|v| v.get_ids().to_vec())
            .map_err(|e| e.to_string())
    }
    fn decode(&self, ids: &[u32]) -> Result<String, String> {
        self.tokenizer
            .decode(
                &ids.iter()
                    .copied()
                    .filter(|&id| id < self.eot)
                    .collect::<Vec<_>>(),
                true,
            )
            .map_err(|e| e.to_string())
    }
    fn prompt(&self, history: &[u32], language: &str) -> Result<Vec<u32>, String> {
        let mut prompt = Vec::new();
        if !history.is_empty() {
            prompt.push(self.id("<|startofprev|>")?);
            prompt.extend_from_slice(&history[history.len().saturating_sub(223)..]);
        }
        prompt.extend([
            self.id("<|startoftranscript|>")?,
            self.id(language)?,
            self.id("<|transcribe|>")?,
        ]);
        Ok(prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeks_to_completed_timestamp_not_fixed_chunks() {
        let ids = [100, 1, 105, 105, 2, 110];
        let (chunks, seek) = split_timestamps(&ids, 100, 3000).unwrap();
        assert_eq!(chunks, vec![&ids[..3], &ids[3..]]);
        assert_eq!(seek, 3000);
        let unfinished = [100, 1, 105, 105, 2];
        let (chunks, seek) = split_timestamps(&unfinished, 100, 3000).unwrap();
        assert_eq!(chunks, vec![&unfinished[..3]]);
        assert_eq!(seek, 10);
    }
    #[test]
    fn ua_cleanup_and_custom_prompt_are_preserved() {
        assert_eq!(ukrainian_cleanup("ыЫэЭёЁъЪ"), "иИеЕеЕ''");
        let settings = voicepaste_core::settings::Settings::initial().unwrap();
        let mut options = TranscriptionOptions::from(&settings);
        assert_eq!(initial_prompt(&options), "");
        options.custom_initial_prompt = " Example vocabulary\r\nПриклад словника \r\n".into();
        assert_eq!(
            initial_prompt(&options),
            " Example vocabulary\r\nПриклад словника"
        );
    }
}
