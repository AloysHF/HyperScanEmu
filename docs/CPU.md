# S+Core 7 CPU

The interpreter decodes the architecture's mixed instruction stream directly:

- two sequential 16-bit compact instructions in a 32-bit fetch word;
- a conditional parallel compact form selected by the T flag;
- 32-bit instructions with their on-bus format/parity bits removed before decode.

Implemented 32-bit groups include integer arithmetic, compare and flags, Boolean
operations, shifts and rotates, multiply/divide, CE register moves, immediate
forms, control-register moves, direct/indexed loads and stores, jumps, branches,
`syscall`, `trap`, `rte`, and cache hints. Compact support includes register ALU,
loads/stores, stack operations, immediate bit operations, branches, jumps and
BP-relative memory operations.

Unsupported opcodes stop execution with the exact PC, normalized instruction and
instruction width. They are not treated as NOPs. Cycle accounting currently uses
an explicit six-cycle estimate for every instruction until per-opcode timing is
validated. Custom-engine operations, cache effects, debug operations, interrupts
and several carry-rotate/CE compact forms remain open.
