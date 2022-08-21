# See https://github.com/casey/just
# TLDR: think makefile, but use `just` instead of `make`

# Print a list of recipies
help:
    @just --list --list-heading $'Coral recipies:\n'

# Run tests for all crates
test:
    # Compiler tests
    cd ./crates/compiler && cargo test
    # Wasm tests
    cd ./crates/wasm && cargo test
    # Coral tests
    cd ./kernel && cargo test --profile kernel

# Run Coral
run:
    cd ./kernel && cargo run --profile kernel

# Build and install userland
userland:
    # Build userboot
    cd ./userland/userboot && cargo build --profile userland
    cargo run --bin cold -- \
        target/wasm32-unknown-unknown/userland/userboot.wasm \
        coral userland/userboot/wasm/syscalls.wasm \
        -o kernel/wasm/userboot.wasm

