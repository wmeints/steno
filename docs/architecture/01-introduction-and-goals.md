# Introduction and goals

This directory contains the architecture documentation for Steno, a 
dictation solution for Linux based on local models. Users can press a 
hotkey to record audio that is then transcribed and submitted to the input 
device so that the text is submitted to the currently active terminal or 
application window.

## Functional goals

- Provide a text-to-speech solution on Linux that works on most widely used 
  desktop environments and the terminal.

- Provide support for Dutch, and English as initial languages for the 
  application.

## Quality goals

- Only the spoken text recorded by the application is transcribed and sent to 
  the input device on the Linux machine to prevent injection attacks.

- Only local models are used to ensure the privacy of the user. 

- Transcribed audio is only accessible for the currently logged in user and 
  stored in a dedicated user directory.
