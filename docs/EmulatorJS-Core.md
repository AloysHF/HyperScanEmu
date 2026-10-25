# EmulatorJS Core (Browser)

HyperScanEmu can run in the browser as a standard
[EmulatorJS](https://emulatorjs.org) core. The build follows EmulatorJS's own
pipeline: the Rust libretro core is compiled for `wasm32-unknown-emscripten`,
linked against the `EmulatorJS/RetroArch` fork, and packaged as the
`<core>-wasm.data` bundles that an EmulatorJS page loads at runtime.

The project site ships a ready-to-use player at
<https://AloysHF.github.io/HyperScanEmu/play.html>: visitors pick firmware and a
disc image from their own machine, and both run from blob URLs, so nothing is
uploaded and the site does not host any firmware or game data.

## What you get

Every nightly publishes a single asset:

| File | Contents |
|------|----------|
| `hyperscan-emu-emulatorjs.zip` | Both core variants, the build report `cores/reports/hyperscanemu.json`, `core.json`, `build.json`, `license.txt` and a README |

The two `.data` files stay separate inside the ZIP: EmulatorJS requests
`<core>-wasm.data` or `<core>-legacy-wasm.data` by name depending on WebGL2
support, so one bundle cannot serve both cases.

The bundle is laid out like this:

```
hyperscan-emu-emulatorjs.zip
├── cores/
│   ├── hyperscanemu-wasm.data
│   ├── hyperscanemu-legacy-wasm.data
│   └── reports/hyperscanemu.json
├── core.json
├── build.json
├── license.txt
└── README.md
```

Each `.data` file is a 7z archive containing `<core>_libretro.js`,
`<core>_libretro.wasm`, `core.json`, `build.json` and `license.txt`.

## Install into a site

EmulatorJS always fetches cores from `<EJS_pathtodata>cores/`, so host the
runtime yourself (or use the pinned CDN files) and drop the core beside it:

```
data/
├── loader.js
├── emulator.min.js
├── emulator.min.css
├── compression/
│   ├── extract7z.js       # extracts the core .data archive
│   └── extractzip.js      # extracts ZIP content and firmware
├── localization/          # optional, e.g. zh-CN.json
└── cores/
    ├── hyperscanemu-wasm.data
    ├── hyperscanemu-legacy-wasm.data
    └── reports/hyperscanemu.json
```

`emulator.min.zip` only contains `emulator.min.js` and `emulator.min.css`;
the `compression/` and `localization/` files are loaded on demand from the
same EmulatorJS data directory. Without `extract7z.js` the core download ends
in a "Network Error".

To upgrade the runtime, replace the runtime files with a newer EmulatorJS
release (≥ 4.2.3) and keep this `cores/` directory.

## Use

```html
<div id="game"></div>
<script>
  EJS_player = "#game";
  EJS_core = "hyperscanemu";
  EJS_gameUrl = URL.createObjectURL(discFile);
  EJS_gameName = discFile.name;
  EJS_biosUrl = URL.createObjectURL(firmwareZip);
  EJS_pathtodata = "data/";
  EJS_startOnLoaded = false;
</script>
<script src="data/loader.js"></script>
```

`site/play.html` in this repository is a complete example.

When the game comes from a blob URL, EmulatorJS writes it into the wasm
filesystem under `EJS_gameName`. Set that to the original file name, extension
included (for example `game.bin` or `game.zip`), otherwise the frontend cannot
infer the content type and the core receives an unknown path.

### Firmware

The HyperScan requires two legally dumped firmware images:

| File | Size |
|------|------|
| `spg290.bin` | 32768 bytes (SPG290 internal ROM) |
| `hyperscan.bin` | 1048576 bytes (HyperScan BIOS) |

EmulatorJS cannot install files into a RetroArch system directory. Instead it
decompresses the `EJS_biosUrl` archive and writes each entry next to the content
file in the wasm filesystem. Package both images in a ZIP (any folder layout;
EmulatorJS flattens entry names) and pass its blob URL as `EJS_biosUrl`. The
core looks in the frontend system directory first and, on Emscripten, falls
back to the content's directory, so the same core works in RetroArch and in the
browser.

Firmware is copyrighted and is neither distributed with this repository nor
required by the site.

### Content

The core loads raw `MODE1/2352` BIN images, single-track CUE/BIN packages, and
ZIP archives of those. `core.json` advertises `cue` and `bin`, which makes
EmulatorJS extract a selected ZIP and prefer its `.cue` entry; when a ZIP
contains no cue sheet it boots the first BIN. A lone `.bin` works without a
sheet because these discs use a single track starting at sector 0.

CUE + BIN pairs must be packed into a ZIP for the browser: EmulatorJS hands the
core one file, so a cue sheet cannot resolve a sibling BIN that was never
loaded. See [Game-File-Formats.md](Game-File-Formats.md) for the container
layouts and [Game-Compatibility.md](Game-Compatibility.md) for tested titles.

Game data is copyrighted and is neither distributed with this repository nor
required by the site: the play page only loads files the visitor selects.

## Rendering and performance

The core renders through libretro's software framebuffer in `XRGB8888` and
presents at up to 640×480. Both variants are still built because EmulatorJS picks
the file name by WebGL2 support.

The **CPU Execution Engine** option is not registered in the browser build.

## Controls

EmulatorJS maps the RetroPad to its default keyboard and gamepad bindings:

| Input | HyperScan Button |
|-------|------------------|
| Arrow keys | Analog stick |
| X | Green |
| Z | Red |
| S | Blue |
| A | Yellow |
| Q / W | Left / right shoulder |
| Enter | START |
| Right Shift | SELECT |

Keyboards and touch controls only produce RetroPad directions, so the core folds
a held direction into the analog axes when no analog stick is active. Physical
gamepads use their real analog sticks. Touch controls and gamepads are provided
by EmulatorJS.

## Saves

The core does not implement save-state serialization or expose battery RAM, so
EmulatorJS's save buttons are turned off by `"save": false` in `core.json`.

## Building the bundle

The nightly workflow runs this automatically. To build locally you need an
activated [emsdk](https://emscripten.org/docs/getting_started/downloads.html)
(`emmake` in `PATH`), `p7zip`, `zip`, and the pinned nightly toolchain:

```bash
# Rust's prebuilt emscripten stdlib carries wasm exception handling, which
# Binaryen's Asyncify pass rejects. Rebuild std with panic=abort instead.
cargo +nightly build -p hyperscanemu-libretro \
  --target wasm32-unknown-emscripten --profile wasm-release \
  -Zbuild-std=std,panic_abort

scripts/build-emulatorjs-core.sh
```

The script clones the pinned `EmulatorJS/RetroArch` revision, links the
staticlib with `Makefile.emulatorjs`, builds the WebGL2 and legacy variants, and
writes `out/hyperscan-emu-emulatorjs.zip`. `--work-dir` moves the RetroArch
checkout out of `target/`, which matters on CI because `rust-cache` restores
`target/` without the checkout's `.git`.

The Pages deployment fetches the pinned EmulatorJS runtime, downloads the
`hyperscan-emu-emulatorjs.zip` asset from the rolling `nightly-build` release,
and redeploys after every successful Nightly Build.
