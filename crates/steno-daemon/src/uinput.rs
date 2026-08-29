//! Text injection through the kernel's `/dev/uinput` virtual-input device.
//!
//! The text-to-keystroke translation is pure and OS-free so it can be
//! unit-tested without a device; only the injector touches `/dev/uinput`.

use uinput::event::keyboard::Key;

/// A single keyboard transition: press (`true`) or release (`false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub press: bool,
}

impl KeyEvent {
    const fn press_of(key: Key) -> Self {
        Self { key, press: true }
    }

    const fn release_of(key: Key) -> Self {
        Self { key, press: false }
    }

    /// A key typed without any modifier held.
    fn typed(key: Key) -> [Self; 2] {
        [Self::press_of(key), Self::release_of(key)]
    }

    /// A key typed with shift held for exactly this keystroke.
    fn shifted(key: Key) -> [Self; 4] {
        [
            Self::press_of(Key::LeftShift),
            Self::press_of(key),
            Self::release_of(key),
            Self::release_of(Key::LeftShift),
        ]
    }
}

/// The result of translating text into keystrokes: the events to emit and
/// the characters that have no keyboard representation (they are skipped,
/// never fatal).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Translation {
    /// One entry per translated character: its press/release group.
    pub groups: Vec<Vec<KeyEvent>>,
    pub skipped: Vec<char>,
}

/// Translate text into keyboard events on a US-QWERTY keymap. Uppercase
/// letters and shifted symbols wrap their keystroke in a LeftShift
/// press/release; characters outside the supported set are reported as
/// skipped instead of aborting the translation.
pub fn translate(text: &str) -> Translation {
    let mut translation = Translation::default();

    for ch in text.chars() {
        match char_events(ch) {
            Some(events) => translation.groups.push(events),
            None => translation.skipped.push(ch),
        }
    }

    translation
}

/// Map one character to its keystroke group, if representable.
fn char_events(ch: char) -> Option<Vec<KeyEvent>> {
    if ch.is_ascii_alphabetic() {
        let key = ascii_alpha_key(ch.to_ascii_lowercase())?;
        return Some(if ch.is_ascii_uppercase() {
            KeyEvent::shifted(key).to_vec()
        } else {
            KeyEvent::typed(key).to_vec()
        });
    }

    if let Some((key, shift)) = keymap_entry(ch) {
        return Some(if shift {
            KeyEvent::shifted(key).to_vec()
        } else {
            KeyEvent::typed(key).to_vec()
        });
    }

    whitespace_or_digit(ch).map(|key| KeyEvent::typed(key).to_vec())
}

/// Lowercase letters a–z indexed by letter position.
static UNSHIFTED_ALPHA: [Key; 26] = [
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
];

/// Digits 0–9 as unshifted keysyms (the shifted forms are the symbols).
static UNSHIFTED_DIGIT: [Key; 10] = [
    Key::_0,
    Key::_1,
    Key::_2,
    Key::_3,
    Key::_4,
    Key::_5,
    Key::_6,
    Key::_7,
    Key::_8,
    Key::_9,
];

fn ascii_alpha_key(lower: char) -> Option<Key> {
    UNSHIFTED_ALPHA.get(lower as usize - 'a' as usize).copied()
}

fn ascii_digit_key(digit: char) -> Option<Key> {
    UNSHIFTED_DIGIT.get(digit as usize - '0' as usize).copied()
}

/// Whitespace and unshifted digits, which have no shift-pair mapping here.
fn whitespace_or_digit(ch: char) -> Option<Key> {
    match ch {
        ' ' => Some(Key::Space),
        '\t' => Some(Key::Tab),
        '\n' => Some(Key::Enter),
        '0'..='9' => ascii_digit_key(ch),
        _ => None,
    }
}

/// Punctuation as (physical key, shift required) on US-QWERTY: unshifted
/// symbol keys and the shifted symbols reachable from the number row and
/// symbol keys.
fn keymap_entry(ch: char) -> Option<(Key, bool)> {
    let entry = match ch {
        // Unshifted symbol keys.
        '-' => (Key::Minus, false),
        '=' => (Key::Equal, false),
        '[' => (Key::LeftBrace, false),
        ']' => (Key::RightBrace, false),
        ';' => (Key::SemiColon, false),
        '\'' => (Key::Apostrophe, false),
        '`' => (Key::Grave, false),
        '\\' => (Key::BackSlash, false),
        ',' => (Key::Comma, false),
        '.' => (Key::Dot, false),
        '/' => (Key::Slash, false),
        // Shifted symbols: typed as Shift + the unshifted key.
        '!' => (Key::_1, true),
        '@' => (Key::_2, true),
        '#' => (Key::_3, true),
        '$' => (Key::_4, true),
        '%' => (Key::_5, true),
        '^' => (Key::_6, true),
        '&' => (Key::_7, true),
        '*' => (Key::_8, true),
        '(' => (Key::_9, true),
        ')' => (Key::_0, true),
        '_' => (Key::Minus, true),
        '+' => (Key::Equal, true),
        '{' => (Key::LeftBrace, true),
        '}' => (Key::RightBrace, true),
        ':' => (Key::SemiColon, true),
        '"' => (Key::Apostrophe, true),
        '~' => (Key::Grave, true),
        '|' => (Key::BackSlash, true),
        '<' => (Key::Comma, true),
        '>' => (Key::Dot, true),
        '?' => (Key::Slash, true),
        _ => return None,
    };
    Some(entry)
}

/// Abstraction over the virtual keyboard so the injector can be driven in
/// tests without `/dev/uinput`. One call emits exactly one character's
/// event group, flushed as a single batch so the kernel (and therefore the
/// focused application) sees press and release in order with nothing
/// interleaved.
pub trait Device {
    fn write_events(&mut self, events: &[KeyEvent]) -> anyhow::Result<()>;
}

/// The real device: a virtual keyboard registered on the system uinput
/// device. The kernel destroys it automatically when this process exits.
pub struct UinputDevice {
    device: uinput::Device,
}

impl UinputDevice {
    /// Create the virtual keyboard via the system's default uinput device.
    ///
    /// Fails when `/dev/uinput` cannot be opened or the device cannot be
    /// created — missing write access is the common cause; the error says
    /// so because the daemon must not run without an injection path.
    pub fn open() -> anyhow::Result<Self> {
        let device = uinput::default()
            .map_err(open_error)?
            .name(DEVICE_NAME)
            .map_err(open_error)?
            .event(uinput::event::Keyboard::All)
            .map_err(open_error)?
            .create()
            .map_err(open_error)?;
        Ok(Self { device })
    }
}

/// The name the virtual keyboard is registered under.
const DEVICE_NAME: &str = "steno-virtual-keyboard";

fn open_error(err: uinput::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "cannot create virtual keyboard on /dev/uinput: {err} \
         (is /dev/uinput present and writable? a udev rule or group \
         membership is typically required)"
    )
}

impl Device for UinputDevice {
    fn write_events(&mut self, events: &[KeyEvent]) -> anyhow::Result<()> {
        for event in events {
            let value = if event.press { 1 } else { 0 };
            self.device
                .send(event.key, value)
                .map_err(|err| anyhow::anyhow!("uinput write failed: {err}"))?;
        }
        self.device
            .synchronize()
            .map_err(|err| anyhow::anyhow!("uinput sync failed: {err}"))?;
        Ok(())
    }
}

/// Types text into the focused application through a [`Device`].
pub struct Injector<D: Device> {
    device: D,
}

impl<D: Device> Injector<D> {
    pub fn new(device: D) -> Self {
        Self { device }
    }

    /// Inject `text`: unsupported characters are skipped with a warning,
    /// every supported character is written as one event group.
    pub fn inject(&mut self, text: &str) -> anyhow::Result<()> {
        let translation = translate(text);
        warn_skipped(&translation);
        self.write_groups(&translation)
    }

    /// Write each character's event group, one atomic batch per character.
    fn write_groups(&mut self, translation: &Translation) -> anyhow::Result<()> {
        for group in &translation.groups {
            self.device.write_events(group)?;
        }
        Ok(())
    }
    /// Task loop: own the device, drain injection requests in FIFO order,
    /// and drop the device on shutdown. One writer guarantees that
    /// injections never interleave keystrokes.
    pub async fn listen(
        mut self,
        mut rx: tokio::sync::mpsc::Receiver<String>,
        ct: tokio_util::sync::CancellationToken,
    ) {
        while let Some(text) = Self::next_text(&mut rx, &ct).await {
            if let Err(err) = self.inject(&text) {
                tracing::error!("text injection failed: {err:#}");
            }
        }
        // Exiting drops the device, destroying the virtual keyboard.
    }

    /// The next injection request, or None once the channel is closed or
    /// cancellation is requested.
    async fn next_text(
        rx: &mut tokio::sync::mpsc::Receiver<String>,
        ct: &tokio_util::sync::CancellationToken,
    ) -> Option<String> {
        tokio::select! {
            _ = ct.cancelled() => None,
            text = rx.recv() => text,
        }
    }
}

/// Log one warning per character that has no keyboard mapping.
fn warn_skipped(translation: &Translation) {
    for ch in &translation.skipped {
        tracing::warn!("skipping character with no keyboard mapping: {ch:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc::channel;
    use tokio_util::sync::CancellationToken;

    impl Translation {
        fn flat_events(&self) -> Vec<KeyEvent> {
            self.groups.iter().flatten().copied().collect()
        }
    }

    /// Device that records every event group it was handed.
    #[derive(Clone, Default)]
    struct MockDevice(Arc<Mutex<Vec<Vec<KeyEvent>>>>);

    impl MockDevice {
        /// Every group recorded, in order.
        fn groups(&self) -> Vec<Vec<KeyEvent>> {
            self.0.lock().unwrap().clone()
        }

        /// All events across groups, flattened in order.
        fn recorded(&self) -> Vec<KeyEvent> {
            self.groups().into_iter().flatten().collect()
        }
    }

    impl Device for MockDevice {
        fn write_events(&mut self, events: &[KeyEvent]) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(events.to_vec());
            Ok(())
        }
    }

    #[tokio::test]
    async fn injector_task_orders_two_texts_no_interleaving() {
        let mock = MockDevice::default();
        let injector = Injector::new(mock.clone());
        let (tx, rx) = channel::<String>(16);
        let ct = CancellationToken::new();
        let task = tokio::spawn(injector.listen(rx, ct.clone()));

        tx.send("ab".to_string()).await.unwrap();
        tx.send("cd".to_string()).await.unwrap();
        drop(tx); // channel closed -> task exits after draining
        task.await.unwrap();

        let all = mock.recorded();
        let keys: Vec<(Key, bool)> = all.iter().map(|e| (e.key, e.press)).collect();
        // First text entirely before second text, each group a+flush.
        assert_eq!(
            keys,
            vec![
                (Key::A, true),
                (Key::A, false),
                (Key::B, true),
                (Key::B, false),
                (Key::C, true),
                (Key::C, false),
                (Key::D, true),
                (Key::D, false),
            ]
        );
    }

    #[tokio::test]
    async fn injector_task_exits_on_cancel() {
        let injector = Injector::new(MockDevice::default());
        let (_tx, rx) = channel::<String>(16);
        let ct = CancellationToken::new();
        let cancel = ct.clone();
        let task = tokio::spawn(injector.listen(rx, ct));
        cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .expect("task exits on cancel")
            .unwrap();
    }

    #[test]
    fn inject_shift_sequence_for_bang() {
        let mock = MockDevice::default();
        let mut injector = Injector::new(mock.clone());
        injector.inject("Hi!").unwrap();

        let all = mock.groups();
        assert_eq!(all.len(), 3, "one group per character");
        // '!' typed as one atomic group: shift press, 1 press, 1 release, shift release.
        assert_eq!(
            all[2],
            vec![
                KeyEvent::press_of(Key::LeftShift),
                KeyEvent::press_of(Key::_1),
                KeyEvent::release_of(Key::_1),
                KeyEvent::release_of(Key::LeftShift),
            ]
        );
        // Shift released before end of group — no leak into the next char.
        assert_eq!(
            all[1],
            vec![KeyEvent::press_of(Key::I), KeyEvent::release_of(Key::I)]
        );
    }

    fn press(key: Key) -> KeyEvent {
        KeyEvent { key, press: true }
    }

    fn release(key: Key) -> KeyEvent {
        KeyEvent { key, press: false }
    }

    fn typed(key: Key) -> Vec<KeyEvent> {
        vec![press(key), release(key)]
    }

    fn shifted(key: Key) -> Vec<KeyEvent> {
        vec![
            press(Key::LeftShift),
            press(key),
            release(key),
            release(Key::LeftShift),
        ]
    }

    #[test]
    fn translate_plain_lowercase() {
        let t = translate("hello world");
        let mut expected = Vec::new();
        for k in [
            Key::H,
            Key::E,
            Key::L,
            Key::L,
            Key::O,
            Key::Space,
            Key::W,
            Key::O,
            Key::R,
            Key::L,
            Key::D,
        ] {
            expected.extend(typed(k));
        }
        assert_eq!(expected, t.flat_events());
        assert!(t.skipped.is_empty());
    }

    #[test]
    fn translate_shift_wraps_only_shifted_chars() {
        let t = translate("Hi! It's 3pm.");
        let mut expected = Vec::new();
        expected.extend(shifted(Key::H));
        expected.extend(typed(Key::I));
        expected.extend(shifted(Key::_1)); // !
        expected.extend(typed(Key::Space));
        expected.extend(shifted(Key::I));
        expected.extend(typed(Key::T));
        expected.extend(typed(Key::Apostrophe));
        expected.extend(typed(Key::S));
        expected.extend(typed(Key::Space));
        expected.extend(typed(Key::_3)); // 3 is unshifted in "3pm"
        expected.extend(typed(Key::P));
        expected.extend(typed(Key::M));
        expected.extend(typed(Key::Dot));
        assert_eq!(expected, t.flat_events());
        assert!(t.skipped.is_empty());
    }

    #[test]
    fn translate_whitespace() {
        let t = translate("a\nb\tc");
        let mut expected = Vec::new();
        expected.extend(typed(Key::A));
        expected.extend(typed(Key::Enter));
        expected.extend(typed(Key::B));
        expected.extend(typed(Key::Tab));
        expected.extend(typed(Key::C));
        assert_eq!(expected, t.flat_events());
        assert!(t.skipped.is_empty());
    }

    #[test]
    fn translate_skips_unsupported_chars() {
        let t = translate("great \u{1f389} thanks");
        assert_eq!(vec!['\u{1f389}'], t.skipped);

        let mut expected = Vec::new();
        for k in [
            Key::G,
            Key::R,
            Key::E,
            Key::A,
            Key::T,
            Key::Space,
            Key::Space,
            Key::T,
            Key::H,
            Key::A,
            Key::N,
            Key::K,
            Key::S,
        ] {
            expected.extend(typed(k));
        }
        assert_eq!(expected, t.flat_events());
    }

    #[test]
    fn translate_skips_non_latin_letters() {
        let t = translate("\u{e9}");
        assert_eq!(vec!['\u{e9}'], t.skipped);
        assert!(t.flat_events().is_empty());
    }
}
