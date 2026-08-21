## Purpose

The `model-provisioning` capability ensures the parakeet ONNX model files that
the transcription model needs are present at `~/.config/steno/models/parakeet`,
downloading them from HuggingFace when they are absent, so the daemon can
transcribe audio without any manual, one-time setup step.

## ADDED Requirements

### Requirement: Model directory location

The daemon MUST store the parakeet model files in `~/.config/steno/models/parakeet`. The path `~/.config/steno/models/parakeet` MUST be the fixed, user-writable location; it MUST NOT be changed by configuration or environment.

#### Scenario: Location is fixed

- **WHEN** the daemon resolves the model directory
- **THEN** it uses `~/.config/steno/models/parakeet`

### Requirement: Missing files trigger a download

The daemon MUST detect that the required parakeet model files are absent from the model directory and, when they are absent, download the correct files from the HuggingFace repository `altunenes/parakeet-rs` into `~/.config/steno/models/parakeet`.

#### Scenario: Files missing on first run

- **WHEN** none of the required parakeet model files exist in `~/.config/steno/models/parakeet`
- **THEN** the daemon downloads the required files from `altunenes/parakeet-rs` into that directory

#### Scenario: Files already present

- **WHEN** all of the required parakeet model files already exist in `~/.config/steno/models/parakeet`
- **THEN** the daemon does not download them again

### Requirement: Correct files are fetched

The daemon MUST fetch exactly the files that the parakeet transcription model requires from HuggingFace, namely `config.json`, `decoder_joint.onnx`, `encoder.onnx`, `encoder.onnx.data`, and `tokenizer.model`.

#### Scenario: Required set is complete

- **WHEN** the daemon finishes downloading the model
- **THEN** `config.json`, `decoder_joint.onnx`, `encoder.onnx`, `encoder.onnx.data`, and `tokenizer.model` are present in the model directory

#### Scenario: Unrelated files are not fetched

- **WHEN** the HuggingFace repository contains files that are not among the required parakeet model files
- **THEN** the daemon does not download those files

### Requirement: Integrity is verified

The daemon MUST verify the downloaded files against their known, expected content so that a corrupt, truncated, or tampered download is rejected, and MUST NOT present a model that failed verification as ready.

#### Scenario: Download is corrupted

- **WHEN** a downloaded file does not match its expected content
- **THEN** the daemon rejects the file and does not report the model as ready

### Requirement: Provisioning occurs during startup

The daemon MUST ensure the model is provisioned as part of its startup, before the transcription model is loaded for use.

#### Scenario: Provisioning precedes transcription

- **WHEN** the daemon starts and the model is not yet provisioned
- **THEN** the model is downloaded before the transcription model is loaded

### Requirement: Provisioning errors are reported

When the daemon cannot provision the model — for example, the network is unreachable or a required file cannot be fetched — it MUST report the failure and MUST NOT proceed as if the model were ready.

#### Scenario: Network unavailable

- **WHEN** a required file cannot be downloaded because the network is unreachable
- **THEN** the daemon reports a provisioning failure
