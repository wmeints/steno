# Solution strategy

This section records the technology choices that make up Steno and how each
choice supports the quality goals defined in [Introduction and
goals](01-introduction-and-goals.md).

## Components

Steno is split into a daemon and a client:

- **`steno-daemon`** — a Rust process that records audio from PipeWire,
  transcribes it with a local model, and injects the text as keystrokes. It
  owns all audio capture, transcription, and input injection.
- **`steno-cosmic`** — a Rust COSMIC client that displays the recording
  notifications broadcast by the daemon. It is a thin, presentational client.

Communication between the two is one-directional: the daemon broadcasts
notifications over D-Bus, and the client only listens. The client never sends
commands back to the daemon.

## Technology choices

### Rust for `steno-daemon` and `steno-cosmic`

Both the daemon and the client are written in Rust. Rust gives the daemon
memory safety and predictable resource handling, which matters for a
long-running process that owns audio capture, model inference, and input
injection. Its type system and concurrency model also fit the shape of the
application: a persistent daemon paired with a lightweight client.

### Parakeet for transcription

[Parakeet][parakeet] from Nvidia is used as the local transcription model. It
currently offers the best transcription accuracy for this use case, runs
entirely on the user's machine, and provides noticeably better Dutch support
than [Whisper][whisper]. Because it runs locally, it keeps transcription on
device for privacy.

### D-Bus for notifications

The daemon sends notifications to the client over D-Bus. Communication is
one-directional — the client listens but never sends commands back to the
daemon — so the daemon cannot be driven by the client. This closes a class of
injection attacks in which a malicious or compromised client could steer the
daemon into transcribing or injecting attacker-controlled text.

### `/dev/uinput` for text injection

Transcribed text is injected into the active window through `/dev/uinput`
rather than through an accessibility or windowing API. A virtual input device
is seen by the operating system as a real keyboard, so the injected text
reaches any application, regardless of desktop environment or whether the
application exposes accessibility APIs.

## Approaches to quality goals

- **Only the spoken text is transcribed and sent to the input device.** The
  daemon records only the audio captured from PipeWire while the hotkey is
  held, transcribes only that audio, and injects only the resulting text. The
  one-directional D-Bus link means no external party — notably the client — can
  hand the daemon text to transcribe or inject, so the only text that ever
  reaches the input device is the user's own voice.

- **Only local models are used for privacy.** Transcription is performed by
  Parakeet running on the user's machine; no audio or transcript leaves the
  device.

- **Transcribed audio is only accessible to the currently logged in user and
  stored in a dedicated user directory.** The daemon stores recorded and
  transcribed audio under a per-user directory that is owned by and accessible
  only to the logged in user.

[parakeet]: https://github.com/NVIDIA/Parakeet
[whisper]: https://github.com/openai/whisper
