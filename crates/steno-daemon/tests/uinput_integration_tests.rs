//! End-to-end smoke test against the real `/dev/uinput` device.
//!
//! Ignored by default: it registers a virtual keyboard on the host. Run
//! with `cargo test -p steno-daemon --test uinput_integration_tests -- --ignored`.
//!
//! Verification strategy: a uinput device is paired with an evdev node
//! (`/dev/input/eventN`). Anything the virtual keyboard emits is readable
//! from that node, so the test injects text through `Injector` and reads
//! the raw kernel events back, asserting the exact keystroke sequence —
//! the same event stream a focused application receives.

use std::fs::File;
use std::io::Read;
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant};

use uinput::event::Code;
use uinput::event::keyboard::Key;

const DEVICE_NAME: &str = "steno-virtual-keyboard";
const TEXT: &str = "Hello, Steno! Line two.";

/// `struct input_event` on 64-bit Linux: timeval(16) + u16 kind + u16 code + i32 value.
const RECORD: usize = 24;
const EV_KEY: u16 = 1;

fn read_record_le(buf: &[u8]) -> (u16, u16, i32) {
    (
        u16::from_le_bytes([buf[16], buf[17]]),
        u16::from_le_bytes([buf[18], buf[19]]),
        i32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
    )
}

/// Block until the virtual keyboard is registered, returning its evdev handler (`eventN`).
fn wait_for_handler(name: &str, deadline: Instant) -> Option<String> {
    loop {
        let probe = std::fs::read_to_string("/proc/bus/input/devices")
            .expect("/proc/bus/input/devices not readable");
        for block in probe.split("\n\n") {
            if block.contains(&format!("Name=\"{name}\""))
                && let Some(line) = block.lines().find(|l| l.starts_with("H: Handlers="))
            {
                let handler = line
                    .split_whitespace()
                    .find(|t| t.starts_with("event"))
                    .expect("device block has no event handler");
                return Some(handler.to_owned());
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn has_device(name: &str) -> bool {
    std::fs::read_to_string("/proc/bus/input/devices")
        .unwrap_or_default()
        .contains(&format!("Name=\"{name}\""))
}

/// Open the paired evdev node. The node appears root-owned until udev
/// applies the input-group rule, so retry until it succeeds or expires.
fn open_evdev(node: &str) -> File {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match File::open(node) {
            Ok(file) => return file,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Err(err) => panic!("cannot read paired evdev node {node}: {err}"),
        }
    }
}

/// Stream records from `file` until it is closed or read fails.
fn spawn_reader(mut file: File) -> Receiver<Vec<u8>> {
    let (tx, rx) = channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = [0u8; RECORD];
        while file.read_exact(&mut buf).is_ok() {
            if tx.send(buf.to_vec()).is_err() {
                break;
            }
        }
    });
    rx
}

/// Drain queued kernel records into EV_KEY `(code, value)` pairs.
fn drain_events(rx: &Receiver<Vec<u8>>) -> Vec<(u16, i32)> {
    let mut events = Vec::new();
    while let Ok(record) = rx.recv_timeout(Duration::from_millis(50)) {
        let (kind, code, value) = read_record_le(&record);
        if kind == EV_KEY {
            events.push((code, value));
        }
    }
    events
}

fn flat_codes(groups: &[Vec<steno_daemon::uinput::KeyEvent>]) -> Vec<(u16, i32)> {
    groups
        .iter()
        .flatten()
        .map(|e| {
            let key: Key = e.key;
            (key.code() as u16, i32::from(e.press))
        })
        .collect()
}

#[test]
#[ignore = "registers a real virtual keyboard on the host"]
fn it_types_through_the_kernel_into_the_paired_evdev_node() {
    let device =
        steno_daemon::uinput::UinputDevice::open().expect("cannot create virtual keyboard");
    let mut injector = steno_daemon::uinput::Injector::new(device);

    let handler = wait_for_handler(DEVICE_NAME, Instant::now() + Duration::from_secs(5))
        .expect("virtual keyboard never registered");
    // Open the paired evdev node before injecting: evdev buffers are
    // per-client, so a late reader would miss the events.
    let node = format!("/dev/input/{handler}");
    let file = open_evdev(&node);
    let rx = spawn_reader(file.try_clone().unwrap());

    injector.inject(TEXT).expect("injection failed");
    injector
        .inject("skipped \u{1f389} ok")
        .expect("skip path failed");
    thread::sleep(Duration::from_millis(300));

    let events = drain_events(&rx);
    drop(file);
    drop(injector); // destroying the device must unregister the keyboard

    // Requirement "Text injection as keyboard input" + "Supported character
    // set": the exact US-QWERTY keystroke stream, in order, shift included.
    let expected = flat_codes(&steno_daemon::uinput::translate(TEXT).groups);
    assert!(!expected.is_empty());
    assert_eq!(
        expected,
        events[..expected.len()],
        "keystroke stream mismatch at the kernel"
    );

    // Requirement "Unsupported characters are skipped": the emoji in the
    // second injection produced no keystrokes, but its text did. The
    // events after the first text's stream must equal the plain translation
    // of "skipped  ok" (emoji skipped, spaces kept).
    let tail = &events[expected.len()..];
    let expected_tail = flat_codes(&steno_daemon::uinput::translate("skipped  ok").groups);
    assert_eq!(expected_tail, tail, "skip path emitted wrong keystrokes");

    // Requirement "Delivery through the virtual-input device": gone after exit.
    thread::sleep(Duration::from_millis(200));
    assert!(
        !has_device(DEVICE_NAME),
        "virtual keyboard still registered after device drop"
    );
}
