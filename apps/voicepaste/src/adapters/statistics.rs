//! SQLite is owned and used only by the dictation thread, never by the UI/audio callback.
use rusqlite::{Connection, params};
use std::{path::Path, time::Duration};
use voicepaste_core::{
    settings::{Device, Language},
    statistics::{Filter, ModelLoad, Sample, Summary},
};

pub struct Statistics(Connection);

fn language_key(language: Language) -> &'static str {
    match language {
        Language::Auto => "auto",
        Language::En => "en",
        Language::Ua => "ua",
        Language::Bilingual => "bilingual",
    }
}

impl Statistics {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        Self::initialize(connection).map_err(|e| e.to_string())
    }

    fn initialize(mut connection: Connection) -> rusqlite::Result<Self> {
        // Do not stall dictation on another instance's database lock.
        connection.busy_timeout(Duration::from_millis(20))?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS transcription_samples_v1 (
                id INTEGER PRIMARY KEY,
                recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                model TEXT NOT NULL,
                revision TEXT NOT NULL,
                beam_size INTEGER NOT NULL CHECK(beam_size BETWEEN 1 AND 10),
                vad INTEGER NOT NULL CHECK(vad IN (0,1)),
                language TEXT NOT NULL,
                device TEXT NOT NULL CHECK(device IN ('cpu','cuda')),
                inference_us INTEGER NOT NULL CHECK(inference_us >= 0),
                audio_samples INTEGER NOT NULL CHECK(audio_samples > 0),
                words INTEGER NOT NULL CHECK(words >= 0),
                characters INTEGER NOT NULL CHECK(characters > 0)
            );
            CREATE INDEX IF NOT EXISTS transcription_filter_v1 ON transcription_samples_v1
                (model, revision, beam_size, vad, language, device);
             CREATE TABLE IF NOT EXISTS model_load_samples_v1 (
                id INTEGER PRIMARY KEY,
                recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                model TEXT NOT NULL,
                revision TEXT NOT NULL,
                beam_size INTEGER NOT NULL CHECK(beam_size BETWEEN 1 AND 10),
                vad INTEGER NOT NULL CHECK(vad IN (0,1)),
                language TEXT NOT NULL,
                device TEXT NOT NULL CHECK(device IN ('cpu','cuda')),
                loading_us INTEGER NOT NULL CHECK(loading_us >= 0)
             );
             CREATE INDEX IF NOT EXISTS model_load_filter_v1 ON model_load_samples_v1
                (model, revision, beam_size, vad, language, device);",
        )?;
        let columns = {
            let mut statement =
                transaction.prepare("PRAGMA table_info(transcription_samples_v1)")?;
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        // Old measurements stay valid. Unknown historical phase times are NULL,
        // never invented zeroes; each phase has its own count/denominator.
        for column in ["vad_us", "preparation_us"] {
            if !columns.iter().any(|name| name == column) {
                transaction.execute_batch(&format!("ALTER TABLE transcription_samples_v1 ADD COLUMN {column} INTEGER CHECK({column} >= 0)"))?;
            }
        }
        transaction.commit()?;
        Ok(Self(connection))
    }

    pub fn record(&self, sample: &Sample) -> Result<(), String> {
        let f = &sample.filter;
        self.0.execute(
            "INSERT INTO transcription_samples_v1
                (model, revision, beam_size, vad, language, device, inference_us, audio_samples, words, characters, vad_us, preparation_us)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![f.model, f.revision, f.beam_size, f.vad, language_key(f.language), sample.device,
                sample.measurement.inference_us, sample.measurement.audio_samples, sample.words, sample.characters,
                sample.measurement.vad_us, sample.measurement.preparation_us],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn record_load(&self, sample: &ModelLoad) -> Result<(), String> {
        let f = &sample.filter;
        self.0.execute(
            "INSERT INTO model_load_samples_v1 (model, revision, beam_size, vad, language, device, loading_us)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![f.model, f.revision, f.beam_size, f.vad, language_key(f.language), sample.device, sample.loading_us],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), String> {
        let transaction = self.0.transaction().map_err(|e| e.to_string())?;
        transaction
            .execute_batch(
                "DELETE FROM transcription_samples_v1; DELETE FROM model_load_samples_v1;",
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())
    }

    pub fn query(&self, filter: &Filter) -> Result<Vec<Summary>, String> {
        let device = match filter.device {
            Device::Cpu => "cpu",
            Device::CudaOnly => "cuda",
            Device::CudaAuto => "",
        };
        let mut statement = self
            .0
            .prepare(
                "SELECT device, COUNT(inference_us), COALESCE(AVG(inference_us)/1000.0,0), COALESCE(AVG(words),0),
                COALESCE(AVG(audio_samples)/16000.0,0),
                COALESCE(TOTAL(inference_us)*16.0/NULLIF(TOTAL(audio_samples),0),0),
                COUNT(loading_us), AVG(loading_us)/1000.0,
                COUNT(vad_us), AVG(vad_us)/1000.0,
                COUNT(preparation_us), AVG(preparation_us)/1000.0
             FROM (
                SELECT model, revision, beam_size, vad, language, device, inference_us, audio_samples, words,
                       vad_us, preparation_us, NULL AS loading_us FROM transcription_samples_v1
                UNION ALL
                SELECT model, revision, beam_size, vad, language, device, NULL, NULL, NULL,
                       NULL, NULL, loading_us FROM model_load_samples_v1
             )
             WHERE model=?1 AND revision=?2 AND beam_size=?3 AND vad=?4 AND language=?5
                AND (?6='' OR device=?6)
             GROUP BY device ORDER BY device",
            )
            .map_err(|e| e.to_string())?;
        statement
            .query_map(
                params![
                    filter.model,
                    filter.revision,
                    filter.beam_size,
                    filter.vad,
                    language_key(filter.language),
                    device
                ],
                |row| {
                    Ok(Summary {
                        device: row.get(0)?,
                        count: row.get(1)?,
                        average_ms: row.get(2)?,
                        average_words: row.get(3)?,
                        average_audio_seconds: row.get(4)?,
                        ms_per_audio_second: row.get(5)?,
                        model_load_count: row.get(6)?,
                        average_model_loading_ms: row.get(7)?,
                        vad_count: row.get(8)?,
                        average_vad_ms: row.get(9)?,
                        preparation_count: row.get(10)?,
                        average_preparation_ms: row.get(11)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicepaste_core::{settings::Settings, statistics::Measurement};

    fn sample(device: &str, us: u64, samples: u64, text: &str) -> Sample {
        Sample::new(
            Filter::from_settings(&Settings::initial().unwrap()).unwrap(),
            device,
            Measurement {
                inference_us: us,
                audio_samples: samples,
                ..Measurement::default()
            },
            text,
        )
        .unwrap()
    }

    #[test]
    fn aggregates_exact_configuration_and_actual_device_without_reset() {
        let db = Statistics::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let mut first = sample("cuda", 100_000, 16000, "hello world");
        first.filter.device = Device::CudaAuto;
        let filter = first.filter.clone();
        assert!(db.query(&filter).unwrap().is_empty());
        db.record(&first).unwrap();
        db.record(&sample("cuda", 500_000, 48000, "one two three four"))
            .unwrap();
        db.record(&sample("cpu", 900_000, 16000, "Привіт")).unwrap();
        // Every other dimension must be excluded independently.
        for dimension in 0..5 {
            let mut other = first.clone();
            match dimension {
                0 => other.filter.model = "large-v3".into(),
                1 => other.filter.revision = "different".into(),
                2 => other.filter.beam_size = if filter.beam_size == 1 { 2 } else { 1 },
                3 => other.filter.vad = !filter.vad,
                _ => {
                    other.filter.language = if filter.language == Language::En {
                        Language::Ua
                    } else {
                        Language::En
                    }
                }
            }
            db.record(&other).unwrap();
        }
        let result = db.query(&filter).unwrap();
        assert_eq!(result.len(), 2);
        let cuda = result.iter().find(|row| row.device == "cuda").unwrap();
        assert_eq!(cuda.count, 2);
        assert_eq!(cuda.average_ms, 300.0);
        assert_eq!(cuda.average_words, 3.0);
        assert_eq!(cuda.average_audio_seconds, 2.0);
        assert_eq!(cuda.ms_per_audio_second, 150.0);
        let mut cpu_only = filter.clone();
        cpu_only.device = Device::Cpu;
        assert_eq!(db.query(&cpu_only).unwrap()[0].count, 1);
        cpu_only.device = Device::CudaOnly;
        assert_eq!(db.query(&cpu_only).unwrap()[0].count, 2);
        assert_eq!(db.query(&filter).unwrap(), result);
    }

    #[test]
    fn history_survives_connection_reopen() {
        let path = std::env::temp_dir().join(format!(
            "voicepaste-stats-test-{}.sqlite",
            std::process::id()
        ));
        // Unique to this test; never opens the user's portable database.
        assert!(!path.exists());
        let sample = sample("cpu", 123000, 16000, "hello");
        {
            let db = Statistics::open(&path).unwrap();
            db.record(&sample).unwrap();
        }
        {
            let db = Statistics::open(&path).unwrap();
            let mut filter = sample.filter;
            filter.device = Device::Cpu;
            assert_eq!(db.query(&filter).unwrap()[0].average_ms, 123.0);
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn model_loads_are_independent_of_transcriptions_and_filtered() {
        let db = Statistics::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let mut sample = sample("cuda", 100_000, 16000, "hello");
        sample.filter.device = Device::CudaOnly;
        let load = ModelLoad {
            filter: sample.filter.clone(),
            device: "cuda".into(),
            loading_us: 2_000_000,
        };
        db.record_load(&load).unwrap();
        let initial = db.query(&sample.filter).unwrap();
        assert_eq!(initial[0].count, 0);
        assert_eq!(initial[0].model_load_count, 1);
        assert_eq!(initial[0].average_model_loading_ms, Some(2000.0));
        for _ in 0..3 {
            db.record(&sample).unwrap();
        }
        let result = db.query(&sample.filter).unwrap();
        assert_eq!(result[0].count, 3);
        assert_eq!(result[0].model_load_count, 1);
        assert_eq!(result[0].average_model_loading_ms, Some(2000.0));
        for dimension in 0..6 {
            let mut other = load.clone();
            match dimension {
                0 => other.filter.model = "large-v3".into(),
                1 => other.filter.revision = "other".into(),
                2 => other.filter.beam_size = if other.filter.beam_size == 1 { 2 } else { 1 },
                3 => other.filter.vad = !other.filter.vad,
                4 => {
                    other.filter.language = if other.filter.language == Language::En {
                        Language::Ua
                    } else {
                        Language::En
                    }
                }
                _ => other.device = "cpu".into(),
            }
            db.record_load(&other).unwrap();
        }
        assert_eq!(db.query(&sample.filter).unwrap(), result);
        db.record_load(&ModelLoad {
            loading_us: 4_000_000,
            ..load
        })
        .unwrap();
        let result = db.query(&sample.filter).unwrap();
        assert_eq!(result[0].model_load_count, 2);
        assert_eq!(result[0].average_model_loading_ms, Some(3000.0));
    }

    #[test]
    fn reset_clears_all_models_and_phases_and_allows_new_measurements() {
        let mut db = Statistics::initialize(Connection::open_in_memory().unwrap()).unwrap();
        for index in 0..3 {
            let mut sample = sample(
                if index == 0 { "cpu" } else { "cuda" },
                100_000,
                16000,
                "hello",
            );
            sample.filter.model = format!("model-{index}");
            sample.filter.beam_size = index + 1;
            sample.measurement.vad_us = Some(1000);
            sample.measurement.preparation_us = Some(2000);
            db.record(&sample).unwrap();
            db.record_load(&ModelLoad {
                filter: sample.filter,
                device: sample.device,
                loading_us: 1_000_000,
            })
            .unwrap();
        }
        db.reset().unwrap();
        let mut db = Statistics::initialize(db.0).unwrap();
        for table in ["transcription_samples_v1", "model_load_samples_v1"] {
            let count: u64 =
                db.0.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
        }
        db.reset().unwrap(); // Empty history is a successful no-op.
        let mut sample = sample("cpu", 300_000, 16000, "new history");
        sample.filter.device = Device::Cpu;
        db.record(&sample).unwrap();
        let result = db.query(&sample.filter).unwrap();
        assert_eq!(result[0].count, 1);
        assert_eq!(result[0].average_ms, 300.0);
        assert_eq!(result[0].model_load_count, 0);
    }

    #[test]
    fn failed_reset_rolls_back_both_tables() {
        let mut db = Statistics::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let mut sample = sample("cpu", 100_000, 16000, "keep history");
        sample.filter.device = Device::Cpu;
        db.record(&sample).unwrap();
        db.record_load(&ModelLoad {
            filter: sample.filter.clone(),
            device: "cpu".into(),
            loading_us: 1_000_000,
        })
        .unwrap();
        let before = db.query(&sample.filter).unwrap();
        assert_eq!(before[0].count, 1);
        assert_eq!(before[0].model_load_count, 1);
        db.0.execute_batch(
            "CREATE TRIGGER fail_load_reset BEFORE DELETE ON model_load_samples_v1
            BEGIN SELECT RAISE(ABORT, 'reset test failure'); END;",
        )
        .unwrap();
        assert!(db.reset().is_err());
        assert_eq!(db.query(&sample.filter).unwrap(), before);
        db.0.execute_batch("DROP TRIGGER fail_load_reset").unwrap();
        db.reset().unwrap();
        assert!(db.query(&sample.filter).unwrap().is_empty());
    }

    #[test]
    fn migration_preserves_history_without_inventing_zero_phase_times() {
        let db = Statistics::initialize(Connection::open_in_memory().unwrap()).unwrap();
        let mut old = sample("cpu", 100_000, 16000, "old history");
        old.filter.device = Device::Cpu;
        db.record(&old).unwrap();
        // Recreate the exact pre-phase table shape without touching user data.
        db.0.execute_batch(
            "ALTER TABLE transcription_samples_v1 DROP COLUMN vad_us;
                            ALTER TABLE transcription_samples_v1 DROP COLUMN preparation_us;",
        )
        .unwrap();
        let db = Statistics::initialize(db.0).unwrap();
        let before = db.query(&old.filter).unwrap();
        assert_eq!(before[0].count, 1);
        assert_eq!(before[0].average_vad_ms, None);
        assert_eq!(before[0].average_preparation_ms, None);
        let mut new = old.clone();
        new.measurement.vad_us = Some(5000);
        new.measurement.preparation_us = Some(12000);
        db.record(&new).unwrap();
        let result = db.query(&old.filter).unwrap();
        assert_eq!(result[0].count, 2);
        assert_eq!(result[0].vad_count, 1);
        assert_eq!(result[0].average_vad_ms, Some(5.0));
        assert_eq!(result[0].preparation_count, 1);
        assert_eq!(result[0].average_preparation_ms, Some(12.0));
        let db = Statistics::initialize(db.0).unwrap();
        assert_eq!(db.query(&old.filter).unwrap(), result);
    }
}
