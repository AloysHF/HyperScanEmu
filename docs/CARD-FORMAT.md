# HyperScan card format

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
