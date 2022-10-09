# Coral

Coral is a kernel for WebAssembly-based operating systems. It exposes a basic
environment and primitives on top of which higher-level environments can be
built.

Coral share a lot of concepts with other [language-based
systems](https://en.wikipedia.org/wiki/Language-based_system), such as
[singularity](https://en.wikipedia.org/wiki/Singularity_(operating_system)) or
[inferno](https://en.wikipedia.org/wiki/Inferno_(operating_system)). One of the
main technical difference we previous systems is that a lot of languages have
support for WebAssembly, enabling Coral to run legacy applications and programs
written in a wide variety of languages.

Currently, Coral can run Rust programs that do not require system APIs, there is
no OS layer yet as the focus is on the kernel itself.

## Design

Coral is conceptually similar to a micro-kernel, pushing most functionalities
such as drivers in "user-space". Programs are written in WebAssmbly (Wasm) and
interact with the kernel and each other exclusively through imported functions.

On Coral a program is composed of a single Wasm component, which acts as the
scheduling unit. Each component can itself be composed of multiple Wasm
instances, which correspond to the isolation unit. This is to be contrasted with
designs such as Unix, where the program itself (the process) is the isolation
unit, making sandboxing more challenging.

The design is still evolving, and might be subject to change over time.

## FAQ

**Does Coral support WASI?**

Coral does not expose a WASI interface by itself, instead it focuses on exposing
the minimal primitives necessary to build more complex abstractions on top of
them.
Exposing higher level interface, such as WASI, is therefore the responsibility
of the OS running on top of coral.

**Is everything running in ring 0?**

Currently yes, but the plan is to support other isolation mechanisms. Given a
perfect compiler, runtime, and the absence of side-channels, software sandboxing
would be perfectly fine, but no software is perfect and side-channels are here
to stay.

However, I would like to explore a more nuanced and flexible approach to
isolation. Not all software is untrusted: I don't have a problem with software
sandboxed drivers sharing the same address space for efficiency, but I don't
want to run programs downloaded from the internet right next to my password
manager.

The control over compilation opens a lot of options regarding isolation
mechanisms and efficient context switching. For instance, Coral could offer
seameless [SGX](https://en.wikipedia.org/wiki/Software_Guard_Extensions)
integration, and [memory protection
key](https://en.wikipedia.org/wiki/Memory_protection#Protection_keys) become a
viable option for program isolation as switching protection keys would require
escaping the sandbox or using runtime provided functions.

**Why a custom Rust toolchain?**:

The Wasm compiler is currently part of the kernel, and compiled as native code.
However, Cranelift (our compiler backend) does not support `#[no_std]`
environment, and therefore we either need to port Cranelift to `no_std` or to
support the standard library in the kernel. Porting Cranelift to `no_std`
requires forking an patching a lot of dependencies, whereas adding a new target
with basic `std` support (i.e. most OS features unimplemented) is relatively
straightforward and can be self contained in a single diff file.

Having to build a custom toolchain is not ideal thought, and this won't be
required in the future. The easiest way to work around the Cranelift `std`
requirement is to move the compiler into a Wasm program, similar to how
micro-kernels move features into servers.
The kernel is designed so that the presence of a compiler is optional, and the
kernel library is `no_std` itself. The first step toward moving the compiler to
Wasm would be to support _ahead of time compilation_, so that the compiler
itself can be compiled in the first place.

Relevant reading: [How Theseus OS added `no_std` support to
Wasmtime](https://www.theseus-os.com/2022/06/21/wasmtime-complete-no_std-port.html)

**What architecture does Coral support?**

For now Coral only run on `x86_64`. Other architectures could be supported, the
main technical limitations begin the available targets of the Wasm compiler and
the architecture-specific code that needs to be written.

## Relevant Links

- [Singularity: Re-Thinking the Software Stack](https://dl.acm.org/doi/pdf/10.1145/1243418.1243424?casa_token=syq3x5KceIYAAAAA:ZORAwiZGA_WPb3h365ONAI7TWTu9vDwO7qwJWk9y5x7GllkthQEwE1BQ20P_TFNTUSp1yuL6VLQJ5Dg)
- [Nebulet, a previous attempt at a WebAssembly-based OS](https://github.com/nebulet/nebulet)

