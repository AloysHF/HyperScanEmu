# Release Process

How to publish a HyperScanEmu release.

## Prerequisites

- Push access to the default branch
- Permission to create tags and GitHub Releases

## Steps

### 1. Update version numbers

Keep these three values identical:

| File | Field | Example |
|---|---|---|
| `Cargo.toml` (workspace root) | `[workspace.package] version` | `"0.1.0"` |
| `crates/hyperscanemu-libretro/hyperscanemu_libretro.info` | `display_version` | `"0.1.0"` |
| `crates/hyperscanemu-libretro/src/lib.rs` | `library_version` | `c"0.1.0"` |

The `.info` file is copied into release artifacts; RetroArch shows
`display_version` as the core version.

### 2. Preflight

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Confirm README/docs command lines still match the CLI, and that
[Game Compatibility](Game-Compatibility.md) claims were not overstated for the
tag.

### 3. Commit and tag

```bash
git add Cargo.toml Cargo.lock \
  crates/hyperscanemu-libretro/hyperscanemu_libretro.info \
  crates/hyperscanemu-libretro/src/lib.rs
git commit -m "chore: bump version to 0.2.0"
git push origin master

git tag v0.2.0
git push origin v0.2.0
```

Releases trigger on tags matching `v*` (`.github/workflows/release.yml`).

### 4. CI builds and drafts the release

The workflow:

1. Builds standalone binaries for Linux x86_64/aarch64, macOS x86_64/aarch64,
   and Windows x86_64
2. Builds libretro cores for those targets plus 32-bit Linux/Windows
   (core-only), renaming the cdylib to `hyperscanemu_libretro.<ext>` and bundling
   `hyperscanemu_libretro.info`
3. Builds Android (`arm64-v8a`, `armeabi-v7a`, `x86`, `x86_64`), iOS (universal
   device dylib + arm64 simulator), and webOS (armv7) libretro cores
4. Creates a **draft** GitHub Release with generated notes and a download table

### 5. Review and publish

1. Open the draft under Releases
2. Edit the changelog if needed
3. Verify artifacts, including:
   - `hyperscan-emu-linux-*.tar.gz`, `hyperscan-emu-macos-*.tar.gz`,
     `hyperscan-emu-windows-*.zip`
   - `*-libretro.*` desktop packages
   - `hyperscan-emu-android-libretro.tar.gz`
   - `hyperscan-emu-ios-libretro.tar.gz`
   - `hyperscan-emu-webos-libretro.tar.gz`
4. Publish the release

Never attach firmware, game media, or card dumps.

### 6. Sync `.info` metadata upstream

RetroArch's Online Updater reads
`dist/info/hyperscanemu_libretro.info` from
[libretro/libretro-super](https://github.com/libretro/libretro-super). If
metadata changed, open a PR there with the updated file and reference this tag.

## Troubleshooting

### CI fails

Check the Actions run. Linux jobs need `libasound2-dev`, `libx11-dev`, and
`libxkbcommon-dev` (installed by the workflow). 32-bit Linux also needs
`gcc-multilib`. Android jobs need the NDK (`r27c`) and `cargo-ndk`.

### Re-trigger a release

```bash
git tag -d v0.2.0
git push origin --delete v0.2.0
# delete the stale draft release, then:
git push origin v0.2.0
```

### Missing artifacts

The workflow only runs on `v*` tag pushes. Manually dispatching the workflow
will not create a release. Tag names must start with `v`.

### Version mismatch in RetroArch

`Cargo.toml`, `hyperscanemu_libretro.info`, and `lib.rs` must share the same
version string.
