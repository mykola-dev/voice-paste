use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, bounded};
use rubato::{FftFixedIn, Resampler};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Instant,
};
use voicepaste_application::AudioCapture;
use voicepaste_core::SessionId;
use voicepaste_core::waveform::Measurement;

const BLOCK: usize = 1024;
const QUEUE_BLOCKS: usize = 128;

struct Block {
    samples: [f32; BLOCK],
    len: usize,
}

pub struct AudioFile {
    pub path: PathBuf,
}
impl AudioFile {
    pub fn remove(self) -> Result<(), String> {
        fs::remove_file(&self.path).map_err(|e| format!("Cannot remove recording: {e}"))
    }
}
impl Drop for AudioFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct Recording {
    stream: Option<cpal::Stream>,
    writer: Option<JoinHandle<Result<AudioFile, String>>>,
    pub failed: Arc<AtomicBool>,
}

impl Recording {
    fn finish(mut self) -> Result<AudioFile, String> {
        drop(self.stream.take()); // stop callbacks and close the producer first
        let result = self
            .writer
            .take()
            .ok_or("Missing audio writer")?
            .join()
            .map_err(|_| "Audio writer panicked")??;
        if self.failed.load(Ordering::Acquire) {
            return Err("Microphone failed or audio queue overflowed; recording discarded".into());
        }
        Ok(result)
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        drop(self.stream.take());
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

pub struct Microphone {
    temp: PathBuf,
    meter: Sender<Measurement>,
    clock: Instant,
}
impl Microphone {
    pub fn new(temp: PathBuf, meter: Sender<Measurement>, clock: Instant) -> Self {
        Self { temp, meter, clock }
    }
}

impl AudioCapture for Microphone {
    type Recording = Recording;
    type Audio = AudioFile;
    fn start(&mut self, session: SessionId) -> Result<Recording, String> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("No default microphone")?;
        let supported = device.default_input_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        if channels == 0 || !(8000..=384000).contains(&config.sample_rate.0) {
            return Err("Unsupported microphone sample rate/channel count".into());
        }
        let path = self.temp.join(format!(
            "recording-{}-{}.wav",
            std::process::id(),
            session.0
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).map_err(|e| e.to_string())?;
        let owned = AudioFile { path };
        let (sender, receiver) = bounded(QUEUE_BLOCKS);
        let failed = Arc::new(AtomicBool::new(false));
        let writer_failed = failed.clone();
        let rate = config.sample_rate.0;
        let writer = thread::Builder::new()
            .name("audio-writer".into())
            .spawn(move || {
                let result = write_audio(file, &owned.path, receiver, rate);
                if let Err(error) = result {
                    writer_failed.store(true, Ordering::Release);
                    return Err(error);
                }
                Ok(owned)
            })
            .map_err(|e| e.to_string())?;
        let mut recording = Recording {
            stream: None,
            writer: Some(writer),
            failed: failed.clone(),
        };
        let meter = Meter {
            sender: self.meter.clone(),
            clock: self.clock,
            session,
        };
        let stream = match format {
            cpal::SampleFormat::F32 => {
                input_stream::<f32>(&device, &config, sender, failed.clone(), channels, meter)
            }
            cpal::SampleFormat::I16 => {
                input_stream::<i16>(&device, &config, sender, failed.clone(), channels, meter)
            }
            cpal::SampleFormat::U16 => {
                input_stream::<u16>(&device, &config, sender, failed.clone(), channels, meter)
            }
            cpal::SampleFormat::I32 => {
                input_stream::<i32>(&device, &config, sender, failed.clone(), channels, meter)
            }
            _ => {
                drop(sender);
                return Err(format!("Unsupported microphone sample format: {format}"));
            }
        }?;
        recording.stream = Some(stream);
        recording
            .stream
            .as_ref()
            .ok_or("Missing microphone stream")?
            .play()
            .map_err(|e| e.to_string())?;
        Ok(recording)
    }
    fn finish(&mut self, recording: Recording) -> Result<AudioFile, String> {
        recording.finish()
    }
    fn discard(&mut self, recording: Recording) -> Result<(), String> {
        recording.finish()?.remove()
    }
    fn failed(&self, recording: &Recording) -> bool {
        recording.failed.load(Ordering::Acquire)
    }
}

struct Meter {
    sender: Sender<Measurement>,
    clock: Instant,
    session: SessionId,
}

fn input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sender: Sender<Block>,
    failed: Arc<AtomicBool>,
    channels: usize,
    meter: Meter,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let error = failed.clone();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let timestamp = meter.clock.elapsed();
                if failed.load(Ordering::Acquire) {
                    return;
                }
                if !data.len().is_multiple_of(channels) {
                    failed.store(true, Ordering::Release);
                    return;
                }
                // Fixed stack blocks and a preallocated bounded channel: no file IO,
                // logging, blocking sends or per-callback heap allocation.
                let mut squares = 0.0f64;
                for chunk in data.chunks(BLOCK * channels) {
                    let mut block = Block {
                        samples: [0.0; BLOCK],
                        len: chunk.len() / channels,
                    };
                    for (output, frame) in
                        block.samples.iter_mut().zip(chunk.chunks_exact(channels))
                    {
                        *output = frame
                            .iter()
                            .map(|v| <f32 as cpal::FromSample<T>>::from_sample_(*v))
                            .sum::<f32>()
                            / channels as f32;
                        if !output.is_finite() {
                            failed.store(true, Ordering::Release);
                            return;
                        }
                        squares += f64::from(*output).powi(2);
                    }
                    if sender.try_send(block).is_err() {
                        failed.store(true, Ordering::Release);
                        return;
                    }
                }
                if !data.is_empty() {
                    // Display handoff may drop a measurement when full; unlike
                    // PCM, it never aborts capture or blocks the audio callback.
                    let level = (squares / (data.len() / channels) as f64).sqrt().min(1.0) as f32;
                    let _ = meter.sender.try_send(Measurement {
                        session: meter.session,
                        timestamp,
                        level,
                    });
                }
            },
            move |_| {
                error.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|e| e.to_string())
}

struct Converter {
    resampler: Option<FftFixedIn<f32>>,
    pending: Vec<f32>,
    output: VecDeque<f32>,
    skip: usize,
    rate: u32,
    input: u64,
    written: u64,
}

impl Converter {
    fn new(rate: u32) -> Result<Self, String> {
        let resampler = if rate == 16000 {
            None
        } else {
            Some(
                FftFixedIn::<f32>::new(rate as usize, 16000, BLOCK, 2, 1)
                    .map_err(|e| e.to_string())?,
            )
        };
        let skip = resampler.as_ref().map_or(0, Resampler::output_delay);
        Ok(Self {
            resampler,
            pending: Vec::with_capacity(BLOCK),
            output: VecDeque::new(),
            skip,
            rate,
            input: 0,
            written: 0,
        })
    }
    fn push(
        &mut self,
        samples: &[f32],
        mut write: impl FnMut(i16) -> Result<(), String>,
    ) -> Result<(), String> {
        self.input += samples.len() as u64;
        if self.resampler.is_none() {
            for value in samples {
                write(pcm16(*value))?;
                self.written += 1;
            }
            return Ok(());
        }
        for value in samples {
            self.pending.push(*value);
            if self.pending.len() == BLOCK {
                self.process(false)?;
            }
        }
        self.drain(&mut write)
    }
    fn process(&mut self, partial: bool) -> Result<(), String> {
        let resampler = self.resampler.as_mut().ok_or("Missing resampler")?;
        let output = if partial && self.pending.is_empty() {
            resampler.process_partial::<&[f32]>(None, None)
        } else if partial {
            resampler.process_partial(Some(&[self.pending.as_slice()]), None)
        } else {
            resampler.process(&[self.pending.as_slice()], None)
        }
        .map_err(|e| e.to_string())?;
        self.pending.clear();
        for &value in &output[0] {
            if self.skip > 0 {
                self.skip -= 1;
            } else {
                self.output.push_back(value);
            }
        }
        Ok(())
    }
    fn drain(&mut self, write: &mut impl FnMut(i16) -> Result<(), String>) -> Result<(), String> {
        let target = self.input * 16000 / self.rate as u64;
        while self.written < target {
            let Some(value) = self.output.pop_front() else {
                break;
            };
            write(pcm16(value))?;
            self.written += 1;
        }
        Ok(())
    }
    fn finish(&mut self, mut write: impl FnMut(i16) -> Result<(), String>) -> Result<(), String> {
        if self.resampler.is_none() {
            return Ok(());
        }
        let target = self.input * 16000 / self.rate as u64;
        for _ in 0..10 {
            self.process(true)?;
            self.drain(&mut write)?;
            if self.written == target {
                return Ok(());
            }
        }
        Err("Resampler tail could not be drained".into())
    }
}

fn pcm16(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * 32768.0)
        .round()
        .clamp(-32768.0, 32767.0) as i16
}

fn write_audio(
    file: File,
    _path: &Path,
    receiver: Receiver<Block>,
    rate: u32,
) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::new(BufWriter::new(file), spec).map_err(|e| e.to_string())?;
    let mut converter = Converter::new(rate)?;
    for block in receiver {
        converter.push(&block.samples[..block.len], |sample| {
            writer.write_sample(sample).map_err(|e| e.to_string())
        })?;
    }
    converter.finish(|sample| writer.write_sample(sample).map_err(|e| e.to_string()))?;
    writer.finalize().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writer_produces_mono_16khz_and_owned_audio_is_removed_on_drop() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-audio-writer-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _directory = Directory(root.clone());
        let path = root.join("fixture.wav");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let audio = AudioFile { path: path.clone() };
        let (sender, receiver) = bounded(2);
        sender
            .send(Block {
                samples: [0.25; BLOCK],
                len: BLOCK,
            })
            .unwrap();
        sender
            .send(Block {
                samples: [-0.25; BLOCK],
                len: 137,
            })
            .unwrap();
        drop(sender);
        write_audio(file, &path, receiver, 48000).unwrap();
        let mut wav = hound::WavReader::open(&path).unwrap();
        assert_eq!(wav.spec().channels, 1);
        assert_eq!(wav.spec().sample_rate, 16000);
        assert_eq!(wav.spec().bits_per_sample, 16);
        assert_eq!(wav.samples::<i16>().count(), (BLOCK + 137) * 16000 / 48000);
        drop(wav);
        drop(audio);
        assert!(!path.exists());
    }
    #[test]
    fn pcm_clamps_without_wraparound() {
        assert_eq!(
            [pcm16(-2.0), pcm16(-1.0), pcm16(0.0), pcm16(1.0), pcm16(2.0)],
            [-32768, -32768, 0, 32767, 32767]
        );
    }
    #[test]
    fn resampling_preserves_duration_including_partial_tail() {
        for rate in [8000, 16000, 44100, 48000, 96000] {
            let mut converter = Converter::new(rate).unwrap();
            let input = vec![0.0; rate as usize + 137];
            let mut output = Vec::new();
            for chunk in input.chunks(727) {
                converter
                    .push(chunk, |v| {
                        output.push(v);
                        Ok(())
                    })
                    .unwrap();
            }
            converter
                .finish(|v| {
                    output.push(v);
                    Ok(())
                })
                .unwrap();
            assert_eq!(output.len(), input.len() * 16000 / rate as usize, "{rate}");
            assert!(output.iter().all(|v| *v == 0));
        }
    }
}
