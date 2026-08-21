mod capture_key;

fn main() {
    // Initialize the logger so `log::info!` output (the capture press/release
    // log lines) is actually emitted. `env_logger` defaults to the `error`
    // filter, which drops every one of those lines, so default to `info` and
    // let `RUST_LOG` override it when more or less detail is wanted.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .init();

    // The capture key is Ctrl+Super+Space: the Ctrl and Super modifiers held
    // with Space as the base key, since a `kbd` hotkey needs a non-modifier
    // base key to bind to.
    let capture_key = capture_key::capture_hotkey();

    if let Err(error) = capture_key::run(capture_key) {
        eprintln!("capture_key: {error}");
        std::process::exit(1);
    }
}
