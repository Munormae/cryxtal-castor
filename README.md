# CryXtal Castor

CryXtal Castor is a BIM-oriented kernel in Rust. Geometry is only one layer; every object is a BIM element with identity, parameters, and lifecycle-ready metadata. The MVP focuses on parametric solids, STEP export, and mesh export while keeping the architecture ready for IFC.

## Architecture

Cargo workspace layout:

- `crates/cryxtal-base`: GUID, tolerance, units, common errors
- `crates/cryxtal-geometry`: wrappers over truck-geometry (curves, surfaces, profiles)
- `crates/cryxtal-topology`: B-Rep wrappers and solid builders
- `crates/cryxtal-shapeops`: boolean operations via truck-shapeops
- `crates/cryxtal-bim`: BIM elements, categories, typed parameters, BIM > geometry link
- `crates/cryxtal-io`: STEP export, mesh export, IFC stubs
- `crates/cryxtal-cli`: BIM-oriented CLI
- `crates/cryxtal-view`: egui desktop app (Truck renderer + BIM controls)

## Build

```bash
cargo build --workspace
```

## Tests

```bash
cargo test --workspace
```

## CLI

Generate a box and export to STEP:

```bash
cargo run -p cryxtal-cli -- generate box --size 100,200,300 --out out/box.step
```

Generate a plate with a hole and export to OBJ:

```bash
cargo run -p cryxtal-cli -- generate plate --width 1000 --height 200 --thickness 200 --hole 100 --material C30 --out out/plate.obj
```

Triangulate from STEP (stub):

```bash
cargo run -p cryxtal-cli -- triangulate --in model.step --out mesh.obj
```

## GUI

Run the egui-based desktop app (Truck renderer):

```bash
cargo run -p cryxtal-view
```

Headless mode (same binary, no GUI):

```bash
cargo run -p cryxtal-view -- headless generate box --size 100,200,300 --out out/box.step
cargo run -p cryxtal-view -- headless generate plate --width 1000 --height 200 --thickness 200 --hole 100 --material C30 --out out/plate.obj
```

Build without GUI dependencies:

```bash
cargo run -p cryxtal-view --no-default-features -- headless generate box --size 100,200,300 --out out/box.step
```

## GUI Controls

- View cube: click faces/edges/corners for smooth orientation; faces are labeled by plane (XY/XZ/YZ).
- Snapping: endpoints, edge midpoints, and face centers (square/diamond/triangle markers).
- Layers: bottom-center layer selector with per-layer color; new elements inherit the active layer; layer can be edited in Properties.
- View modes: Ctrl+1 skeleton, Ctrl+2 opaque by layer, Ctrl+3 transparent by layer, Ctrl+4 material (stub).
- Selection handles: selected elements show corner handles.
- Esc: cancel the current tool and return to selection mode.

## Examples

```bash
cargo run -p cryxtal-cli --example export_box_step
cargo run -p cryxtal-cli --example plate_with_hole_to_obj
```

## Notes

- STEP export currently supports solids created directly by `truck-modeling`. Boolean results are best exported via mesh (OBJ).
- STEP import and IFC export are stubbed. See roadmap.

## Roadmap

- v0.1 (MVP): box/plate-with-hole solids, STEP export, OBJ export, BIM element wrapper, CLI
- v0.2: richer parametric objects, parameter constraints, serialization
- v0.3: IFC layer (schema mapping, property sets, relationships)
- v0.4: viewer integration and scene management
