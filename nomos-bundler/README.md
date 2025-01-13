# Nomos Bundler

This crate performs the bundling tasks for other Nomos' crates.

## Usage

Any crate that needs to be bundled should have their own file in the `src` directory of this crate,
with the same name as the bundled crate, and implement their bundling logic.

Each of those files should be mapped to a standalone binary in the `Cargo.toml` file so they can be called
independently.

## Improvements

At a later point, when `Rust` releases the ability to run rust files as scripts (with dependencies specifies inline),
this crate could be turned into simple scripts that can live in their respective crates.
