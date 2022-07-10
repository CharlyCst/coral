# See https://github.com/casey/just
# TLDR: think makefile, but use `just` instead of `make`

alias h := help
alias t := test
alias r := run

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
    cd ./kernel && cargo test

# Run Coral
run:
    cd ./kernel && cargo run
