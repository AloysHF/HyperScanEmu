# Android Libretro Core

The HyperScanEmu libretro core runs on Android and works with most RetroArch
based frontends.

## Install in RetroArch on Android

### Via Online Updater (when available)

1. Open RetroArch
2. **Main Menu → Online Updater → Core Downloader**
3. Select **HyperScan (HyperScanEmu)**
4. Load the core from **Main Menu → Load Core**

Update later with **Online Updater → Update Installed Cores**.

### Manual installation

1. Download `hyperscan-emu-android-libretro.tar.gz` from the
   [Releases](https://github.com/AloysHF/HyperScanEmu/releases) page. It contains
   per-ABI `hyperscanemu_libretro_android.so` files for `arm64-v8a`,
   `armeabi-v7a`, `x86`, and `x86_64`.
2. Copy the `.so` that matches your device ABI (usually `arm64-v8a`) into
   RetroArch's `cores/` directory (often
   `/storage/emulated/0/RetroArch/cores/`).
3. Copy `hyperscanemu_libretro.info` into RetroArch's `info/` directory.
4. Place firmware in the **system** directory (see
   [RetroArch Core](RetroArch-Core.md)):
   ```text
   spg290.bin      32768 bytes
   hyperscan.bin  1048576 bytes
   ```
5. Load the core, then **Load Content** with a BIN, single-track CUE, or ZIP
   disc package.

## Build the Android core locally

Requires the [Android NDK](https://developer.android.com/ndk) and
[`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk):

```bash
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
export ANDROID_NDK_HOME=/path/to/android-ndk

cargo ndk -t arm64-v8a -t armeabi-v7a -t x86 -t x86_64 --platform 21 \
  build -p hyperscanemu-libretro --release
```

Each ABI produces `libhyperscanemu.so`. Rename it to
`hyperscanemu_libretro_android.so` when installing. The release workflow packages
these automatically.

## Notes

- Retail gameplay is not yet validated on Android.
- Card image selection and atomic card saving remain frontend responsibilities.
- Headless diagnostics (`inspect-disc`, `trace`, `run`) are desktop standalone
  features and are not part of the Android core.
