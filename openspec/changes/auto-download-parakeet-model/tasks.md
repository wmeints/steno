## 1. Model configuration

- [x] 1.1 Add constants for the HuggingFace source (`altunenes/parakeet-rs`, repo `nemotron-3.5-asr-streaming-0.6b-onnx`) and the required file set (`config.json`, `tokenizer.model`, `encoder.onnx`, `decoder_joint.onnx`, `encoder.onnx.data`). Verify: `cargo build` compiles the constants module.
- [x] 1.2 Add a constant/derivation for the model directory `~/.config/steno/models/parakeet`, using the user's config dir (e.g. `dirs`/`$XDG_CONFIG_HOME`), not a hardcoded home. Verify: a unit test asserts the resolved path ends with `.config/steno/models/parakeet`.

## 2. Provisioning module

- [x] 2.1 Add a `model-provisioning` (or `model`) module to `steno-daemon` with a function that ensures the model is provisioned; return a result type that reports success or a specific failure. Verify: `cargo build` compiles the module.
- [x] 2.2 Implement "all required files present and non-empty" detection that decides whether provisioning can be skipped. Verify: unit test — all present+non-empty ⇒ skip; one missing or zero-byte ⇒ proceed.
- [x] 2.3 Add a dependency to fetch from HuggingFace (e.g. `hf_hub`) to `crates/steno-daemon/Cargo.toml`. Verify: `cargo build` resolves and compiles the dependency.
- [x] 2.4 Implement download of each required file to a temp path beside the target, then rename into place only after it completes. Verify: after a run, each required file exists in the model directory and no `.tmp` file remains.
- [x] 2.5 Verify each downloaded file's integrity (expected size, and content hash when available) against the Hub value; reject on mismatch. Verify: unit test with a wrong-size/temp-corrupted file asserts rejection and no ready state.
- [x] 2.6 Report a failure (and not proceed) when a file cannot be fetched (network unreachable, 404, or integrity failure). Verify: with the network blocked, the function returns an error rather than "ready".

## 3. Startup integration

- [x] 3.1 Call the provisioning function from `main.rs` after logger init and before the capture hotkey is bound, logging progress. Verify: run the daemon with an empty model directory and observe it downloads then binds the hotkey.
- [x] 3.2 On a provisioning error, log it and exit non-zero without binding the hotkey. Verify: force a network failure and confirm a non-zero exit and no hotkey binding.

## 4. Verification

- [x] 4.1 Run `cargo build` and `cargo clippy` for `steno-daemon` with no warnings. Verify: both commands exit 0.
- [x] 4.2 Run `cargo test` for the new module. Verify: all tests pass.
- [x] 4.3 Smoke test: delete `~/.config/steno/models/parakeet`, start the daemon, confirm it downloads all five files and reports them ready. Verify: the five files are present with the expected sizes.
- [x] 4.4 Smoke test restart: start the daemon again with files present; confirm no re-download occurs. Verify: no network fetch and the daemon proceeds to bind the hotkey.
