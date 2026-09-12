# SPG290 MMIO status

The bus accepts explicitly modeled MMIO registers with 8-bit, 16-bit and 32-bit
little-endian accesses. Unknown addresses stop execution with the access type and
address.

Implemented today:

- PPU register/RAM windows at `0x0801_0000`, including separate character and sprite palettes, with cycle-driven VBlank status and IRQ source 53;
- SPU register and internal SRAM windows, while DAC FIFO behavior remains the first synthesized audio path;
- DAC FIFO control at `0x0805_1034` through `0x0805_1474`, plus clock and buffer registers;
- TVE mode/fade control, triple framebuffer addresses, buffer selection and MIU ready status;
- interrupt pending, software interrupt, priority and mask registers at `0x080a_0000` through `0x080a_0024`;
- CD servo registers at `0x0806_0000` through `0x0806_0080`;
- deterministic 1×/2×/4×/8× sector scheduling, raw-sector ring-buffer DMA and IRQ source 60;
- firmware-visible CD DSP commands, program memory, disc identification and single-track Q subchannel;
- SPG290 I²C master registers at `0x0813_0020` through `0x0813_0038`;
- cycle-scheduled 8-bit, 16-bit and repeating I²C transfers, acknowledge bits and IRQ source 39;
- both HyperScan controllers, including buttons, analog axes and sampled-byte checksums;
- UART setup, ready/empty status and deterministic transmit capture;
- RFID output on GPIO bit 1 at `0x0820_0024` and response input at `0x0820_0068`;
- system configuration, clock, MIU and buffer-control register files needed by BIOS initialization;
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
duplicates line pairs as observed. PPU bitmap layers support line tables,
positions, RGB565/ARGB1555 transparency, depth and blending. Character tiles use
number tables and the character palette; sprites use their descriptor RAM,
pattern buffer and independent palette.

The interrupt controller exposes peripheral level state using the documented
vector-to-pending-bit mapping. Priority fields are retained and readable; the CPU
still uses provisional highest-vector arbitration until priority conformance
tests are available.

The DAC follows its 54 MHz hardware clock, reads unsigned PCM from the configured
ring buffer, emits signed stereo samples and raises SPU IRQ source 63 at each
half-buffer boundary. Hardware synthesis registers remain future work.
