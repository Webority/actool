# actool: usage

actool compiles an `.xcassets` catalog or an `.icon` bundle into an `Assets.car` and the icon files Apple's `actool` writes, as an open Rust reimplementation.

## Install

- Build the binary from a checkout of this repository: `cargo build --release --locked`. The program is `target/release/actool`.
- Put that binary on the path, or call it by its full path.
- Compiling on macOS matches Apple's output most closely; off macOS, PDF and SVG image sets are rasterised by pure-Rust stand-ins.

## Set up

None. The binary takes no configuration file; each build passes its flags on the command line.

## Settings

None. The program reads no settings file and no environment variable; every setting is a command-line flag.

## Main calls

- `actool <document> --compile <dir>`: compiles one `.xcassets` catalog or `.icon` bundle into `<dir>`, which receives `Assets.car` and the other outputs.
- `--print-contents`: prints the catalog contents tree and stops; ignored when `--compile` is given.
- `--app-icon <name>`: names the app icon set. A matching `.icns` and a populated partial plist depend on the bundle's file name matching it.
- `--alternate-app-icon <name>`: compiles a named alternate icon set; repeatable.
- `--target-device <device>`: filters image sets by device (`iphone`, `ipad` and the others the catalog names); repeatable.
- `--platform <platform>`: the target platform, `macosx`, `iphoneos` or `iphonesimulator`; default `macosx`.
- `--minimum-deployment-target <version>`: the deployment target both compile paths use; default `11.0`.
- `--output-partial-info-plist <file>`: writes the partial Info.plist for the icon and catalog paths.

## Example

Compile an iOS catalog for iPhone and iPad, with the partial plist written beside it:

```bash
actool --output-partial-info-plist build/partial.plist \
  --app-icon AppIcon \
  --target-device iphone --target-device ipad \
  --minimum-deployment-target 26.0 \
  --platform iphoneos \
  --development-region en \
  --compress-pngs \
  --compile build/assets \
  Assets.xcassets
```

## Do not

- Do not rely on `--warnings`, `--errors`, `--notices`, `--skip-app-store-deployment` or `--enable-on-demand-resources`: they are accepted so Xcode's command line runs, and nothing reads them.
- Do not expect `--compress-pngs` or `--development-region` on an `.icon` bundle: both are catalog-path flags.
- Do not expect the icon atlas coordinates to match Apple's byte for byte; the atlas is packed by this tool's own rule.
- Do not expect SVG text to render off macOS, because no fonts are loaded there.
- Do not run `--compile` into a shared folder such as `/tmp` when another build may use it.
