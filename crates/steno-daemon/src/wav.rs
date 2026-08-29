//! WAV writing for captured microphone audio.
//!
//! Captured recordings are written to disk as a debug artifact before
//! transcription; the target directory (e.g. `/tmp/steno`) is created on
//! demand.

use std::path::Path;

/// Sample rate all captures are normalized to for the Parakeet TDT model.
pub const SAMPLE_RATE: u32 = 16_000;

/// Write 16 kHz mono f32 samples as a WAV file, creating the parent
/// directory (e.g. /tmp/steno) when missing.
pub fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut writer = hound::WavWriter::create(path, spec()).map_err(io_other)?;
    write_samples(&mut writer, samples)?;
    writer.finalize().map_err(io_other)
}

/// 16 kHz mono 32-bit float PCM.
fn spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    }
}

fn write_samples<W: std::io::Write + std::io::Seek>(
    writer: &mut hound::WavWriter<W>,
    samples: &[f32],
) -> std::io::Result<()> {
    for sample in samples {
        writer.write_sample(*sample).map_err(io_other)?;
    }
    Ok(())
}

fn io_other(e: hound::Error) -> std::io::Error {
    std::io::Error::other(e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavReader, WavSpec};

    fn tmp_wav(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("steno-wav-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("capture.wav")
    }

    #[test]
    fn round_trips_samples_and_spec() {
        let path = tmp_wav("roundtrip");
        let samples = [0.0f32, 0.25, -0.5, 1.0, -1.0];

        write_wav(&path, &samples).expect("write_wav failed");

        let mut reader = WavReader::open(&path).expect("open written wav");
        assert_spec(reader.spec());
        assert_samples(&mut reader, &samples);
        cleanup(&path);
    }

    fn assert_spec(spec: WavSpec) {
        assert_eq!(spec.sample_rate, SAMPLE_RATE);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 32);
        assert_eq!(spec.sample_format, SampleFormat::Float);
    }

    fn assert_samples(reader: &mut WavReader<std::io::BufReader<std::fs::File>>, expected: &[f32]) {
        let read: Vec<f32> = reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .expect("decode samples");
        assert_eq!(read, expected);
    }

    fn cleanup(path: &std::path::Path) {
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
    #[test]
    fn creates_missing_parent_directory() {
        let path = tmp_wav("mkdir");
        assert!(!path.parent().unwrap().exists());

        write_wav(&path, &[0.5f32]).expect("write_wav failed");
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
