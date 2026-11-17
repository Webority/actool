# Changelog

## [Unreleased]

## [2.2.4-webority.7] - 17-Nov-2026

### Fixed

- A tree past one leaf (more than 510 renditions, or more than 126 bitmap keys) gets the root node Apple's `assetutil` reads; the old root crashed it, and App Store processing hung on every upload carrying such a catalog. A tree too large for one root is now an error instead of a malformed file.
- SVG vector renditions are keyed at scale 0, as Xcode's actool keys them, so they no longer collide with the 1x raster.
- App icon sets drop Apple Watch and Mac sizes when compiling for iOS, as Xcode does.

## [2.2.4-webority.6] - 08-Nov-2026

### Added

- Dark and tinted iOS app icon variants compile as Xcode 26.5's actool compiles them.
- `--target-device` filters image sets by device, as Xcode does.
- PDF and SVG image sets are rasterised off macOS, at the platform's scales.
- `--print-contents` prints the catalog contents tree, as Xcode 26.5's actool does.

### Fixed

- The print-contents code and its test satisfy the workspace lints.

### Tooling

- Xcode 26.5 references recorded for icon variants, print-contents, device families and vector rasters, with an iPhone and iPad split catalog.
- Raster parity test file marked as test code for the expect lint.

## [2.2.4-webority.5] - 08-Nov-2026

### Changed

- The crate adopts the shared workspace lint table.

### Fixed

- Device app icons are written as CgBI PNGs, as Xcode writes them.

## [2.2.4-webority.4] - 08-Nov-2026

### Fixed

- Grey renditions state the row stride they are really stored with.

### Tooling

- The crate moves to Rust edition 2024.
- Nested ifs are collapsed into let-chains.

## [2.2.4-webority.3] - 05-Nov-2026

### Changed

- The icns lengths and the symbols plist are written by the crate itself, so the byteorder and plist crates are gone.

### Fixed

- An iOS single-size app icon compiles as Apple's actool compiles it.
- Non-macOS builds stay warning-free.
- The lint gaps on macOS paths are closed.

### Tooling

- The ratchet runs on push, with its suppression baseline seeded; pushes are gated on fmt, clippy and tests.
- Two unused crates are removed.
- rustfmt and clippy's automatic fixes are applied.
- Every unwrap and expect outside tests is removed.
- The repo facts moved to the shared layout.
- A single-size iOS icon catalog and a script that records Apple's actool output for it are added as tests.

## [2.2.4-webority.2] - 03-Sep-2026

### Added

- Alternate app icon sets compile and are listed as alternates.

## [2.2.4-webority.1] - 03-Sep-2026

### Added

- The fork starts from upstream `viraptor/actool` 2.2.4 (03-Jun-2026). Earlier upstream releases, 1.0.0 to 2.2.4, are in the upstream repository.

### Security

- quick-xml moves to 0.41 to clear two advisories. The crate does not call the affected code.
