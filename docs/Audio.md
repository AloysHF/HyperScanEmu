# Audio implementation

The first audio path models the SPG290 DAC ring-buffer DMA used for software-mixed
PCM. Its sample clock is 54 MHz divided by `P_DAC_SAMPLE_CLK + 1`. The validated
CD-quality divisor `1223` therefore produces approximately 44.117 kHz.

Implemented behavior:

- 1, 2, 4 and 8 KiB circular buffers in emulated memory;
- mono and interleaved stereo 16-bit DMA;
- unsigned PCM to signed host-sample conversion;
- programmable channel and clock enable state;
- half-buffer and wrap interrupts on vector 63;
- deterministic, integer-only sample scheduling.

The output queue contains interleaved signed 16-bit stereo frames and is drained
once per video frame. The 24-channel PCM/ADPCM synthesizer, envelope engine and
beat/envelope interrupts are not implemented yet.
