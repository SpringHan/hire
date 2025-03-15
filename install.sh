#!/bin/bash

keymap="default.toml"
target="./"

# Get arugments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --keymap)
            keymap="$2"
            shift 2
            ;;
        --target)
            target="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Compile
echo "--- Start to compile project ---"
cargo build --release

# Copy executable file to target path
cp "./target/release/hire" "$target"
echo "--- Copied executable file to target path ---"

# Copy keymap.toml
mkdir -p "$HOME/.config/springhan/hire/"
cp "keymap_config/$keymap" "$HOME/.config/springhan/hire/keymap.toml"
echo "--- Copied keymap config file to config path ---"
