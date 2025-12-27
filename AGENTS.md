# Repository Guidelines

## Project Structure & Module Organization

This is a Rust workspace. Source lives under `crates/` with one crate per module:
`cryxtal-base`, `-geometry`, `-topology`, `-shapeops`, `-bim`, `-io`, `-cli`, and
`-view` (GUI). Examples are in `examples/`. Build artifacts go to `target/`.
Local patched Truck dependencies are under `.tmp_truck/` and are referenced in
the workspace `Cargo.toml`.

## Build, Test, and Development Commands

- `cargo build --workspace`: build all crates in the workspace.
- `cargo test --workspace`: run all tests.
- `cargo run -p cryxtal-cli -- generate box --size 100,200,300 --out out/box.step`:
  CLI example that generates a STEP file.
- `cargo run -p cryxtal-view`: run the egui desktop app.
- `cargo run -p cryxtal-view -- headless generate box --size 100,200,300 --out out/box.step`:
  run the same binary without GUI.
- `cargo run -p cryxtal-view --no-default-features -- headless ...`: build/run without GUI deps.

## Coding Style & Naming Conventions

Use standard Rust style: 4-space indentation, `snake_case` for modules/functions,
`CamelCase` for types/traits, and `SCREAMING_SNAKE_CASE` for constants. Prefer
`rustfmt` for formatting (`cargo fmt`) and keep public APIs documented.

## Testing Guidelines

Tests are run with `cargo test --workspace`. There is no dedicated `tests/`
directory; tests live alongside modules where appropriate. Name tests
descriptively (e.g., `test_export_box_step`).

## Commit & Pull Request Guidelines

The repository has no commit history yet, so follow a simple convention:
short, imperative commit subjects (e.g., "Add headless export"). PRs should
include a clear description, rationale, and how to validate changes. For GUI
changes, include screenshots or short screen captures. Link related issues if
available.

## Configuration Tips

GUI support is controlled by the `gui` feature in `crates/cryxtal-view`.
If you adjust Truck dependency patches, update the `[patch.crates-io]` section
in the workspace `Cargo.toml`.
