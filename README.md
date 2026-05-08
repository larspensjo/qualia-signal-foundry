# Qualia Signal Foundry

**Qualia Signal Foundry** is an experimental platform for exploring simulations of consciousness-like behavior.

The project investigates how a software system can model presence, continuity, memory, perception, reflection, and interaction over time. The goal is not to build a productivity assistant, but to create a research playground for experimenting with artificial agents that feel more continuous, situated, and internally coherent.

This project is currently in an early research and prototyping phase.

## Goals

The project explores ideas such as:

- real-time audio interaction
- short-term and long-term memory
- associative memory
- memory decay and reinforcement
- sleep-like consolidation phases
- tool use as a form of perception
- context-budgeted cognition
- multiple AI model roles
- simulated continuity of identity

Many design questions are still open. The repository should be treated as a working lab, not a finished framework.

## Current Status

This is a work in progress.

Expect:

- incomplete features
- changing architecture
- experimental code
- evolving documentation
- research notes mixed with implementation plans

The early focus is on building enough infrastructure to run small experiments and learn from them.

## Requirements

Recommended development environment:

- Rust
- Git
- Visual Studio Code or another Rust-capable editor
- A terminal such as PowerShell, Windows Terminal, or equivalent

Check the Rust installation with:

```powershell
rustc --version
cargo --version
```

If Rust is not installed, install it from:

```text
https://rustup.rs/
```

## Setup

Clone the repository:

```powershell
git clone https://github.com/<owner>/<repo>.git
cd <repo>
```

Build the project:

```powershell
cargo build
```

Run tests:

```powershell
cargo test
```

Run the project:

```powershell
cargo run
```

At this stage, the exact executable behavior may change frequently as the project evolves.

## Repository Structure

The repository is expected to grow roughly along these lines:

```text
docs/
  project background, research notes, architecture sketches, decisions

src/
  implementation code

tests/
  integration and behavior tests

examples/
  small experiments and prototypes
```

The documentation is part of the experiment. Some documents describe stable background ideas, while others track open questions, working assumptions, design sketches, and research decisions.

## Documentation

Important documentation areas may include:

```text
docs/00-Project-Frame/
docs/10-Concepts/
docs/20-Research-Questions/
docs/30-Architecture/
docs/40-Experiments/
docs/50-Decisions/
docs/60-Checklists/
docs/70-Diary/
```

The documentation should help both a project manager and a researcher understand:

- what the project is trying to explore
- what is already decided
- what is still open
- which experiments should be run next
- why earlier decisions were made

## Design Philosophy

The project should remain open-ended while still being executable.

A useful rule of thumb:

> Capture ideas early, but do not promote them to architecture too quickly.

Research notes, concept documents, experiment logs, and decision records should remain separate so that the project can evolve without prematurely locking down the design.
