# Building block view

This section documents the building blocks of Steno and how they relate. It
starts from the top level — Steno in its environment — and drills down to the
containers that make up the system.

## Context

The figure below shows Steno at the top level: the people and external systems
it interacts with. A user presses a hotkey to record their voice through
PipeWire; as soon as they release it, the recorded audio is transcribed by a
local model and the resulting text is injected as keystrokes into the virtual
input device (`/dev/uinput`). Notifications such as the start and stop of
recording are broadcast over D-Bus.

```mermaid
C4Context
    title "Context diagram for Steno"

    Person(user, "User", "Dictates text from a Linux desktop or terminal.")

    System(steno, "Steno", "Speech-to-text dictation application for Linux. Records the user's voice, transcribes it with a local model, and injects the resulting text as keystrokes.")

    System_Ext(pipewire, "PipeWire", "Audio server that captures the user's voice while the hotkey is held.")
    System_Ext(model, "Local transcription model", "On-device model that transcribes the recorded audio into text, keeping all data local for privacy.")
    System_Ext(uinput, "/dev/uinput", "Virtual input device into which the transcribed text is injected as keystrokes for the currently active window.")
    System_Ext(dbus, "D-Bus", "Desktop bus used to broadcast notifications such as the start and stop of recording.")

    Rel(user, steno, "Presses and releases a hotkey", "Keyboard")
    Rel(steno, pipewire, "Records audio while the hotkey is held")
    Rel(steno, model, "Sends the recorded audio for transcription")
    Rel(model, steno, "Returns the transcribed text")
    Rel(steno, uinput, "Injects the transcribed text as keystrokes")
    Rel(steno, dbus, "Emits start / stop of recording notifications")
    Rel(dbus, user, "Delivers the notifications")
```

## Application processes

Steno is split into two processes. `steno-daemon` is a process that
records audio from PipeWire, transcribes it with a local model, and injects the
text as keystrokes; it owns all audio capture, transcription, and input
injection. `steno-cosmic` is a COSMIC client that displays the recording
notifications broadcast by the daemon. Communication between the two is 
one-directional: the daemon broadcasts notifications over D-Bus, and the client 
only listens, so it never sends commands back to the daemon.

```mermaid
C4Container
    title "Container diagram for Steno"

    Person(user, "User", "Dictates text from a Linux desktop or terminal.")

    System_Boundary(steno, "Steno") {
        Container(daemon, "steno-daemon", "Rust", "Records audio from PipeWire, transcribes it with a local model, and injects the text as keystrokes. Owns all audio capture, transcription, and input injection.")
        Container(client, "steno-cosmic", "Rust", "A COSMIC client that displays the recording notifications broadcast by the daemon. A thin, presentational client.")
    }

    System_Ext(pipewire, "PipeWire", "Audio server that captures the user's voice while the hotkey is held.")
    System_Ext(model, "Local transcription model", "On-device model that transcribes the recorded audio into text, keeping all data local for privacy.")
    System_Ext(uinput, "/dev/uinput", "Virtual input device into which the transcribed text is injected as keystrokes for the currently active window.")

    Rel(user, daemon, "Presses and releases a hotkey", "Keyboard")
    Rel(daemon, pipewire, "Records audio while the hotkey is held")
    Rel(daemon, model, "Sends the recorded audio for transcription")
    Rel(model, daemon, "Returns the transcribed text")
    Rel(daemon, uinput, "Injects the transcribed text as keystrokes")
    Rel(daemon, client, "Broadcasts recording notifications", "D-Bus")
    Rel(client, user, "Displays the recording notifications")
```
