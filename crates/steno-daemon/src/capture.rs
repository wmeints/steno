//! PipeWire microphone capture.
//!
//! [`CaptureSession`] runs a PipeWire main loop on a dedicated thread and
//! captures the default microphone as 16 kHz mono F32LE until [`CaptureSession::stop`].

use std::sync::{Arc, Mutex};

use pipewire as pw;
use pw::spa;
use spa::param::audio::{AudioFormat, AudioInfoRaw};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::Pod;
use tokio::sync::oneshot;

/// Captured sample rate and channel count the stream requests.
const CAPTURE_RATE: u32 = 16_000;
const CAPTURE_CHANNELS: u32 = 1;

/// Readiness channel shared between the capture thread and `start()`'s
/// caller. Whoever reports the streaming outcome takes the sender so the
/// result is sent exactly once.
type Ready = Arc<Mutex<Option<oneshot::Sender<Result<(), String>>>>>;

/// One microphone capture run on a dedicated PipeWire thread.
///
/// Samples accumulate as mono f32 at [`crate::wav::SAMPLE_RATE`]. Call
/// [`CaptureSession::stop`] to end the capture and take the samples.
pub struct CaptureSession {
    samples: Arc<Mutex<Vec<f32>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Option<pw::channel::Sender<()>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl CaptureSession {
    /// Spawn the PipeWire capture thread. The returned receiver resolves to
    /// `Ok(())` once the stream is streaming, or `Err(reason)` when capture
    /// cannot start — including the concrete PipeWire setup or negotiation
    /// failure.
    pub fn start() -> (CaptureSession, oneshot::Receiver<Result<(), String>>) {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(pw::init);

        let (ready_tx, ready_rx) = oneshot::channel();
        let (stop_tx, stop_rx) = pw::channel::channel();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let thread_samples = Arc::clone(&samples);
        let thread_error = Arc::clone(&error);
        let join = std::thread::spawn(move || {
            let ready: Ready = Arc::new(Mutex::new(Some(ready_tx)));
            if let Err(reason) =
                run_capture_loop(thread_samples, Arc::clone(&ready), thread_error, stop_rx)
            {
                // Report a setup or negotiation failure to the start()
                // caller when the thread ended before announcing streaming;
                // afterwards the failure lives in the error slot for stop().
                if let Some(tx) = ready.lock().expect("ready mutex poisoned").take() {
                    let _ = tx.send(Err(reason));
                }
            }
        });

        (
            CaptureSession {
                samples,
                error,
                stop: Some(stop_tx),
                join: Some(join),
            },
            ready_rx,
        )
    }

    /// Signal the capture loop to quit, join its thread, and return the
    /// captured samples plus any error the loop hit (failed negotiation,
    /// stream error, thread panic). Samples captured before a mid-session
    /// error are still returned so a truncated take can be salvaged.
    pub fn stop(mut self) -> (Vec<f32>, Option<String>) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let panicked = match self.join.take() {
            Some(join) => join.join().is_err(),
            None => false,
        };
        let error = self
            .error
            .lock()
            .expect("error mutex poisoned")
            .take()
            .or_else(|| panicked.then(|| "capture thread panicked".to_owned()));
        let samples = std::mem::take(&mut *self.samples.lock().unwrap_or_else(|p| p.into_inner()));
        (samples, error)
    }
}

impl Drop for CaptureSession {
    /// A session dropped without [`CaptureSession::stop`] still signals the
    /// PipeWire thread to quit so the microphone is not held open. The join
    /// is deliberately not waited on here so Drop never blocks.
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            let _ = stop.send(());
        }
    }
}

struct CaptureData {
    format: AudioInfoRaw,
    samples: Arc<Mutex<Vec<f32>>>,
    ready: Ready,
    error: Arc<Mutex<Option<String>>>,
}

fn run_capture_loop(
    samples: Arc<Mutex<Vec<f32>>>,
    ready: Ready,
    error: Arc<Mutex<Option<String>>>,
    stop_rx: pw::channel::Receiver<()>,
) -> Result<(), String> {
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let core = context.connect_rc(None).map_err(|e| e.to_string())?;

    // Quit the loop when stop() signals us; the receiver must outlive run().
    let _recv = stop_rx.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });

    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };
    let stream =
        pw::stream::StreamBox::new(&core, "steno-capture", props).map_err(|e| e.to_string())?;

    let data = CaptureData {
        format: AudioInfoRaw::default(),
        samples,
        ready,
        error: Arc::clone(&error),
    };

    // Mainloop handles for the callbacks: bail out on negotiation mismatch or
    // stream error instead of capturing with an unexpected format.
    let mainloop_param = mainloop.clone();
    let mainloop_state = mainloop.clone();
    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(move |_, user_data, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            if let Err(e) = user_data.format.parse(param) {
                user_data
                    .error
                    .lock()
                    .expect("error mutex poisoned")
                    .replace(format!("failed to parse negotiated format: {e}"));
                mainloop_param.quit();
                return;
            }
            if user_data.format.format() != AudioFormat::F32LE
                || user_data.format.rate() != CAPTURE_RATE
                || user_data.format.channels() != CAPTURE_CHANNELS
            {
                user_data
                    .error
                    .lock()
                    .expect("error mutex poisoned")
                    .replace(format!(
                        "unexpected negotiated format: {:?} rate:{} channels:{}",
                        user_data.format.format(),
                        user_data.format.rate(),
                        user_data.format.channels(),
                    ));
                mainloop_param.quit();
            }
        })
        .process(|stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data() else {
                return;
            };
            let size = size.min(bytes.len());
            let mono = extract_mono_f32(&bytes[..size], CAPTURE_CHANNELS);
            user_data
                .samples
                .lock()
                .expect("samples mutex poisoned")
                .extend(mono);
        })
        .state_changed(move |_, user_data, _old, new| match new {
            pw::stream::StreamState::Streaming => {
                if let Some(tx) = user_data.ready.lock().expect("ready mutex poisoned").take() {
                    let _ = tx.send(Ok(()));
                }
            }
            pw::stream::StreamState::Error(msg) => {
                user_data
                    .error
                    .lock()
                    .expect("error mutex poisoned")
                    .replace(msg);
                mainloop_state.quit();
            }
            _ => {}
        })
        .register()
        .map_err(|e| e.to_string())?;

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    audio_info.set_rate(CAPTURE_RATE);
    audio_info.set_channels(CAPTURE_CHANNELS);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO;
    audio_info.set_position(position);

    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(spa::pod::Object {
            type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: spa::param::ParamType::EnumFormat.as_raw(),
            properties: audio_info.into(),
        }),
    )
    .map_err(|e| format!("failed to serialize format pod: {e}"))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).expect("serialized pod is valid")];

    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| format!("failed to connect capture stream: {e}"))?;

    mainloop.run();

    match error.lock().expect("error mutex poisoned").clone() {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// Interpret interleaved little-endian f32 frames and return channel 0 of
/// each frame. A trailing partial frame is ignored.
pub fn extract_mono_f32(bytes: &[u8], channels: u32) -> Vec<f32> {
    const SAMPLE_SIZE: usize = std::mem::size_of::<f32>();
    let frame_size = SAMPLE_SIZE * channels.max(1) as usize;

    bytes
        .chunks_exact(frame_size)
        .map(|frame| {
            f32::from_le_bytes(
                frame[..SAMPLE_SIZE]
                    .try_into()
                    .expect("frame is full-sized"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_bytes(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn mono_passthrough() {
        let bytes = f32_bytes(&[0.1, -0.2, 0.3]);
        assert_eq!(extract_mono_f32(&bytes, 1), vec![0.1, -0.2, 0.3]);
    }

    #[test]
    fn stereo_returns_first_channel_only() {
        // interleaved: L0 R0 L1 R1
        let bytes = f32_bytes(&[0.1, 0.9, -0.2, 0.8]);
        assert_eq!(extract_mono_f32(&bytes, 2), vec![0.1, -0.2]);
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(extract_mono_f32(&[], 1).is_empty());
        assert!(extract_mono_f32(&[], 2).is_empty());
    }

    #[test]
    fn trailing_partial_frame_ignored() {
        // one full mono sample + 3 trailing bytes
        let mut bytes = f32_bytes(&[0.5]);
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(extract_mono_f32(&bytes, 1), vec![0.5]);

        // stereo: full frame + 4 trailing bytes (not enough for a frame)
        let mut bytes = f32_bytes(&[0.5, 0.9]);
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        assert_eq!(extract_mono_f32(&bytes, 2), vec![0.5]);
    }
}
