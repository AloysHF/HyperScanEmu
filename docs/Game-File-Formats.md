# Game File Formats

HyperScanEmu accepts disc packages and RFID card images. Firmware (internal ROM
and BIOS) is never bundled; dump it legally from hardware you own.

## Disc images

Supported media:

- a raw `MODE1/2352` BIN image;
- a single-track CUE sheet that references one such BIN;
- a ZIP containing exactly one such CUE and its referenced BIN.

Host media loading enforces bounded reads, safe relative paths and an
uncompressed size limit before the emulator core receives bytes.

The core validates every sector before exposing user data:

- the 12-byte CD-ROM sync pattern;
- the absolute BCD minute/second/frame address, including the 150-frame lead-in;
- mode byte `1`;
- exact 2352-byte sector boundaries;
- an ISO 9660 primary descriptor at LBA 16 and a descriptor terminator;
- an `NSR02` or `NSR03` UDF volume-recognition descriptor.

Successful validation exposes both the original 2352-byte sector and exactly
2048 user bytes per LBA. The CD servo consumes raw sectors for ring-buffer DMA,
scheduled at the selected drive speed. EDC/ECC checking and multi-track CUE
support are tracked separately; callers must not treat a renamed ISO or arbitrary
binary file as supported media.

Validate media without firmware:

```bash
cargo run -p hyperscanemu -- inspect-disc <media.bin|media.cue|media.zip>
```

## Card images

HyperScan card images are exactly 120 bytes. The core keeps the image in memory,
marks it dirty after an effective write, and returns the modified image when it
is ejected. Frontends are responsible for saving dirty images atomically.

Known data regions:

| Offset | Length | Purpose |
|---:|---:|---|
| `0` | 7 | RFID UID |
| `8` | 2 | Game identifier |
| `10` | 2 | Card identifier |
| `12` | 92 | Game save data observed on existing cards |

The wire protocol is firmware-driven through GPIO. Commands begin with a
seven-bit opcode and may continue to nine bytes. Responses contain a two-byte
preamble, least-significant-bit-first bytes, odd parity bits and, where required,
a two-byte complemented CRC.

Supported commands are REQA (`0x26`), WUPA (`0x52`), RID (`0x78`), READ (`0x01`),
RALL (`0x00`), WRITE-E (`0x53`) and WRITE-NE (`0x1a`). Addressing wraps across the
120-byte image, matching observed firmware-visible behavior.

## Firmware

| File | Size | Role |
|---|---:|---|
| `spg290.bin` | 32768 | SPG290 internal ROM |
| `hyperscan.bin` | 1048576 | HyperScan BIOS |

Standalone commands take both firmware paths explicitly. The libretro core loads
them from the frontend system directory using the filenames above.
