use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use hf_hub::{
    HFClient,
    progress::{DownloadEvent, ProgressEvent, ProgressHandler},
    repository::{HFRepository, RepoTypeModel},
};

/// Required model files as (repository path in `altunenes/parakeet-rs`,
/// pinned expected byte size).
///
/// The `tdt/` prefix is the repository's storage location: it belongs to
/// the download URL only. Files are stored flat in the model directory
/// under their basename — that is the layout the ParakeetTDT model reads.
///
/// The set is exactly what `ParakeetTDT::from_pretrained` loads: the
/// encoder, its external-data file, the joint decoder, and the vocab.
/// The int8 variants are only fallback candidates of the loader, shadowed
/// by the full-precision names, and `nemo128.onnx` is not referenced by
/// the transcription model, so neither is provisioned.
///
/// Sizes pinned from the HuggingFace repository API, 2026-08-23.
const REQUIRED_FILES: &[(&str, u64)] = &[
    ("tdt/decoder_joint-model.onnx", 72_520_893),
    ("tdt/encoder-model.onnx", 41_770_866),
    ("tdt/encoder-model.onnx.data", 2_435_420_160),
    ("tdt/vocab.txt", 93_939),
];

const REPO_OWNER: &str = "altunenes";

const REPO_NAME: &str = "parakeet-rs";

struct LogProgress;

impl ProgressHandler for LogProgress {
    fn on_progress(&self, event: &hf_hub::progress::ProgressEvent) {
        if let ProgressEvent::Download(DownloadEvent::Progress { files }) = event {
            for file in files {
                tracing::info!(
                    "file: {} {}/{} bytes",
                    file.filename,
                    file.bytes_completed,
                    file.total_bytes
                );
            }
        }
    }
}

pub async fn ensure_parakeet_model() -> Result<()> {
    if !is_model_available()? {
        download_model().await?;
    }

    Ok(())
}

/// Derive the user's config directory: `$XDG_CONFIG_HOME` when set,
/// otherwise `$HOME/.config`. Fails when neither can be derived —
/// provisioning must not fall back to a temporary, world-readable
/// location.
fn resolve_config_dir(
    xdg: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    if let Some(xdg) = xdg.filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(xdg));
    }

    let home = home.filter(|path| !path.is_empty()).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot derive the model directory: neither $XDG_CONFIG_HOME nor $HOME is set"
        )
    })?;

    Ok(PathBuf::from(home).join(".config"))
}

fn model_dir_from_base(config_dir: &Path) -> PathBuf {
    config_dir.join("steno").join("models").join("parakeet")
}

pub fn parakeet_model_dir() -> Result<PathBuf> {
    let config_dir = resolve_config_dir(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )?;

    Ok(model_dir_from_base(&config_dir))
}

/// Staging directory for in-flight downloads, beside the model directory
/// so that moving a verified file into place is an atomic same-filesystem
/// rename.
fn staging_dir() -> Result<PathBuf> {
    let model_dir = parakeet_model_dir()?;
    let models_dir = model_dir.parent().ok_or_else(|| {
        anyhow::anyhow!("model directory {:?} has no parent for staging", model_dir)
    })?;

    Ok(models_dir.join(".parakeet-staging"))
}

fn basename(repo_path: &str) -> &str {
    repo_path.rsplit('/').next().unwrap_or(repo_path)
}

/// A file counts as provisioned only when it exists and is non-empty.
fn file_is_ready(file: &Path) -> bool {
    std::fs::metadata(file)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
}

/// The model is available only when every required file exists, flat in
/// the model directory, and is non-empty.
pub fn is_model_available() -> Result<bool> {
    is_model_available_in(&parakeet_model_dir()?)
}

fn is_model_available_in(model_dir: &Path) -> Result<bool> {
    for (repo_path, _) in REQUIRED_FILES {
        if !file_is_ready(&model_dir.join(basename(repo_path))) {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn download_model() -> Result<()> {
    let model_dir = parakeet_model_dir()?;
    let staging = staging_dir()?;

    std::fs::create_dir_all(&model_dir)?;
    std::fs::create_dir_all(&staging)?;

    let client = HFClient::new()?;
    let repo = client.model(REPO_OWNER, REPO_NAME);

    // Clean up staging on every exit path, so a failed or interrupted run
    // never leaves staged files behind that a later run could mistake for
    // input.
    let result = download_all(&repo, &staging, &model_dir).await;

    let _ = std::fs::remove_dir_all(&staging);

    if result.is_ok() {
        tracing::info!("parakeet model ready at {:?}", model_dir);
    }

    result
}

async fn download_all(
    repo: &HFRepository<RepoTypeModel>,
    staging: &Path,
    model_dir: &Path,
) -> Result<()> {
    for (repo_path, expected_size) in REQUIRED_FILES {
        let dest = model_dir.join(basename(repo_path));
        if file_is_ready(&dest) {
            continue;
        }

        download_one(repo, repo_path, *expected_size, staging, &dest).await?;
    }

    Ok(())
}

async fn download_one(
    repo: &HFRepository<RepoTypeModel>,
    repo_path: &str,
    expected_size: u64,
    staging: &Path,
    dest: &Path,
) -> Result<()> {
    // hf-hub writes to `staging.join(repo_path)`, i.e. staging/tdt/<basename>.
    let staged = staging.join(repo_path);

    tracing::info!("downloading {} ({} bytes)", repo_path, expected_size);

    repo.download_file()
        .filename(repo_path.to_string())
        .local_dir(staging.to_path_buf())
        .progress(LogProgress)
        .send()
        .await?;

    place_verified(&staged, dest, expected_size)
}

/// Verify the staged download's byte count and move it into place. On
/// mismatch the staged file is deleted and the destination is left
/// untouched, so the model directory never sees a rejected file.
fn place_verified(staged: &Path, dest: &Path, expected_size: u64) -> Result<()> {
    let staged_size = std::fs::metadata(staged)
        .with_context(|| format!("staged file {:?} is missing after download", staged))?
        .len();

    if staged_size != expected_size {
        let _ = std::fs::remove_file(staged);
        anyhow::bail!(
            "downloaded file has size {staged_size}, expected {expected_size}; rejecting it"
        );
    }

    std::fs::rename(staged, dest)
        .with_context(|| format!("moving {:?} into place at {:?}", staged, dest))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn tmp_dir() -> PathBuf {
        let n = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("steno-model-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, size: u64) {
        std::fs::write(path, vec![0u8; size as usize]).unwrap();
    }

    #[test]
    fn config_dir_prefers_xdg() {
        let dir = resolve_config_dir(Some(OsStr::new("/xdg")), Some(OsStr::new("/home"))).unwrap();
        assert_eq!(dir, PathBuf::from("/xdg"));
    }

    #[test]
    fn config_dir_falls_back_to_home() {
        let dir = resolve_config_dir(None, Some(OsStr::new("/home"))).unwrap();
        assert_eq!(dir, PathBuf::from("/home/.config"));

        let dir = resolve_config_dir(Some(OsStr::new("")), Some(OsStr::new("/home"))).unwrap();
        assert_eq!(dir, PathBuf::from("/home/.config"));
    }

    #[test]
    fn config_dir_errors_when_neither_is_set() {
        assert!(resolve_config_dir(None, None).is_err());
    }

    #[test]
    fn model_dir_is_flat_under_config() {
        let dir = model_dir_from_base(Path::new("/home/u/.config"));
        assert_eq!(dir, PathBuf::from("/home/u/.config/steno/models/parakeet"));
    }

    #[test]
    fn availability_requires_all_files_present_and_nonempty() {
        let dir = tmp_dir();
        let model_dir = dir.join("parakeet");
        std::fs::create_dir_all(&model_dir).unwrap();

        for (repo_path, _) in REQUIRED_FILES {
            write_file(&model_dir.join(basename(repo_path)), 1);
        }
        assert!(is_model_available_in(&model_dir).unwrap());

        // Zero-byte file counts as absent.
        write_file(&model_dir.join("vocab.txt"), 0);
        assert!(!is_model_available_in(&model_dir).unwrap());

        // Missing file counts as absent.
        std::fs::remove_file(model_dir.join("encoder-model.onnx")).unwrap();
        assert!(!is_model_available_in(&model_dir).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn place_verified_moves_file_with_matching_size() {
        let dir = tmp_dir();
        let staged = dir.join("staged.onnx");
        let dest = dir.join("flat.onnx");
        write_file(&staged, 1234);

        place_verified(&staged, &dest, 1234).unwrap();

        assert!(!staged.exists());
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), 1234);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn place_verified_rejects_and_deletes_mismatched_size() {
        let dir = tmp_dir();
        let staged = dir.join("staged.onnx");
        let dest = dir.join("flat.onnx");
        write_file(&staged, 1);

        let err = place_verified(&staged, &dest, 1234).unwrap_err();
        assert!(err.to_string().contains("rejecting"));

        // The staged file is gone and nothing was placed.
        assert!(!staged.exists());
        assert!(!dest.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn place_verified_errors_when_staged_file_missing() {
        let dir = tmp_dir();
        let staged = dir.join("staged.onnx");
        let dest = dir.join("flat.onnx");

        assert!(place_verified(&staged, &dest, 1).is_err());
        assert!(!dest.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
