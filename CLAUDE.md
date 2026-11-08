# actool

Open Rust fork of Apple's `actool` (upstream `viraptor/actool`): compiles `.xcassets` and `.icon` bundles to an `Assets.car` identical to the system tool's output. xOS Tools' `xos-assets` compiles through it; bound to cloud project 156 (xOS Tools).

@~/.claude/conventions/pm.md
@~/.claude/conventions/mcp-usage.md
@~/.claude/conventions/shared-packages.md

## Stack & layout

- Rust crate `actool` (`Cargo.toml`). `src/lib.rs` is the library, `src/main.rs` the CLI, whose flags match `/usr/bin/actool`.
- `src/bom.rs` BOM container; `car.rs` CAR header and keys; `catalog.rs` xcassets and app icons; `compiler.rs` legacy xcassets compile; `packer.rs` rendition packing.
- `src/icon_bundle.rs`, `icon_json.rs`, `icon_render.rs`, `icon_effects.rs` for `.icon` (IconComposer) bundles; `symbols.rs`, `icns.rs`, `deepmap2.rs`, `svg_raster.rs`, `pdf_raster.rs`, `cgbi.rs`, `name_hash.rs` for the remaining outputs and formats.
- `tools/` holds the comparison and probe scripts and `validate_car`; `tests/` holds the integration tests, one file per area; `tests/ref_samples/` holds Apple's reference output.

## Build, run, verify

- Cheapest proof: `cargo test --locked -q <name>`; full: `cargo test --locked -q`.
- Before push, the repo `verify` from `.webority/settings.json`: `cargo fmt --all --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked -q`.
- Parity signal: `tools/validate_car <path>` runs CoreUI's `imagesWithName:` and `colorWithName:`; match Apple's OK count (9 OK on the element-web fixture), not the SKIP count.
- Structural diff: `python3 tools/compare_car.py <a> <b>` audits the BOM layout (inline keys, BITMAPKEYS, tree headers, named-block order).
- Third-party repos: `tools/validate_repos.py --only <slug>`.
- Reference: `/usr/bin/actool` on a Mac. Compile ours and Apple's into separate local subdirectories at the same `--minimum-deployment-target`, then diff. Never use shared `/tmp`.

## Tests

- Keep every parity and regression test (`tests/*_parity.rs`, `*_regressions.rs`, `e2e_compile.rs`, `icon_bundle_*.rs`): each pins Xcode-matching output.
- Add a new test only for Xcode-matching output; any other change ships without one.

## Rules

- Keep the BOM inline-key region: fixed-key trees (RENDITIONS, APPEARANCEKEYS) pack rendition keys after the entry table, and a zero-filled or misplaced region makes every lookup return empty without an error (`bom.rs` `build_leaf_node`).
- CARHEADER coreUI version is 975 for `.icon`; lower values take the legacy path and lookups fail (`car::make_carheader_versioned`). Other catalogs use 972.
- BITMAPKEYS lists every facet with a 52-byte attribute mask; an empty tree breaks every facet lookup (`icon_bundle::build_bitmapkeys`).
- Color and Gradient rendition keys use `scale=1` though `scale_factor` is 0, because `colorWithName:displayGamut:` filters on the key.
- Pre-rendered icon PNGs use `template_rendering_intent: 0`; the default -1 makes CUICatalog look for a template variant.
- `.icon` layer assets use `force_non_opaque: true`, so layers always composite with alpha whatever their source alpha.
- Every `.icon` group needs its own IconGroup rendition, and the iconstack lists all groups back to front; wiring only `groups[0]` drops later groups' facets (`icon_bundle.rs`, `layered` block).
- Frosted-glass colour is kept only when the group's shadow is `layer-color`; `none` and `neutral` strip it. The exact tint strength is renderer-bound.
- Emit the same `.car` at every deployment target from 11.0 to 26.0; only the `deploymentPlatformVersion` string in EXTENDED_METADATA changes.
- Write `<app-icon>.icns` and a populated partial plist only when the bundle's filename stem matches `--app-icon` case-sensitively; a mismatch gives only `Assets.car` and an empty plist.
- Named blocks keep Apple's order (CARHEADER, RENDITIONS, FACETKEYS, APPEARANCEKEYS, KEYFORMAT, EXTENDED_METADATA, BITMAPKEYS); the order does not load-block but keeps the diff small.
- iPhoneOS: rendition keys carry Idiom (attr 15) and Subtype (attr 16); the imageset key format is fixed `[7,13,12,15,16,17,1,2]`; the EXTENDED_METADATA platform is `ios`.
- Idiom values: universal=0, iphone=1, ipad=2, tv=3, car=4, watch=5, ios-marketing=6.
- iOS app-icon key format is `[7,13,12,15,16,9,8,17,1,2]`; dim2 and dim1 map sizes through `catalog::ios_icon_dim2` and `icon_size_index`.
- A 60pt@3x iPhone icon also yields a 90pt@2x subtype-1792 rendition cloned from it, forced inline (`catalog::parse_appiconset`).
- Icon atlas geometry is deliberately unmatched: CUICatalog reads the exact INLK coordinates, and Apple's bin-packer was not reverse-engineered.

## Where facts live

- `docs/icon-bundle-parity.md`: `.icon` parity status, what byte-matches and what is renderer-bound.
- `docs/atlas-packing.md`: the atlas packing rules and the dead ends.
- `docs/icon-shading.md`, `docs/icon-shading-plan.md`: icon shading and glass research.
- xos-tools `docs/xcode-deviations.md`, the actool row: where xOS Tools deviates from Xcode through this tool.
- Decision docs #0365 and #0366 (cloud Documents): the choice of this Rust compiler over the alternatives.
