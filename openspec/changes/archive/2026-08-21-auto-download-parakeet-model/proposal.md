## Why

The `parakeet_rs` crate that performs transcription does not obtain the parakeet
model files itself, and the daemon has no code that fetches them. Without the
model files present on disk, the daemon cannot transcribe recorded audio at all.
Today the operator would have to locate and download these files by hand, which
blocks first-time setup.

## What Changes

- The `steno-daemon` gains a startup step that detects whether the required
  parakeet model files are present in `~/.config/steno/models/parakeet` and,
  when they are missing, downloads them from the HuggingFace repository
  `altunenes/parakeet-rs` before the rest of startup proceeds.
- A new `model-provisioning` capability in the daemon owns the download, storage
  location, and integrity check of the parakeet model files.
- The daemon's model directory and the source HuggingFace repository are defined
  by convention (not by configuration).

## Capabilities

### New Capabilities
- `model-provisioning`: ensures the parakeet ONNX model files are present at
  `~/.config/steno/models/parakeet`, downloading them from HuggingFace when they
  are missing, before transcription is attempted.

### Modified Capabilities
<!-- none: capture-key-interception requirements are unchanged -->
