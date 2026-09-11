# SPG290 MMIO status

The bus accepts only explicitly modeled 32-bit MMIO registers. Unknown addresses
and unsupported access widths stop execution with the access type and address.

Implemented today:

- six timer register blocks at `0x0816_0000` through `0x0816_5fff`;
- timer gate/reload controls beginning at `0x0821_006c`;
- shared timer clock selection at `0x0821_00e4`;
- timer mode counter, preload/reload, overflow status/acknowledge and IRQ source 56;
- 27 MHz divided clocks and the 32.768 kHz source using integer phase accumulation.

Capture, comparison and PWM timer modes remain explicit errors. Timer timing is
advanced from emulated CPU cycles rather than host wall-clock time, preserving
determinism. The current CPU cost is still a six-cycle estimate, so long-running
timer accuracy is provisional.
