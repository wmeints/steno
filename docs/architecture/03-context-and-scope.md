# Context and scope

This section covers the context and scope of the system. 

The figure below shows Steno in its environment: the people and external
systems it interacts with. A user presses a hotkey to record their voice
through PipeWire; as soon as they release it, the recorded audio is
transcribed by a local model and the resulting text is injected as keystrokes
into the virtual input device (`/dev/uinput`). Notifications for the start of
recording, the start of transcription, and the completion of the dictation are
emitted as desktop notifications over D-Bus.

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
