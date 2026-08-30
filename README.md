# Steno

Welcome to Steno, an open-source dictation tool for Linux. With this tool 
you can talk to your computer and it will type in what you said in any 
application you're working with.

## Introduction

### Why this application exists

I made this application because I find that typing gets harder every day for 
me. After doing research I found that there were great tools for Windows and
Mac, but not for Linux. So I decided to roll my own.

### Goals for this application

- Provide a dictation interface for Linux computers that supports a wide range of 
  applications e.g. terminals, web browsers, text editors, and desktop 
  applications.

- Provide a better balance between typing and talking, so you have to use your
  keyboard less to work with the computer focusing on talking to the computer
  to input larger bodies of text rather than typing them.

### Non-goals for this application

- This application is not meant to control the full user interface of the
  computer. You'll have to look for other solutions or use this application as
  inspiration.

## How this application is built

I use a fully agentic process to engineer this application. My focus is on the
harness engineering and making sure the architecture and functional specs are
correct. The coding itself is done via [Qwen 3.8][qwen_model] on a 
[Spark DGX][dgx_machine]. You can learn more in the engineering docs.

## System requirements

First, and foremost, use Linux. This tool doesn't work on Windows. Other than
that, you need the following tools:

- [Node](https://nodejs.org) - for the commitlint package
- [Rust](https://rust-lang.org/tools/install/) - the main programming language
- [Lefthook](https://lefthook.dev/) - for the pre-commit hooks
- [Oh-my-pi](https://omp.sh/) - the agent used to write the code

If you need help running Qwen 3.8 Flash on a DGX machine, have a look
at [this awesome repository](https://github.com/hasso5703/dgx-spark-qwen38) by
Hassan Basbunar.

## Getting started

Build the release binary and run the installer from a graphical (systemd)
user session:

```bash
cargo build --release -p steno-daemon   # or: scripts/install.sh --build
scripts/install.sh
```

The installer places the daemon at `~/.local/bin/stenod`, enables and starts
the `stenod` systemd **user** service (auto-starts on login, restarts on
failure), and — asking for `sudo` once — installs the udev rule that grants
the `uinput` group access to `/dev/uinput`. If your user was just added to
the `uinput` group, log out and back in before the daemon can inject text.

The service expects CUDA runtime libraries at `~/.local/lib/steno-cuda`. If
yours live elsewhere, override the path with a drop-in instead of editing the
unit:

```bash
systemctl --user edit stenod   # add e.g. Environment=LD_LIBRARY_PATH=/your/path
```

Check on the service with `systemctl --user status stenod` and view logs with
`journalctl --user -u stenod`.

## Documentation

- [Architecture](docs/architecture)
- [Engineering](docs/engineering)
- [Specs](openspec/)

[qwen_model]: https://huggingface.co/Qwen/Qwen3.8-27B
[dgx_machine]: https://build.nvidia.com/spark
