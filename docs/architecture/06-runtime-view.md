# Runtime view

This section describes how Steno behaves at runtime. Component names refer to
the [Building block view](05-building-block-view.md).

## Recording flow

One dictation, from hotkey press to injected text:

```mermaid
sequenceDiagram
    autonumber
    actor U as User
    participant KL as KeyListener
    participant R as Recorder
    participant C as CaptureSession
    participant P as PipeWire
    participant M as ParakeetTDT
    participant I as Injector
    participant N as Notifier
    participant DB as D-Bus
    participant UI as /dev/uinput

    U->>KL: Hold Ctrl+Super
    loop evdev poll every 15 ms
        KL->>KL: Sample held modifiers (CaptureState)
    end
    KL->>R: RecorderCommand::Start (mpsc)
    R->>C: Start capture on dedicated PipeWire thread
    C->>P: Open 16 kHz mono f32 microphone stream
    alt Stream reaches streaming state within 3 s
        C-->>R: Ready
        R->>N: RecordingStarted
        N->>DB: org.freedesktop.Notifications.Notify
        P-->>C: Samples accumulate while held
        U->>KL: Release Ctrl+Super
        KL->>R: RecorderCommand::Stop
        R->>C: stop (spawn_blocking)
        C-->>R: Captured samples (+ capture error, if any)
        opt Debug mode (--debug)
            R->>R: Write WAV to /tmp/steno
        end
        alt Samples present
            R->>N: TranscriptionStarted
            N->>DB: org.freedesktop.Notifications.Notify
            R--)M: transcribe_samples (spawn_blocking)
            Note over R,M: Fire-and-forget — the model mutex serializes concurrent inference
            M-->>R: Transcribed text
            R->>I: Text (mpsc, FIFO)
            I->>UI: Inject keystrokes (US-QWERTY, one batch per character)
            I->>N: DictationFinished
            N->>DB: org.freedesktop.Notifications.Notify
        else Capture produced no audio
            R->>R: Log warning, flow ends
        end
    else Capture fails to start
        R->>C: stop
        R->>R: Log error, flow ends
    end
```

Behavior the diagram cannot show:

- A new press during inference starts a second recording immediately; the
  model mutex queues its transcription behind the first.
- Notifications are best-effort: with no session bus the `Notifier` discards
  events and the rest of the flow is unaffected.
- Characters with no US-QWERTY mapping are skipped with a warning; injection
  of the remaining text continues.
