# capture-key-interception Specification

## Purpose

The `steno-daemon` capability to observe global keyboard state and detect when the user activates the capture hotkey (<kbd>Ctrl</kbd>+<kbd>Super</kbd>), so the daemon can react to the user's intent to record and, later, release the recording.

## MODIFIED Requirements

### Requirement: Capture key definition

The daemon MUST treat <kbd>Ctrl</kbd> and <kbd>Super</kbd> held simultaneously as the capture key. There is no base key: the capture key is the pair of modifiers alone. Holding only one of the two modifiers MUST NOT be considered the capture key active. The capture key MUST NOT be redefined by configuration in this capability.

The key is detected by polling the held-modifier state. The original design used <kbd>Ctrl</kbd>+<kbd>Super</kbd>+<kbd>Space</kbd> with <kbd>Space</kbd> as a registered hotkey base key; that approach was tried and did not work on the target desktop, so the capture key was changed to the polled <kbd>Ctrl</kbd>+<kbd>Super</kbd> pair (owner-confirmed working). Do not reintroduce the <kbd>Space</kbd> base key.

#### Scenario: Both modifiers held

- **WHEN** the user holds <kbd>Ctrl</kbd> and <kbd>Super</kbd> simultaneously
- **THEN** the daemon considers the capture key active

#### Scenario: Only one modifier held

- **WHEN** the user holds only <kbd>Ctrl</kbd> or only <kbd>Super</kbd>
- **THEN** the daemon does NOT consider the capture key active

### Requirement: Capture key press detection

The daemon MUST detect when the capture key transitions from inactive to active and treat that transition as a capture key press. Because the key state is polled, a press MUST be detected even when one modifier was already held before the other is pressed.

#### Scenario: Transition from no keys to full combination

- **WHEN** the user holds <kbd>Ctrl</kbd> and <kbd>Super</kbd> simultaneously
- **THEN** the daemon detects a capture key press

#### Scenario: Press while a modifier is already held

- **WHEN** <kbd>Ctrl</kbd> is held and the user then also holds <kbd>Super</kbd>
- **THEN** the daemon detects a capture key press

### Requirement: Capture key release detection

The daemon MUST detect when the capture key transitions from active to inactive and treat that transition as a capture key release. A release MUST occur as soon as either <kbd>Ctrl</kbd> or <kbd>Super</kbd> is no longer held.

#### Scenario: Release all keys together

- **WHEN** <kbd>Ctrl</kbd> and <kbd>Super</kbd> are held and the user releases both
- **THEN** the daemon detects a capture key release

#### Scenario: Release one key while the other remains held

- **WHEN** <kbd>Ctrl</kbd> and <kbd>Super</kbd> are held and the user releases only <kbd>Ctrl</kbd> (or only <kbd>Super</kbd>)
- **THEN** the daemon detects a capture key release
