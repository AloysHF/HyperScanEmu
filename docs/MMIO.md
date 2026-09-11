# SPG290 MMIO status

The bus accepts only explicitly modeled 32-bit MMIO registers. Unknown addresses
and unsupported access widths stop execution with the access type and address.

Implemented today:

- PPU register/RAM windows at `0x0801_0000`, with cycle-driven VBlank status and IRQ source 53;
- TVE mode/fade control, triple framebuffer addresses, buffer selection and MIU ready status;
- CD servo registers at `0x0806_0004` through `0x0806_006c`;
- deterministic 1×/2×/4×/8× sector scheduling, raw-sector ring-buffer DMA and IRQ source 60;
- firmware-visible CD DSP commands, program memory, disc identification and single-track Q subchannel;
- SPG290 I²C master registers at `0x0813_0020` through `0x0813_0038`;
- cycle-scheduled 8-bit, 16-bit and repeating I²C transfers, acknowledge bits and IRQ source 39;
- both HyperScan controllers, including buttons, analog axes and sampled-byte checksums;
- RFID output on GPIO bit 1 at `0x0820_0024` and response input at `0x0820_0068`;
- six timer register blocks at `0x0816_0000` through `0x0816_5fff`;
- timer gate/reload controls beginning at `0x0821_006c`;
- shared timer clock selection at `0x0821_00e4`;
- timer mode counter, preload/reload, overflow status/acknowledge and IRQ source 56;
- 27 MHz divided clocks and the 32.768 kHz source using integer phase accumulation.

Capture, comparison and PWM timer modes remain explicit errors. Timer timing is
advanced from emulated CPU cycles rather than host wall-clock time, preserving
determinism. The current CPU cost is still a six-cycle estimate, so long-running
timer accuracy is provisional.

The controller implementation models the firmware-visible protocol rather than
the internal controller MCU and ADC. Writes to the controller bus complete but
have no external side effect because no write command has yet been identified.

RFID pulse widths are measured in emulated CPU cycles and converted to the
13.56 MHz carrier clock. REQA, WUPA, RID, READ, RALL, WRITE-E and WRITE-NE are
implemented with framing, odd parity and protocol CRC responses.

The CD path currently supports the validated single MODE1 data-track layout.
CD audio requests remain an explicit error, and generated Q-subchannel CRC bytes
are placeholders pending a subcode conformance test.

The display path schedules NTSC/PAL frames from the 27 MHz pixel clock and
converts the selected RGB565 direct framebuffer to XRGB8888. Progressive output
duplicates line pairs as observed. PPU palette and sprite RAM are addressable,
but text layers and sprites are not composed yet.
