## Why

The `steno-daemon` process has no way to detect when the user intends to record. It must observe the keyboard so it can capture the moment the capture hotkey is pressed and the moment it is released. That press/release pair is the signal that later triggers audio capture and transcription, so intercepting the capture key is the first piece of the recording pipeline.

## What Changes

- Add keyboard event interception to `steno-daemon` so it receives global key press and release events from the keyboard.
- Define the capture key as the key combination <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> (Space as base key, Ctrl and Super as modifiers).
- When the capture key is pressed (all three keys held simultaneously), log the press event.
- When the capture key is released, log the release event.
- This change covers capture-key interception and logging only. Starting/stopping audio recording and triggering transcription are out of scope and handled by later work.

## Capabilities

### New Capabilities

- `capture-key-interception`: The `steno-daemon` capability to detect the global capture hotkey (<kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd>) press and release events and record them as log entries.

### Modified Capabilities

<!-- No existing capabilities; the specs/ directory is empty. -->
