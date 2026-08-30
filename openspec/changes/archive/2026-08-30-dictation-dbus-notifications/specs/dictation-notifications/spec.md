# dictation-notifications Specification (Delta)

## Purpose

The `steno-daemon` capability to inform the user and other desktop
applications about dictation lifecycle state by sending D-Bus desktop
notifications on the session bus for the start of recording, the start of
transcription, and the end of dictation.

## ADDED Requirements

### Requirement: Dictation events are notified

The daemon MUST send a desktop notification for each of the three dictation
lifecycle events:

1. **Recording started** — when microphone capture begins.
2. **Transcription started** — when capture ends and the recorded audio is
   handed to transcription.
3. **Dictation finished** — when the transcribed text has been fully written
   to the virtual-input device.

Each notification MUST identify the event it reports in a way a user can
distinguish on screen.

#### Scenario: Full dictation cycle

- **WHEN** the user presses Ctrl+Super, speaks, and releases the key
- **THEN** the desktop shows, in order, a recording-started notification, a
  transcription-started notification, and — once the text has finished being
  typed — a dictation-finished notification

#### Scenario: Capture produces no audio

- **WHEN** a recording stops with zero captured samples
- **THEN** no transcription-started and no dictation-finished notification is
  sent (the recording-started notification was already sent)

### Requirement: Notifications are delivered over the session bus

Notifications MUST be delivered as D-Bus desktop notifications on the user's
session bus using the freedesktop Notifications interface, without the daemon
owning a bus name of its own. Delivery MUST NOT depend on a particular
desktop environment — any standards-compliant notification daemon receives
them.

#### Scenario: Notification appears in any compliant desktop

- **WHEN** the daemon emits a dictation event under a session with a
  freedesktop-compliant notification daemon running
- **THEN** that daemon displays the notification

### Requirement: Notification delivery is best-effort

A notification failure MUST NOT affect recording, transcription, or text
injection. If the session bus or notification daemon is unavailable, the
daemon MUST log the failure and continue operating normally. Emitting an
event MUST NOT block the code path that produces it beyond handing the event
to the notification channel.

#### Scenario: No notification daemon running

- **WHEN** a dictation event is emitted while no notification daemon is
  listening on the session bus
- **THEN** the daemon logs a warning and the dictation completes normally —
  text is still injected

#### Scenario: Session bus unavailable at startup

- **WHEN** the daemon starts without access to a D-Bus session bus
- **THEN** the daemon logs an error naming the session bus, continues to run,
  and all dictation functionality except notifications still works

### Requirement: Events are delivered in order

The notification stream MUST preserve event order per dictation cycle: the
dictation-finished event MUST never be notified before the recording-started
or transcription-started events of the same cycle.

#### Scenario: Rapid successive dictations

- **WHEN** the user completes two dictations back to back
- **THEN** the notifications appear in causal order for each cycle (started →
  transcribing → finished), with no finished-before-started inversion
