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
   <system>/hyperscanemu/spg290.bin       32768 bytes
   <system>/hyperscanemu/hyperscan.bin  1048576 bytes
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

## RetroDroid

The same core ships inside [RetroDroid](https://github.com/AloysHF/RetroDroid),
the all-in-one Android app for this emulator family. RetroDroid loads
`libhyperscanemu.so` directly (no RetroArch required) and reads the neighbouring
`android.json` for the system profile: display names, content extensions,
virtual-controller preset, firmware requirements and orientation.

Rules the core follows for any Android host:

- Firmware (`hyperscanemu/spg290.bin`, `hyperscanemu/hyperscan.bin`) is read
  from the directory answered by `RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY`; the
  content path itself may be a disposable copy in the app cache. RetroDroid
  answers that call with one shared private system root and copies the files
  from the user-visible `BIOS/hyperscan/` folder into
  `<root>/hyperscanemu/`, so the same relative layout serves every frontend.
- Load failures are reported with `RETRO_ENVIRONMENT_SET_MESSAGE` and the log,
  never as a silent `false`.
- Panics are caught at the C ABI boundary, so a bad frame cannot kill the host.
- Save states carry an `HSEM` magic header plus a format version; states from
  another version are rejected instead of being misread.
- Cores are linked with 16 KB page alignment for Android 15 devices.
- The core writes no save files of its own (there is no battery-backed storage
  in this system); all state lives in save states.

## Notes

- Retail gameplay is not yet validated on Android.
- Card image selection and atomic card saving remain frontend responsibilities.
- Headless diagnostics (`--inspect`, `--trace`, headless frame runs) are desktop
  standalone features and are not part of the Android core.
