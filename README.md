# Steno

Welcome to Steno, an open-source text-to-speech tool for Linux. With this tool 
you can talk to your computer and it will type in what you said in any 
application you're working with.

## Introduction

### Why this application exists

I made this application because I find that typing gets harder every day for 
me. After doing research I found that there were great tools for Windows and
Mac, but not for Linux. So I decided to roll my own.

### Goals for this application

- Provide a TTS interface for Linux computers that supports a wide range of 
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

## Getting started

TODO: Describe how to install the application.

## Documentation

- [Architecture](docs/architecture)
- [Engineering](docs/engineering)
- [Specs](openspec/)

[qwen_model]: https://huggingface.co/Qwen/Qwen3.8-27B
[dgx_machine]: https://build.nvidia.com/spark