# See https://github.com/casey/just
# TLDR: think makefile, but use `just` instead of `make`

# Print a list of recipies
help:
    @just --list --list-heading $'Coral recipies:\n'

# Run tests for all crates
test:
    # Compiler tests
    cd ./crates/compiler && cargo test
    # Wasm tests -- for now only checking
    cd ./crates/wasm && cargo check
    # Coral tests
    cd ./kernel && cargo test --profile kernel

# Run Coral
run:
    cd ./kernel && cargo run --profile kernel

# Build and install userland
userland:
    # Build userboot
    cd ./userland/userboot && cargo build --profile userland
    cargo run -p coral-bindgen -- \
        -o kernel/wasm/userboot.wasm \
        target/wasm32-unknown-unknown/userland/userboot.wasm \
        userland/userboot/bindgen.toml

