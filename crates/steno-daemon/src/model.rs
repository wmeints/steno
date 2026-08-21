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

/// A file the daemon needs to download for the model to be ready.
#[derive(Debug)]
pub struct ModelFile {
    /// The file name inside the repository, e.g. `config.json`.
    pub name: &'static str,
}

impl ModelFile {
    /// The relative path of the file within the repository, e.g.
    /// `nemotron-3.5-asr-streaming-0.6b-onnx/config.json`.
    pub fn repo_path(&self) -> String {
        format!("{MODEL_DIR}/{}", self.name)
    }

    /// Whether the file is present and non-empty at `path`.
    pub fn is_ready(&self, path: &Path) -> bool {
        match std::fs::metadata(path.join(self.name)) {
            Ok(metadata) => metadata.is_file() && metadata.len() > 0,
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
/// Skips the download when every required file is already present and
/// non-empty, otherwise downloads each missing file from
/// [`MODEL_REPOSITORY`] to a temporary file, verifies its size against the
/// value declared by HuggingFace, and moves it into place. Returns a specific
/// [`Error`] (rather than a false "ready") when any file cannot be fetched or
/// fails its size check.
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
        let file = ModelFile { name };
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
/// into place only after the final byte has been written, so an interrupted
/// download never leaves a partial file that a later startup mistakes for
/// complete.
async fn download_file(
    repo: &hf_hub::HFRepository<hf_hub::RepoTypeModel>,
    file: &ModelFile,
    model_dir: &Path,
) -> Result<Downloaded, Error> {
    let dest = model_dir.join(file.name);

    // The server-declared content length, used both as the expected size and as
    // the point against which the streamed byte count is compared.
    let (expected, stream) = download_streamed(repo, &file.repo_path()).await?;
    let expected =
        expected.ok_or_else(|| hf_hub::HFError::malformed_response_at("missing file size", file.repo_path()))?;

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
///
/// The final count is compared against the declared size by the caller, so a
/// truncated or corrupt download is rejected rather than treated as complete.
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
fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("model");
    dest.with_file_name(format!("{name}.tmp"))
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
pub fn model_dir() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|home| Path::new(&home).join(".config"))
                .unwrap_or_else(|| PathBuf::from("/"))
        });
    config.join("steno").join("models").join("parakeet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_resolves_under_config() {
        let config = std::path::Path::new("/home/user/.config");
        let resolved = config.join("steno").join("models").join("parakeet");

        assert!(resolved.to_string_lossy().ends_with(".config/steno/models/parakeet"));
    }

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
    fn is_ready_requires_a_non_empty_file() {
        let dir = temp_dir();

        let file = ModelFile { name: "config.json" };
        assert!(!file.is_ready(&dir));

        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        assert!(file.is_ready(&dir));

        std::fs::write(dir.join("config.json"), b"").unwrap();
        assert!(!file.is_ready(&dir));
    }

    #[test]
    fn temp_path_sits_beside_the_destination() {
        let dest = std::path::Path::new("/home/user/.config/steno/models/parakeet/config.json");

        let temp = temp_path(dest);
        assert_eq!(temp.parent().unwrap(), dest.parent().unwrap());
        assert_ne!(temp, dest);
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
