//! Parakeet model provisioning.
//!
//! The `parakeet_rs` transcription model does not fetch its own files, so the
//! daemon provisions them at startup: when the required ONNX files are absent
//! from the model directory they are downloaded from HuggingFace and verified
//! against their declared size before the daemon binds the capture hotkey.
//!
//! The source repository and the target directory are fixed by convention
//! (`altunenes/parakeet-rs` / `nemotron-3.5-asr-streaming-0.6b-onnx` and
//! `~/.config/steno/models/parakeet`), not by configuration.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use rand::Rng;

use futures::StreamExt;

use hf_hub::progress::{ProgressEvent, ProgressHandler};
use hf_hub::repository::download::HFByteStream;

/// The HuggingFace repository that holds the parakeet ONNX model files.
pub const MODEL_REPOSITORY: &str = "altunenes/parakeet-rs";

/// The sub-directory of the repository that holds the model files, used as the
/// prefix of every file's path within the repository.
pub const MODEL_DIR: &str = "nemotron-3.5-asr-streaming-0.6b-onnx";

/// The Git revision (branch) the model files are resolved from.
pub const MODEL_BRANCH: &str = "main";

/// The parakeet model files the daemon requires, in order.
pub const REQUIRED_FILES: &[&str] = &[
    "config.json",
    "tokenizer.model",
    "encoder.onnx",
    "decoder_joint.onnx",
    "encoder.onnx.data",
];

/// The known byte sizes of the required model files, keyed by file name.
///
/// These are the sizes the files have on the source repository, recorded as a
/// trusted constant rather than read from the server. A present file is only
/// treated as ready when its size matches, and a downloaded file is only moved
/// into place when its byte count matches, so a partial or corrupt file is
/// rejected instead of being mistaken for complete. `None` means the size is
/// unknown and cannot be verified.
pub const EXPECTED_SIZES: &[(&str, Option<u64>)] = &[
    ("config.json", Some(2969)),
    ("tokenizer.model", Some(416233)),
    ("encoder.onnx", Some(44000192)),
    ("decoder_joint.onnx", Some(101807616)),
    ("encoder.onnx.data", Some(2439900152)),
];

/// A file the daemon needs to download for the model to be ready.
#[derive(Debug)]
pub struct ModelFile {
    /// The file name inside the repository, e.g. `config.json`.
    pub name: &'static str,
    /// The known byte size of the file, or `None` if it cannot be verified.
    pub size: Option<u64>,
}

impl ModelFile {
    /// The relative path of the file within the repository, e.g.
    /// `nemotron-3.5-asr-streaming-0.6b-onnx/config.json`.
    pub fn repo_path(&self) -> String {
        format!("{MODEL_DIR}/{}", self.name)
    }

    /// Whether the file is present at `path` with its expected size.
    ///
    /// A file whose size is unknown is never treated as ready, so it is
    /// downloaded and its byte count is checked rather than being mistaken for
    /// complete.
    pub fn is_ready(&self, path: &Path) -> bool {
        match std::fs::metadata(path.join(self.name)) {
            Ok(metadata) => metadata.is_file() && self.size == Some(metadata.len()),
            Err(_) => false,
        }
    }
}

/// The error type for provisioning failures.
pub type Error = hf_hub::HFError;

/// The result of provisioning a single file: the resolved local path and its
/// verified byte size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    /// The local path the file was downloaded to.
    pub path: PathBuf,
    /// The verified byte size of the file, equal to the server-declared size.
    pub size: u64,
}

/// Logs download progress to the daemon's log output, so the (large) one-time
/// download is observable instead of appearing to hang.
struct LogProgress;

impl ProgressHandler for LogProgress {
    fn on_progress(&self, event: &ProgressEvent) {
        if let ProgressEvent::Download(hf_hub::progress::DownloadEvent::Progress { files }) = event {
            for file in files {
                log::info!(
                    "model: {} {}/{} bytes",
                    file.filename,
                    file.bytes_completed,
                    file.total_bytes
                );
            }
        }
    }
}

/// Ensures the parakeet model is provisioned at `model_dir`.
///
/// Skips the download when every required file is already present with its
/// expected size, otherwise downloads each missing file from
/// [`MODEL_REPOSITORY`] to a temporary file, verifies its byte count against
/// the expected size, and moves it into place. Returns a specific [`Error`]
/// (rather than a false "ready") when any file cannot be fetched or fails its
/// size check.
///
/// `model_dir` is `~/.config/steno/models/parakeet`; it is a parameter so the
/// pure logic can be exercised in tests without touching the real config
/// directory.
pub async fn ensure_provisioned(model_dir: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(model_dir)?;

    let (owner, name) = MODEL_REPOSITORY
        .split_once('/')
        .expect("repository id is owner/name");
    let client = hf_hub::HFClient::new()?;
    let repo = client.model(owner, name);
    for name in REQUIRED_FILES {
        // `EXPECTED_SIZES` values are `Option<u64>`, so `find().map(...)` yields
        // `Option<Option<u64>>`; the `.flatten()` collapses it to `Option<u64>`.
        #[allow(clippy::map_flatten)]
        let size = EXPECTED_SIZES
            .iter()
            .find(|(file_name, _)| *file_name == *name)
            .map(|(_, size)| *size)
            .flatten();
        let file = ModelFile { name, size };
        if file.is_ready(model_dir) {
            log::info!("model: {} present, skipping", file.name);
            continue;
        }

        log::info!("model: downloading {}", file.name);
        let downloaded = download_file(&repo, &file, model_dir).await?;
        log::info!(
            "model: {} verified ({})",
            file.name,
            format_size(downloaded.size)
        );
    }

    log::info!("model: provisioned at {}", model_dir.display());
    Ok(())
}

/// Downloads `file` from `repo` to `model_dir`, verifying its size.
///
/// The file is streamed to a temporary path beside its destination and moved
/// into place only after the final byte has been written and its byte count
/// matches the expected size, so a truncated or corrupt download is rejected
/// and an interrupted download never leaves a partial file that a later
/// startup mistakes for complete.
async fn download_file(
    repo: &hf_hub::HFRepository<hf_hub::RepoTypeModel>,
    file: &ModelFile,
    model_dir: &Path,
) -> Result<Downloaded, Error> {
    let dest = model_dir.join(file.name);

    // The expected size is a trusted constant, not the server-declared length,
    // so the byte count is checked against a value the server cannot inflate.
    let expected = file.size.ok_or_else(|| {
        hf_hub::HFError::malformed_response_at("unknown file size", file.repo_path())
    })?;

    let (_declared, stream) = download_streamed(repo, &file.repo_path()).await?;

    let temp = temp_path(&dest);
    let result = stream_to_file(stream, &temp).await;

    match result {
        Ok(got) => {
            if got != expected {
                let _ = std::fs::remove_file(&temp);
                return Err(hf_hub::HFError::malformed_response_at(
                    format!("size mismatch: got {got}, expected {expected}"),
                    file.repo_path(),
                ));
            }
            std::fs::rename(&temp, &dest)?;
            Ok(Downloaded { path: dest, size: got })
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

/// Streams `filename` from `repo`, returning the declared size and the byte
/// stream. Xet-backed large files are fetched transparently by the crate.
async fn download_streamed(
    repo: &hf_hub::HFRepository<hf_hub::RepoTypeModel>,
    filename: &str,
) -> Result<(Option<u64>, HFByteStream), Error> {
    repo.download_file_stream()
        .filename(filename.to_string())
        .revision(MODEL_BRANCH.to_string())
        .progress(LogProgress)
        .send()
        .await
}

/// Writes `stream` to `path` and returns the total number of bytes written.
///
/// The byte count is compared against the expected size by [`download_file`],
/// so a truncated or corrupt download is rejected rather than treated as
/// complete.
async fn stream_to_file(stream: HFByteStream, path: &Path) -> Result<u64, Error> {
    let mut file = File::create(path)?;
    let mut bytes = 0u64;
    let mut stream = Box::pin(stream);
    loop {
        match stream.next().await {
            Some(Ok(chunk)) => {
                file.write_all(&chunk)?;
                bytes += chunk.len() as u64;
            }
            Some(Err(error)) => return Err(error),
            None => break,
        }
    }
    file.flush()?;
    Ok(bytes)
}

/// The temporary path used for a file being downloaded into `dest`, beside it.
///
/// The name carries a unique per-process suffix so two daemons downloading the
/// same file do not race on one shared temporary file, and a stale temporary
/// file left by an interrupted run is not silently truncated and overwritten.
fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("model");
    let unique = random_hex();
    dest.with_file_name(format!("{name}.{}-{unique}.tmp", std::process::id()))
}

/// A short random hex token, distinct from the pid, for the temporary name.
fn random_hex() -> String {
    let mut bytes = [0u8; 8];
    rand::rngs::OsRng.fill(&mut bytes);
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// A human-readable size, for log output.
fn format_size(bytes: u64) -> String {
    let (value, unit) = match bytes {
        b if b >= (1 << 30) => (bytes as f64 / (1 << 30) as f64, "GiB"),
        b if b >= (1 << 20) => (bytes as f64 / (1 << 20) as f64, "MiB"),
        b if b >= (1 << 10) => (bytes as f64 / (1 << 10) as f64, "KiB"),
        _ => (bytes as f64, "B"),
    };
    if value >= 100.0 || value.fract() == 0.0 {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}
/// The model directory, `~/.config/steno/models/parakeet`, derived from the
/// user's config directory (honouring `$XDG_CONFIG_HOME`), not a hardcoded home.
///
/// Returns an error rather than falling back to a non-user-writable location
/// when neither `$XDG_CONFIG_HOME` nor `$HOME` is set, so the daemon never
/// downloads model files into an arbitrary directory such as filesystem root.
pub fn model_dir() -> Result<PathBuf, Error> {
    let config = config_dir()?;
    Ok(config.join("steno").join("models").join("parakeet"))
}

/// The user config directory, `~/.config` (honouring `$XDG_CONFIG_HOME`).
///
/// Pure and testable: it reads the environment and returns an error when no
/// user-writable config directory can be derived.
fn config_dir() -> Result<PathBuf, Error> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                Path::new(&home).join(".config")
            })
        })
        .ok_or_else(|| {
            hf_hub::HFError::malformed_response_at(
                "no XDG_CONFIG_HOME or HOME is set, cannot derive the model directory",
                "config",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_files_are_the_five_model_files() {
        assert_eq!(
            REQUIRED_FILES,
            &[
                "config.json",
                "tokenizer.model",
                "encoder.onnx",
                "decoder_joint.onnx",
                "encoder.onnx.data"
            ]
        );
    }

    #[test]
    fn expected_sizes_cover_the_required_files() {
        for name in REQUIRED_FILES {
            assert!(
                EXPECTED_SIZES.iter().any(|(file_name, _)| file_name == name),
                "missing expected size for {name}"
            );
        }
    }
    #[test]
    fn is_ready_requires_the_expected_size() {
        let dir = temp_dir();
        let file = ModelFile { name: "config.json", size: Some(4) };
        // A missing file is not ready.
        assert!(!file.is_ready(&dir));

        // A wrong-size file (too small) is not ready.
        std::fs::write(dir.join("config.json"), b"abc").unwrap();
        assert!(!file.is_ready(&dir));

        // A wrong-size file (too large) is not ready.
        std::fs::write(dir.join("config.json"), b"abcde").unwrap();
        assert!(!file.is_ready(&dir));

        // A file of the exact expected size is ready.
        std::fs::write(dir.join("config.json"), b"abcd").unwrap();
        assert!(file.is_ready(&dir));

        // A file whose size is unknown can never be treated as ready.
        let unknown = ModelFile { name: "config.json", size: None };
        assert!(!unknown.is_ready(&dir));
    }

    #[test]
    fn temp_path_sits_beside_the_destination_and_is_unique() {
        let dest = std::path::Path::new("/home/user/.config/steno/models/parakeet/config.json");

        let temp = temp_path(dest);
        assert_eq!(temp.parent().unwrap(), dest.parent().unwrap());
        assert_ne!(temp, dest);
        // The name ends in `.tmp` and is distinct from the destination.
        assert!(temp.file_name().unwrap().to_str().unwrap().ends_with(".tmp"));
        // Two calls produce distinct names, so concurrent downloads do not
        // share a single temporary file.
        assert_ne!(temp_path(dest), temp_path(dest));
    }

    #[test]
    fn format_size_reads_naturally() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1 KiB");
        assert_eq!(format_size(4096), "4 KiB");
        assert_eq!(format_size(1024 * 1024), "1 MiB");
        assert_eq!(format_size(1536), "1.5 KiB");
    }

    #[test]
    fn config_dir_errors_without_home() {
        // When neither XDG_CONFIG_HOME nor HOME is set, config_dir must fail
        // rather than falling back to filesystem root.
        let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
        let prev_home = std::env::var_os("HOME");
        let result = unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::remove_var("HOME");
            config_dir()
        };
        if let Some(xdg) = prev_xdg {
            unsafe {
                std::env::set_var("XDG_CONFIG_HOME", xdg);
            }
        }
        if let Some(home) = prev_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        }
        assert!(result.is_err());
    }
    fn temp_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("steno-model-{}", uuid()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn uuid() -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::process::id().hash(&mut hasher);
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}
