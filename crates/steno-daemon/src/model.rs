use std::path::PathBuf;

use anyhow::Result;
use hf_hub::{
    HFClient,
    progress::{DownloadEvent, ProgressEvent, ProgressHandler},
};

const REQUIRED_FILES: &[&str] = &[
    "tdt/decoder_joint-model.int8.onnx",
    "tdt/decoder_joint-model.onnx",
    "tdt/encoder-model.int8.onnx",
    "tdt/encoder-model.onnx",
    "tdt/encoder-model.onnx.data",
    "tdt/nemo128.onnx",
    "tdt/vocab.txt",
];

const REPO_NAME: &str = "altunenes";

const MODEL_NAME: &str = "parakeet-rs";

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

pub fn parakeet_model_dir() -> Result<PathBuf> {
    let mut config_dir: PathBuf = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| std::env::var("HOME").unwrap_or("/tmp".to_string()))
        .into();

    config_dir.extend(&["steno", "models", "parakeet"]);

    Ok(config_dir)
}

fn is_model_available() -> Result<bool> {
    let local_dir = parakeet_model_dir()?;

    for file in REQUIRED_FILES {
        let file_path = local_dir.with_file_name(file.to_string());

        if !file_path.exists() {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn download_model() -> Result<()> {
    let client = HFClient::new()?;
    let repo = client.model(REPO_NAME, MODEL_NAME);
    let output_dir = parakeet_model_dir()?;

    for file in REQUIRED_FILES {
        tracing::info!("downloading {}", file.to_string());

        repo.download_file()
            .filename(file.to_string())
            .local_dir(output_dir.clone())
            .progress(LogProgress)
            .send()
            .await?;
    }

    tracing::info!("downloaded all model files to {:?}", output_dir.clone());

    Ok(())
}
