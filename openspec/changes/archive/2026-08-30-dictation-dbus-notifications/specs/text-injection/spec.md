# text-injection Specification (Delta)

## MODIFIED Requirements

### Requirement: Injection is triggered by transcription output

The daemon MUST expose a module entry point usable from the main daemon process such that a finished transcription's text is handed to the injector and appears in the focused application without further user action. Injection requests MUST be processed in the order received. Once every keystroke of an injected text has been written to the virtual-input device, the injector MUST report that injection as complete so downstream observers (the notification path) can announce the end of the dictation.

#### Scenario: Transcription completes

- **WHEN** a transcription result is submitted for injection
- **THEN** the text is typed into the focused application and the submitting caller is not blocked beyond handing off the request

#### Scenario: Two results in quick succession

- **WHEN** two texts are submitted for injection before the first finishes typing
- **THEN** the second is injected completely after the first, with no interleaving of keystrokes, and each completion is reported in the same order

#### Scenario: Injection finishes

- **WHEN** the last keystroke of a submitted text has been written to the device
- **THEN** the injector reports the text's injection as complete exactly once
