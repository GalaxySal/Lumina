# Kip Integration

This directory is reserved for the [Kip](https://github.com/kip-dili/kip) source code.
Kip is an experimental programming language in Turkish where grammatical case and mood are part of the type system.
It serves as the "Semantic Intelligence" engine for Lumina.

## Setup

Since the build environment may not have Haskell/Stack, we use a pre-compiled or mocked binary for the integration.

## Integration

The binary `kip-lang` is registered as a Tauri sidecar.
Lumina interacts with it via stdin/stdout.
