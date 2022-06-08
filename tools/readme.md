# Tools

A set of tools used by the Coral project.

## Rust Coral toolchain

Coral uses a patched Rust toolchain that provides a new
`x86_64-coral-unknown-kernel` target.
This section describes how to install the coral toolchain.

**1] Clone Rust**:

Clone the Rust repository in the `tools` folder:

```sh
git clone https://github.com/rust-lang/rust
```

**2] Apply the patch**:

```sh
cd rust
git checkout 1.60.0
git apply ../rustc-coral.path
```

**3] Create the configuration**

Copy the configuration template:

```sh
cp ../rustc-coral.config.tom config.toml

# Use your favorite editor
vim config.toml
```

The values to complete are indicated by double curly brackets (`{{like-that}}`).
Here is the significations of the various values:

```
{{host-architecture}}: The host target triple.
{{absolute-build-path}}: an absolute path to a build folder.
```

**4] Build & install the toolchain**

Start by building the compiler using the `x.py` tool of the Rust project. This
may take a while (e.g. a few hours!).

```sh
python3 x.py install --stage 1
```

Then install the toolchain locally, using Rustup, by replacing the values in the
following command by those used in `config.toml`:

```sh
rustup toolchain link coral {{absolute-build-path}}/{{host-architecture}}/stage1/
```

It is then possible to check if the compiler was correctly installed with:

```sh
# Print something similar to "rustc 1.xx.xx"
rustc +coral --version
```

