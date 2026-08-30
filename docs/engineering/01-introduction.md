# Introduction

The engineering workflow for this project uses agents in its core. I try to
automate as much as possible, because I can't physically control every aspect
of the code with my keyboard anymore due to chronic condition.

## Core principles

- **Prove that it works with tests:** The software only works when it runs and 
  can be tested automatically.

- **Keep things pragmatic:** I love great software architecture and puzzling
  but this project is here to solve problems. If the code is not ideal, I don't
  want to be stopped by it.

- **Keep things secure:** This application is capable of entering input nearly
  everywhere in my system. It's therefore important that the thing I say is the
  thing that is entered in the application and nothing else.

## Supported agents

This project is entirely optimized for working with [Qwen 3.8][QWEN38] and 
[Oh-my-pi][OMP]. You can try to use other agents, but I can't guarantee that it
will work as intended as I've built the harness to fit this combination.

[OMP]: https://omp.sh/
[QWEN38]: https://huggingface.co/Qwen/Qwen3.8-Flash-Next
