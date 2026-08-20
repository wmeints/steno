## Purpose

The `steno-daemon` capability to observe global keyboard events and detect when the user activates the capture hotkey (<kbd>Ctrl</kbd>+<kbd>Super</kbd>), so the daemon can react to the user's intent to record and, later, release the recording.

## ADDED Requirements

### Requirement: Capture key definition

The daemon MUST treat the key combination of <kbd>Ctrl</kbd> and <kbd>Super</kbd> pressed simultaneously as the capture key. The capture key MUST NOT be redefined by configuration in this capability.

#### Scenario: Both modifier keys held

- **WHEN** the user holds both <kbd>Ctrl</kbd> and <kbd>Super</kbd>
- **THEN** the daemon considers the capture key active

#### Scenario: Only one modifier held

- **WHEN** the user holds only <kbd>Ctrl</kbd> or only <kbd>Super</kbd>
- **THEN** the daemon does NOT consider the capture key active

### Requirement: Capture key press detection

The daemon MUST detect when the capture key transitions from inactive to active and treat that transition as a capture key press.

#### Scenario: Transition from no modifier to both modifiers

- **WHEN** neither <kbd>Ctrl</kbd> nor <kbd>Super</kbd> is held and the user then holds both
- **THEN** the daemon detects a capture key press

#### Scenario: Press while one modifier already held

- **WHEN** only <kbd>Super</kbd> is held and the user then holds <kbd>Ctrl</kbd> as well
- **THEN** the daemon detects a capture key press

### Requirement: Capture key release detection

The daemon MUST detect when the capture key transitions from active to inactive and treat that transition as a capture key release. A release MUST occur as soon as either <kbd>Ctrl</kbd> or <kbd>Super</kbd> is no longer held.

#### Scenario: Release both modifiers together

- **WHEN** both <kbd>Ctrl</kbd> and <kbd>Super</kbd> are held and the user releases both
- **THEN** the daemon detects a capture key release

#### Scenario: Release one modifier while the other remains held

- **WHEN** both <kbd>Ctrl</kbd> and <kbd>Super</kbd> are held and the user releases only <kbd>Super</kbd>
- **THEN** the daemon detects a capture key release

### Requirement: Capture key event logging

The daemon MUST log a distinct entry when the capture key is pressed and a distinct entry when the capture key is released, so an operator can verify interception from the daemon's log output.

#### Scenario: Press is logged

- **WHEN** the capture key transitions from inactive to active
- **THEN** the daemon logs a press event

#### Scenario: Release is logged

- **WHEN** the capture key transitions from active to inactive
- **THEN** the daemon logs a release event

#### Scenario: Repeated press and release cycles

- **WHEN** the user repeatedly presses and releases the capture key
- **THEN** the daemon logs a press event and a release event for each complete cycle, in order
