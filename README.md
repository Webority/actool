# actool

actool compiles `.xcassets` catalogs and `.icon` bundles into the `Assets.car` and icon files that Apple's `actool` produces, as an open Rust reimplementation.

## Fork

This is Webority's fork of `viraptor/actool` at upstream 2.2.4. It adds:

- Alternate app icon sets, compiled and listed as alternates.
- Xcode's single-size iOS app icon, compiled as Xcode's actool compiles it.
- The `plist` and `byteorder` crates removed: the icns lengths and the symbols plist are written by the crate itself, and quick-xml is at 0.41.
- Device app icons written as CgBI PNGs, as Xcode writes them.
- `--print-contents`, printing the catalog contents tree as Xcode 26.5's actool does.
- PDF and SVG image sets rasterised off macOS, with resvg and hayro, at 1x and 2x on macOS and 1x to 3x on iOS.
- `--target-device` honoured for image sets.
- The shared workspace lint table.

xOS Tools consumes this fork by release tag, `2.2.4-webority.<n>`.

## Usage

On macOS, with the `actool` binary on the path:

```
actool --compile <out-dir> --platform macosx --minimum-deployment-target <version> <Assets.xcassets>
```

For an iOS app icon in Xcode's single-size form:

```
actool --output-format human-readable-text --notices --warnings --output-partial-info-plist <partial.plist> --app-icon AppIcon --compress-pngs --enable-on-demand-resources YES --development-region en --target-device iphone --target-device ipad --minimum-deployment-target 26.0 --platform iphoneos --compile <out-dir> <AppIcon.xcassets>
```

The document is the one positional argument. `Assets.car` and the other outputs land in the `--compile` directory.

## Flags

Every flag the CLI parses, in the order it is declared in `src/main.rs`. "Accepted and ignored" flags are parsed so Xcode's command line runs unchanged, and nothing reads them.

| Flag | Status | Note |
|---|---|---|
| `--output-format` | Honoured | `human-readable-text` prints text; any other value prints XML, including `binary1`. |
| `--compile` | Honoured | Output directory. The run fails without it, except with `--print-contents` or `--version`. |
| `--warnings` | Accepted and ignored | No warnings are printed. |
| `--errors` | Accepted and ignored | Errors are printed regardless of this flag. |
| `--notices` | Accepted and ignored | No notices are printed. |
| `--output-partial-info-plist` | Honoured | Writes the partial Info.plist for the icon and catalog paths. |
| `--app-icon` | Honoured | Names the app icon set; a matching `.icns` and a populated partial plist depend on its stem. |
| `--include-all-app-icons` | Honoured | Compiles every app icon set in the catalog. |
| `--alternate-app-icon` | Honoured | Compiles the named alternate icon set; repeatable. |
| `--accent-color` | Honoured | Sets the accent colour for `.icon` bundles and catalogs. |
| `--widget-background-color` | Honoured | Catalog path only; `.icon` bundles ignore it. |
| `--platform` | Honoured | Target platform: `macosx`, `iphoneos` or `iphonesimulator`. |
| `--minimum-deployment-target` | Honoured | Deployment target for both compile paths. |
| `--target-device` | Honoured | Filters image sets by device; app icons are not filtered, as Xcode does not filter them either. |
| `--standalone-icon-behavior` | Honoured | `default`, `all` or `none`, for both compile paths. |
| `--compress-pngs` | Honoured | Catalog path only. |
| `--skip-app-store-deployment` | Accepted and ignored | Nothing reads it. |
| `--enable-on-demand-resources` | Accepted and ignored | Nothing reads it. |
| `--bundle-identifier` | Honoured | Used only with `--generate-objc-asset-symbols`; ignored otherwise. |
| `--development-region` | Honoured | Catalog path only. |
| `--include-language` | Honoured | Catalog path only; repeatable, limits the languages included. |
| `--include-partial-info-plist-localizations` | Honoured | Catalog path only; `NO` leaves localisations out of the partial plist. |
| `--export-dependency-info` | Honoured | Writes the dependency file a build system reads, for both compile paths. |
| `--generate-objc-asset-symbols` | Honoured | Writes the Objective-C symbols header and stops, when `--bundle-identifier` is also given; otherwise ignored. |
| `--generate-asset-symbol-index` | Honoured | Writes the symbol index with `--generate-objc-asset-symbols`. |
| `--print-contents` | Honoured | Prints the catalog contents tree and stops; ignored when `--compile` is given. |
| `--version` | Honoured | Prints the version plist and exits. |

## Known gaps

- Atlas geometry differs from Apple's: the icon atlas is packed by our own rule, so its coordinates do not match the ones Apple's packer writes.
- deepmap2 compression is macOS only, because it needs Apple's private APIs.
- PDF normalisation is macOS only.
- Vector rasters are checked for names, scales and sizes but not for pixels yet (TASK-28486).
- SVG text renders nothing off macOS, because no fonts are loaded.
- The palette pixel format is not supported.

## Licence

Upstream declares MIT in Cargo.toml; the licence text file is pending.

## Origin

### But why?

Apple's actool is a proprietary, closed utility. That makes it hard to both cross compile from a different system and to build in a tightly controlled environment like Nixpkgs.

Also it's fun.

### Implementation

The system has been implemented almost entirely automatically by Claude with
deciduous planning.

We're engaging only in cleanroom implementation based on known outputs of the
original tool, with abundance of caution for potential legal issues.
