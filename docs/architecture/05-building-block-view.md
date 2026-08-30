# Building block view

This section documents the building blocks of Steno and how they relate. It
starts from the top level — Steno in its environment — and drills down to the
containers and components as they are currently implemented.

## Context

The figure below shows Steno at the top level: the people and external systems
it interacts with. A user presses a hotkey to record their voice through
PipeWire; as soon as they release it, the recorded audio is transcribed by a
local model and the resulting text is injected as keystrokes into the virtual
input device (`/dev/uinput`). Notifications for the start of recording, the
start of transcription, and the completion of the dictation are emitted as
desktop notifications over D-Bus.

```mermaid
C4Context
    title "Context diagram for Steno"

    Person(user, "User", "Dictates text from a Linux desktop or terminal.")

    System(steno, "Steno", "Speech-to-text dictation application for Linux. Records the user's voice, transcribes it with a local model, and injects the resulting text as keystrokes.")

    System_Ext(pipewire, "PipeWire", "Audio server that captures the user's voice while the hotkey is held.")
    System_Ext(model, "Local transcription model", "On-device Parakeet TDT model that transcribes the recorded audio into text, keeping all data local for privacy.")
    System_Ext(uinput, "/dev/uinput", "Virtual input device into which the transcribed text is injected as keystrokes for the currently active window.")
    System_Ext(dbus, "D-Bus", "Session bus carrying dictation notifications to the desktop's notification daemon, which renders them for the user.")

    Rel(user, steno, "Presses and releases a hotkey", "Keyboard")
    Rel(steno, pipewire, "Records audio while the hotkey is held")
    Rel(steno, model, "Sends the recorded audio for transcription")
    Rel(model, steno, "Returns the transcribed text")
    Rel(steno, uinput, "Injects the transcribed text as keystrokes")
    Rel(steno, dbus, "Emits dictation notifications")
    Rel(dbus, user, "Delivers the notifications")
```

## Application processes

Steno is currently a single process: `steno-daemon` (binary `stenod`), an
async Rust daemon built on `tokio`. It listens for the capture hotkey
(<kbd>Ctrl</kbd>+<kbd>Super</kbd>) on evdev keyboards, records audio from
PipeWire while the hotkey is held, transcribes the audio with a local Parakeet
TDT model, injects the resulting text as keystrokes through a virtual keyboard
registered on `/dev/uinput`, and announces each stage as a standard
freedesktop desktop notification over D-Bus. It ships as a systemd user
service; a failing capture path makes the daemon exit non-zero.

The notification path does not need a dedicated client such as the previously
planned COSMIC app `steno-cosmic`: the calls ride the standard
`org.freedesktop.Notifications` interface, so the notification daemon that
ships with the user's desktop environment renders them. The `steno-cosmic`
crate is an empty scaffold and is not part of the diagrams below.
Communication stays one-directional: the daemon only emits notifications and
never accepts commands back.

```mermaid
C4Container
    title "Container diagram for Steno"

    Person(user, "User", "Dictates text from a Linux desktop or terminal.")

    System_Boundary(steno, "Steno") {
        Container(daemon, "steno-daemon", "Rust, tokio", "Long-running process (binary stenod). Listens for the capture hotkey, records audio from PipeWire, transcribes it with a local model, injects the text as keystrokes, and emits dictation notifications. Owns all audio capture, transcription, and input injection.")
    }

    System_Ext(pipewire, "PipeWire", "Audio server that captures the user's voice while the hotkey is held.")
    System_Ext(model, "Local transcription model", "On-device Parakeet TDT model that transcribes the recorded audio into text, keeping all data local for privacy.")
    System_Ext(uinput, "/dev/uinput", "Virtual input device into which the transcribed text is injected as keystrokes for the currently active window.")
    System_Ext(dbus, "D-Bus", "Session bus carrying dictation notifications to the desktop's notification daemon, which renders them for the user.")

    Rel(user, daemon, "Presses and releases Ctrl+Super", "Keyboard")
    Rel(daemon, pipewire, "Records audio while the hotkey is held")
    Rel(daemon, model, "Sends the recorded audio for transcription")
    Rel(model, daemon, "Returns the transcribed text")
    Rel(daemon, uinput, "Injects the transcribed text as keystrokes")
    Rel(daemon, dbus, "Emits dictation notifications", "org.freedesktop.Notifications")
    Rel(dbus, user, "Delivers the notifications")
```

## Daemon components

`main.rs` wires the daemon together as a handful of tasks connected by
channels:

- `KeyListener` (`listener.rs`) grabs the evdev keyboards and samples the
  held modifiers every 15 ms; a pure `CaptureState` state machine turns
  `Ctrl+Super` transitions into `Start`/`Stop` commands for the `Recorder`.
- `Recorder` (`recorder.rs`) owns the `CaptureSession` (`capture.rs`), a
  PipeWire stream running on its own thread that captures the default
  microphone as 16 kHz mono f32 samples. On stop, the samples are
  transcribed off the async runtime by the `ParakeetTDT` session, and the
  resulting text is handed to the `Injector` through a FIFO channel. In
  `--debug` mode each recording is also written as a WAV file
  (`wav.rs`) under `/tmp/steno`.
- `Injector` (`uinput.rs`) translates text into keystrokes on a US-QWERTY
  keymap and writes each character as a single batch to the virtual keyboard
  `steno-virtual-keyboard`; characters with no keyboard representation are
  skipped, never fatal.
- `Notifier` (`notifications.rs`) owns the D-Bus session connection and turns
  each `DictationEvent` into an `org.freedesktop.Notifications.Notify` call:
  `RecordingStarted` and `TranscriptionStarted` come from the `Recorder`,
  `DictationFinished` from the `Injector` once the text has fully landed.
  Delivery is best-effort — an unreachable bus downgrades notifications to
  discard mode and never affects recording, transcription, or injection.
- `model.rs` provisions the pinned Parakeet TDT files, downloading them from
  Hugging Face on first run into `$XDG_CONFIG_HOME/steno/models/parakeet`.
  The session loads with the CUDA execution provider when built with the
  default `cuda` feature, falling back to CPU. The `KeyListener` is only
  constructed after the model has loaded, so `Ctrl+Super` is never swallowed
  while nothing can be captured.

```mermaid
C4Container
    title "Component diagram for steno-daemon"

    Person(user, "User", "Dictates text from a Linux desktop or terminal.")

    System_Boundary(steno, "Steno") {
        Container_Boundary(daemon, "steno-daemon (stenod)") {
            Component(listener, "KeyListener", "listener.rs", "Samples the grabbed evdev keyboards every 15 ms; the CaptureState state machine turns Ctrl+Super transitions into Start/Stop commands.")
            Component(recorder, "Recorder", "recorder.rs", "Handles Start/Stop, owns the capture session, triggers transcription, and forwards the finished text to the injector.")
            Component(capture, "CaptureSession", "capture.rs", "PipeWire stream on a dedicated thread; captures the default microphone as 16 kHz mono f32 samples.")
            Component(parakeet, "ParakeetTDT", "model.rs + parakeet-rs", "Local transcription session over the pinned Parakeet TDT model files, serialized by a mutex.")
            Component(wav, "wav", "wav.rs", "Writes a captured recording as a 16 kHz mono WAV in debug mode.")
            Component(injector, "Injector", "uinput.rs", "Translates text into US-QWERTY keystrokes and writes each character as one batch to the virtual keyboard.")
            Component(notifier, "Notifier", "notifications.rs", "Turns each DictationEvent into a freedesktop desktop notification on the session bus.")
        }
    }

    System_Ext(evdev, "evdev keyboards", "Physical keyboards grabbed by the daemon through kbd-global; the source of the held-modifier state.")
    System_Ext(pipewire, "PipeWire", "Audio server providing the default microphone stream.")
    System_Ext(hf, "Hugging Face", "Model repository the pinned Parakeet TDT files are downloaded from on first run.")
    System_Ext(uinput, "/dev/uinput", "Virtual input device the transcribed keystrokes are injected through.")
    System_Ext(dbus, "D-Bus", "Session bus carrying the notification method calls to the desktop's notification daemon.")

    Rel(user, listener, "Holds / releases Ctrl+Super", "Keyboard")
    Rel(listener, evdev, "Samples held modifiers", "kbd-global (evdev)")
    Rel(listener, recorder, "RecorderCommand: Start / Stop", "mpsc channel")
    Rel(recorder, capture, "Starts / stops the capture session")
    Rel(capture, pipewire, "Captures the default microphone", "16 kHz mono f32")
    Rel(recorder, wav, "Writes the debug WAV")
    Rel(recorder, parakeet, "Transcribes the captured samples")
    Rel(recorder, injector, "The transcribed text", "mpsc channel")
    Rel(injector, uinput, "Injects the keystrokes")
    Rel(parakeet, hf, "Downloads the model files on first run")
    Rel(recorder, notifier, "RecordingStarted / TranscriptionStarted", "mpsc channel")
    Rel(injector, notifier, "DictationFinished", "mpsc channel")
    Rel(notifier, dbus, "org.freedesktop.Notifications.Notify")
```
