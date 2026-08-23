# model-provisioning Specification

## Purpose

The `model-provisioning` capability ensures the parakeet ONNX model files that the
transcription model needs are present at `~/.config/steno/models/parakeet` — stored
flat, without subdirectories — downloading them from HuggingFace when they are absent,
so the daemon can transcribe audio without any manual, one-time setup step.

## MODIFIED Requirements

### Requirement: Model directory location

The daemon MUST store the parakeet model files flat in `~/.config/steno/models/parakeet`, with each required file placed directly in that directory under its basename (no `tdt/` subdirectory). The directory is derived from the user's config directory: `$XDG_CONFIG_HOME` when set, otherwise `$HOME/.config`. When neither can be derived, provisioning MUST fail with an error rather than falling back to a temporary or world-readable location. The model directory MUST NOT be changed by configuration in this capability.

#### Scenario: Location is derived from the user's config dir

- **WHEN** the daemon resolves the model directory
- **THEN** it uses `$XDG_CONFIG_HOME/steno/models/parakeet` if `$XDG_CONFIG_HOME` is set, otherwise `$HOME/.config/steno/models/parakeet`

#### Scenario: Files are stored flat

- **WHEN** the daemon stores a required model file locally
- **THEN** the file is placed at `<model dir>/<basename>` without a `tdt/` subdirectory

#### Scenario: Config dir cannot be derived

- **WHEN** neither `$XDG_CONFIG_HOME` nor `$HOME` is set
- **THEN** provisioning fails with an error and no model files are written

### Requirement: Missing files trigger a download

The daemon MUST detect that the required parakeet model files are absent from the model directory and, for each absent file, download it from the HuggingFace repository `altunenes/parakeet-rs` at its repository path `tdt/<basename>` into the flat local location `<model dir>/<basename>`. A file is absent when it does not exist or is empty. A file that exists and is non-empty MUST NOT be re-downloaded.

#### Scenario: Files missing on first run

- **WHEN** none of the required parakeet model files exist in `~/.config/steno/models/parakeet`
- **THEN** the daemon downloads the required files from `altunenes/parakeet-rs` into that directory, flat under the directory

#### Scenario: Files already present

- **WHEN** all of the required parakeet model files already exist in `~/.config/steno/models/parakeet` and are non-empty
- **THEN** the daemon does not download them again

#### Scenario: Empty file is treated as absent

- **WHEN** a required model file exists but has a size of zero
- **THEN** the daemon downloads that file

### Requirement: Correct files are fetched

The daemon MUST fetch exactly the files the ParakeetTDT transcription model loads, from the `tdt/` directory of the HuggingFace repository `altunenes/parakeet-rs`: `decoder_joint-model.onnx`, `encoder-model.onnx`, `encoder-model.onnx.data`, and `vocab.txt`. The int8 variants (`decoder_joint-model.int8.onnx`, `encoder-model.int8.onnx`) MUST NOT be fetched, because the model loader resolves them only as fallbacks that are shadowed by the full-precision files. `nemo128.onnx` MUST NOT be fetched, because the transcription model does not reference it.

#### Scenario: Required set is complete

- **WHEN** the daemon finishes downloading the model
- **THEN** `decoder_joint-model.onnx`, `encoder-model.onnx`, `encoder-model.onnx.data`, and `vocab.txt` are present in the model directory, flat under the directory

#### Scenario: Unrelated files are not fetched

- **WHEN** the HuggingFace repository contains files that are not among the required parakeet model files
- **THEN** the daemon does not download those files

### Requirement: Integrity is verified

The daemon MUST verify each downloaded file against its pinned expected byte size before the file becomes visible in the model directory, so that a corrupt, truncated, or tampered download is rejected and a failed model is never reported ready. A downloaded file whose size does not match MUST be deleted and provisioning MUST fail. Files MUST be downloaded to a staging location and moved into the model directory only after verification, so an interrupted download MUST NOT leave a partial file in the model directory. Size verification catches truncation and corruption; content-hash verification is a follow-up hardening, not part of this requirement.

#### Scenario: Download is corrupted

- **WHEN** a downloaded file's byte count does not match its pinned expected size
- **THEN** the daemon deletes the file and reports a provisioning failure without presenting the model as ready

#### Scenario: Interrupted download leaves no partial file

- **WHEN** a download fails before the file verifies
- **THEN** no partial file for that file exists in the model directory

#### Scenario: Verified file is placed into the model directory

- **WHEN** a downloaded file matches its pinned expected size
- **THEN** it is moved from the staging location to `<model dir>/<basename>`
