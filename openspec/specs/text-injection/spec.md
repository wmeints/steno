# text-injection Specification

## Purpose

The `steno-daemon` capability to deliver text — the transcription output — into whichever application currently has keyboard focus, by emitting it as keyboard input through the kernel's virtual-input device, without integrating with any desktop environment or compositor protocol.

## Requirements

### Requirement: Text injection as keyboard input

The daemon MUST be able to inject a given string of text so that it appears as typed keyboard input in the application that currently holds focus. Injection MUST NOT depend on the desktop environment or compositor in use.

#### Scenario: Inject plain text

- **WHEN** the daemon injects the text "hello world" into an application with keyboard focus
- **THEN** that application receives the characters `hello world` as if typed on a physical keyboard, in order

#### Scenario: Focus changes between captures

- **WHEN** the user focuses a different application and the daemon injects text
- **THEN** the text lands in the newly focused application, with no reconfiguration

### Requirement: Supported character set

Injection MUST cover the characters dictation commonly produces: ASCII letters (upper and lower case), ASCII digits, space, tab, newline, and common punctuation (`. , ; : ! ? ' " ( ) [ ] { } - _ / \ @ # $ % ^ & * + = < > ~ | ` `). Upper-case letters and shifted symbols MUST be produced with the corresponding shift modifier held for exactly those keystrokes.

#### Scenario: Mixed case and symbols

- **WHEN** the daemon injects "Hi! It's 3pm."
- **THEN** the application receives the exact string, with capitals and symbols typed with shift and shift released before the next unshifted character

#### Scenario: Multi-line text

- **WHEN** the daemon injects text containing a newline
- **THEN** the application receives an Enter keypress at that position

### Requirement: Unsupported characters are skipped

When the text contains a character the injection cannot represent (for example a non-Latin script or an emoji), the daemon MUST skip that character, MUST log a warning identifying it, and MUST continue injecting the remaining characters. An unsupported character MUST NOT abort the injection of the rest of the text.

#### Scenario: Emoji in transcription

- **WHEN** the daemon injects "great 🎉 thanks"
- **THEN** the application receives "great  thanks" and the daemon logs one warning about the skipped character

### Requirement: Delivery through the virtual-input device

The daemon MUST create its virtual keyboard device when it starts injecting-capable operation and MUST destroy the device when the daemon exits, so no phantom keyboard is left registered on the system. If the virtual-input device cannot be opened (missing `/dev/uinput` or insufficient permissions), the daemon MUST fail to start with a clear error rather than run in a state where captured text has nowhere to go.

#### Scenario: Device lifecycle

- **WHEN** the daemon runs and then exits
- **THEN** the virtual keyboard appears in the system's input device list while running and is gone after exit

#### Scenario: Permission denied at startup

- **WHEN** the daemon starts without write access to `/dev/uinput`
- **THEN** the daemon exits non-zero with an error naming the device and the likely permission cause

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
