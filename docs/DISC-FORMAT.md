# Disc image format

HyperScanEmu accepts a raw `MODE1/2352` BIN image, a single-track CUE sheet, or a
ZIP containing exactly one such CUE and its referenced BIN. Host media loading
enforces bounded reads, safe relative paths and an uncompressed size limit before
the emulator core receives bytes.

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
