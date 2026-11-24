# Hire

## Introduction

This project is a TUI program written in Rust, which is a file manager.

## Technology Stack

1. Use `Rust` programming language
2. Use `ratatui` crate as TUI framework
3. Only build for `Linux` platform

## Documents

1. `docs/ARCHITECTURE.md`: The architecture doc of the project, written for LLMs.
2. `README.md`: README file written in English.

## Principles

**These are the rules you must follow**:

1. Whenever you modify the files in workspace, update `docs/ARCHITECTURE.md` instantly.
2. The `ARCHITECTURE.md` is written for LLMs, use fewer ASCII-art diagrams and more readable text.
3. Using English to write the comments in code and docs in `docs/` folder.
4. Only add new function when it's necessary, in order to make the code more readable.
5. You can get the git repository info, but **never** execute commands that can change git status without my order.
6. You should add new keybindings into `keymap_config/*.toml` after adding new `AppCommand`.
7. Keep comments concise and explain why the code does something rather than what it does, so they serve as a record of pitfalls and lessons learned.
