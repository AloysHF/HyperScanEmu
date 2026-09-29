# iOS Libretro Core

HyperScanEmu runs as a libretro core on RetroArch for iPhone and iPad.

> **Note**: iOS does not support downloading cores through RetroArch's Online
> Updater. The core must be injected into the RetroArch IPA and re-signed. That
> limitation may change in future RetroArch releases.

## Prerequisites

- iPhone or iPad (arm64, iOS 15+)
- A RetroArch IPA (1.17.0 is a known-good baseline for manual injection; newer
  versions change the app folder layout)
- `hyperscan-emu-ios-libretro.tar.gz` from
  [Releases](https://github.com/AloysHF/HyperScanEmu/releases), containing:
  - `hyperscanemu_libretro_ios.dylib` — universal arm64 + x86_64 device core
  - `simulator/hyperscanemu_libretro_ios.dylib` — arm64 simulator core
  - `hyperscanemu_libretro.info` — core metadata
- A file manager and IPA signing tool (ESign, SideStore, AltStore, etc.)

## Install

### 1. Extract the IPA

1. Rename `RetroArch.ipa` to `RetroArch.zip` and extract it
2. Open `Payload/RetroArch.app/`

### 2. Inject the core

Copy `hyperscanemu_libretro_ios.dylib` into `Payload/RetroArch.app/modules/`.

### 3. Inject core metadata

1. Extract `assets.zip` inside `RetroArch.app/`
2. Copy `hyperscanemu_libretro.info` into `assets/info/`
3. Recompress the directory back to `assets.zip` and replace the original

### 4. Install firmware and content

Place in the frontend system directory used by RetroArch on device:

```text
spg290.bin      32768 bytes
hyperscan.bin  1048576 bytes
```

Load a BIN / single-track CUE / ZIP disc package through **Load Content**. See
[Game File Formats](Game-File-Formats.md) and [RetroArch Core](RetroArch-Core.md).

### 5. Re-sign the IPA

The modified IPA **must** be re-signed before install.

**ESign on device:** repackage the modified app tree as an `.ipa`, import it into
ESign, sign with your certificate, then install.

**SideStore / AltStore:** import the modified IPA and use the tool's normal
signing and install flow.

**Developer identity:** use `codesign` / Xcode with a matching provisioning
profile for the modified bundle.

## Build the iOS core locally

On macOS with Xcode command-line tools:

```bash
rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim

cargo build -p hyperscanemu-libretro --release --target aarch64-apple-ios
cargo build -p hyperscanemu-libretro --release --target x86_64-apple-ios
cargo build -p hyperscanemu-libretro --release --target aarch64-apple-ios-sim

lipo -create \
  target/aarch64-apple-ios/release/libhyperscanemu.dylib \
  target/x86_64-apple-ios/release/libhyperscanemu.dylib \
  -output hyperscanemu_libretro_ios.dylib
```

Install the universal dylib as described above. The release workflow packages
device and simulator binaries together.

## Notes

- Retail gameplay is not yet validated on iOS.
- Prefer a personal or developer signing certificate you control; do not
  redistribute signed IPAs that embed copyrighted firmware or games.
- Desktop standalone diagnostics are not available on iOS.
