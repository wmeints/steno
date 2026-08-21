## Context

`steno-daemon` is a Rust process that owns audio capture, transcription, and
input injection. It transcribes with the local `parakeet_rs` model, but that
crate does not obtain its own model files. `main.rs` currently initializes the
logger and then binds the capture hotkey, so there is no startup ordering that
guarantees the model exists. See proposal.md for motivation.

The model files live at the HuggingFace repository `altunenes/parakeet-rs`, in
the `nemotron-3.5-asr-streaming-0.6b-onnx` directory. The five required files
and their sizes are:

| File | Size |
| --- | --- |
| `config.json` | 2.9 KB |
| `tokenizer.model` | 406 KB |
| `encoder.onnx` | 42 MB |
| `decoder_joint.onnx` | 97 MB |
| `encoder.onnx.data` | 2.4 GB |

The large total (~2.6 GB) makes the download a startup cost, not a background
task, and makes integrity checking and resume/retry worthwhile.

## Goals / Non-Goals

**Goals:**
- Provision the model at startup only when files are missing, into
  `~/.config/steno/models/parakeet`.
- Fetch exactly the required files; verify each; fail clearly if any is absent
  after download.
- Keep the source repository and the target directory as compile-time constants.

**Non-Goals:**
- Caching/refreshing a version already on disk (out of scope for this change).
- Concurrent multi-threaded download of a single file.
- Configuring the model source or directory at runtime.

## Decisions

- **Detection: all required files present, non-empty.** Provisioning is
  skipped only when every required file exists and is non-empty. Any missing or
  zero-byte file triggers a (re)download of that file.
- **Fetch method: `hf_hub` crate.** Using `hf_hub` (the Rust HuggingFace Hub
  client, same family as `parakeet_rs`) resolves the `altunenes/parakeet-rs`
  tree to its `main` branch and downloads each file via the Hub API. HuggingFace
  serves large files over LFS/Xet transparently, so no manual LFS handling is
  needed. *Alternative:* shell out to `huggingface-cli`/`wget` — rejected as it
  couples the daemon to a Python CLI and shell quoting.
- **Integrity: size + optional hash.** `hf_hub` reports each file's expected
  content size; the daemon writes to a temp file, verifies the final size and
  (when available) a content hash against the expected value, then renames into
  place. A corrupt or truncated file is rejected and re-fetched or reported.
- **Atomicity.** Each file is downloaded to a temp path beside the target and
  moved into place only after it verifies, so an interrupted download never leaves
  a half file that a later startup mistakes for complete.
- **Source and target are constants.** `altunenes/parakeet-rs` /
  `nemotron-3.5-asr-streaming-0.6b-onnx` and `~/.config/steno/models/parakeet`
  are fixed constants, matching the issue and the existing config convention.
- **Ordering.** Provisioning runs after logger init and before the capture
  hotkey is bound, so the model is ready before transcription can be attempted.

## Risks / Trade-offs

- **Large one-time download at startup.** First run may take a long time on slow
  links. Accepted because transcription cannot work without the files; progress
  is logged. *Mitigation:* size check + temp-then-rename so restarts resume
  cleanly.
- **Network failure / partial download.** Without completion the daemon cannot
  transcribe. *Mitigation:* reported failure rather than a false "ready", plus
  temp-then-rename so a restart re-fetches the incomplete file.
- **Source drift.** If the upstream repository renames or adds files, the fixed
  required set may not match. *Mitigation:* the required set is explicit;
  `config.json`/`tokenizer.model` guard against a wrong variant.
- **hf_hub API/version changes.** A pinned crate version limits churn; if a newer
  API changes, the download function is the single place to adjust.
