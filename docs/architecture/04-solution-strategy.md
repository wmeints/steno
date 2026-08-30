# Solution strategy

This section records the technology choices that make up Steno and how each
choice supports the quality goals defined in [Introduction and
goals](01-introduction-and-goals.md).

## Components

Steno is currently a single process:

- **`steno-daemon`** (binary `stenod`) — an async Rust process that listens
  for the capture hotkey, records audio from PipeWire, transcribes it with a
  local model, injects the text as keystrokes, and emits dictation
  notifications over D-Bus. It owns all audio capture, transcription, and
  input injection.

The notifications ride the standard freedesktop desktop-notification
interface, so the notification daemon of the user's desktop environment
renders them; no dedicated client app is needed. The previously planned COSMIC
client (`steno-cosmic`) is therefore not implemented and has no place in the
building block view (see [Building block view](05-building-block-view.md)).

Communication over D-Bus is one-directional either way: the daemon only emits
notifications and exposes no control interface, so nothing listening on the
bus can send commands back to the daemon.

## Technology choices

### Rust for `steno-daemon`

The daemon is written in Rust. Rust gives it memory safety and predictable
resource handling, which matters for a long-running process that owns audio
capture, model inference, and input injection. Its type system and concurrency
model also fit the shape of the application: a single persistent process whose
work is split into async tasks connected by channels.

### Parakeet for transcription

[Parakeet TDT][parakeet] from Nvidia is the local transcription model. It
currently offers the best transcription accuracy for this use case, runs
entirely on the user's machine, and provides noticeably better Dutch support
than [Whisper][whisper]. Because it runs locally, it keeps transcription on
device for privacy.

### D-Bus for notifications

The daemon emits dictation notifications over D-Bus using the standard
`org.freedesktop.Notifications` interface, so they render on any desktop
environment without a dedicated client. Communication is one-directional —
the daemon only emits and listens for nothing — so no external party can
drive the daemon over the bus. This closes a class of injection attacks in
which a malicious or compromised listener could steer the daemon into
transcribing or injecting attacker-controlled text.

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
  one-directional D-Bus link means no external party can hand the daemon text
  to transcribe or inject, so the only text that ever reaches the input device
  is the user's own voice.

- **Only local models are used for privacy.** Transcription is performed by
  Parakeet running on the user's machine; no audio or transcript leaves the
  device.

- **Transcribed audio is only accessible to the currently logged in user and
  stored in a dedicated user directory.** The daemon stores recorded and
  transcribed audio under a per-user directory that is owned by and accessible
  only to the logged in user.

[parakeet]: https://github.com/NVIDIA/Parakeet
[whisper]: https://github.com/openai/whisper
