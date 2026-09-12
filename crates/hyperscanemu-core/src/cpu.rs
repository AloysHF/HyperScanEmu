use crate::{Bus, EmulatorError};

pub const RESET_PC: u32 = 0x9f00_0000;
pub const CYCLES_PER_INSTRUCTION_ESTIMATE: u64 = 6;

const FLAG_V: u32 = 1 << 0;
const FLAG_C: u32 = 1 << 1;
const FLAG_Z: u32 = 1 << 2;
const FLAG_N: u32 = 1 << 3;
const FLAG_T: u32 = 1 << 4;

const CR_PSR: usize = 0;
const CR_FLAGS: usize = 1;
const CR_ECR: usize = 2;
const CR_EXCEPTION_VECTOR: usize = 3;
const CR_EPC: usize = 5;
const CR_EMA: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CpuException {
    Parity = 6,
    Syscall = 7,
    ReservedInstruction = 9,
    Trap = 10,
    Interrupt = 20,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Advanced { width: u8 },
    Exception { cause: CpuException, width: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score7 {
    registers: [u32; 32],
    control: [u32; 32],
    special: [u32; 3],
    ce_high: u32,
    ce_low: u32,
    pc: u32,
    previous_pc: u32,
    cycles: u64,
    pending_interrupts: u64,
}

impl Default for Score7 {
    fn default() -> Self {
        Self::new()
    }
}

impl Score7 {
    pub fn new() -> Self {
        let mut cpu = Self {
            registers: [0; 32],
            control: [0; 32],
            special: [0; 3],
            ce_high: 0,
            ce_low: 0,
            pc: RESET_PC,
            previous_pc: RESET_PC,
            cycles: 0,
            pending_interrupts: 0,
        };
        cpu.control[CR_EXCEPTION_VECTOR] = RESET_PC;
        cpu.control[29] = 0x2000_0000;
        cpu
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn pc(&self) -> u32 {
        self.pc
    }

    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    pub fn register(&self, index: usize) -> Option<u32> {
        self.registers.get(index).copied()
    }

    pub fn set_register(&mut self, index: usize, value: u32) -> Result<(), EmulatorError> {
        let register = self
            .registers
            .get_mut(index)
            .ok_or(EmulatorError::InvalidRegister { index })?;
        *register = value;
        Ok(())
    }

    pub fn control_register(&self, index: usize) -> Option<u32> {
        self.control.get(index).copied()
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    pub fn set_control_register(&mut self, index: usize, value: u32) -> Result<(), EmulatorError> {
        let register = self
            .control
            .get_mut(index)
            .ok_or(EmulatorError::InvalidControlRegister { index })?;
        *register = value;
        Ok(())
    }

    pub fn request_interrupt(&mut self, source: u8) -> Result<(), EmulatorError> {
        if !(1..64).contains(&source) {
            return Err(EmulatorError::InvalidInterruptSource { source });
        }
        self.pending_interrupts |= 1_u64 << source;
        Ok(())
    }

    pub fn step(&mut self, bus: &mut Bus) -> Result<StepOutcome, EmulatorError> {
        self.previous_pc = self.pc;
        if self.control[CR_PSR] & 1 != 0 && self.pending_interrupts != 0 {
            let source = (63 - self.pending_interrupts.leading_zeros()) as u8;
            self.pending_interrupts &= !(1_u64 << source);
            self.enter_interrupt(source);
            return Ok(StepOutcome::Exception {
                cause: CpuException::Interrupt,
                width: 0,
            });
        }
        let raw = bus.read_u32(self.pc & !3)?;
        let format = ((raw >> 30) & 2) | ((raw >> 15) & 1);
        let outcome = match format {
            0 => {
                let instruction = if self.pc & 2 == 0 {
                    raw as u16 & 0x7fff
                } else {
                    (raw >> 16) as u16 & 0x7fff
                };
                self.pc = self.pc.wrapping_add(2);
                self.execute16(instruction, bus)?
            }
            1 => {
                self.pc = self.pc.wrapping_add(4);
                self.enter_exception(CpuException::Parity);
                StepOutcome::Exception {
                    cause: CpuException::Parity,
                    width: 4,
                }
            }
            2 => {
                let instruction = if self.flag(FLAG_T) {
                    raw as u16 & 0x7fff
                } else {
                    (raw >> 16) as u16 & 0x7fff
                };
                self.pc = self.pc.wrapping_add(4);
                self.execute16(instruction, bus)?
            }
            3 => {
                let instruction = (raw & 0x7fff) | ((raw >> 1) & 0x3fff_8000);
                self.pc = self.pc.wrapping_add(4);
                self.execute32(instruction, bus)?
            }
            _ => unreachable!(),
        };
        self.cycles = self.cycles.wrapping_add(CYCLES_PER_INSTRUCTION_ESTIMATE);
        Ok(outcome)
    }

    fn execute32(&mut self, instruction: u32, bus: &mut Bus) -> Result<StepOutcome, EmulatorError> {
        let opcode = (instruction >> 25) & 0x1f;
        match opcode {
            0x00 => self.execute_special(instruction),
            0x01 => self.execute_immediate(instruction, false),
            0x02 => {
                let displacement = (instruction >> 1) & 0x00ff_ffff;
                if instruction & 1 != 0 {
                    self.registers[3] = self.pc;
                }
                self.pc = (self.previous_pc & 0xfe00_0000) | (displacement << 1);
                Ok(StepOutcome::Advanced { width: 4 })
            }
            0x03 => self.execute_indexed(instruction, bus, true),
            0x04 => {
                let condition = ((instruction >> 10) & 0x1f) as u8;
                if self.check_branch_condition(condition) {
                    let split = ((instruction >> 6) & 0x0007_fe00) | ((instruction >> 1) & 0x01ff);
                    let displacement = sign_extend(split, 19) << 1;
                    if instruction & 1 != 0 {
                        self.registers[3] = self.pc;
                    }
                    self.pc = self.previous_pc.wrapping_add(displacement);
                }
                Ok(StepOutcome::Advanced { width: 4 })
            }
            0x05 => self.execute_immediate(instruction, true),
            0x06 => self.execute_control(instruction),
            0x07 => self.execute_indexed(instruction, bus, false),
            0x08 => {
                let rd = field(instruction, 20, 0x1f);
                let ra = field(instruction, 15, 0x1f);
                let operand = sign_extend((instruction >> 1) & 0x3fff, 14);
                let result = self.registers[ra].wrapping_add(operand);
                if instruction & 1 != 0 {
                    self.set_add_flags(self.registers[ra], operand, result);
                }
                self.registers[rd] = result;
                Ok(StepOutcome::Advanced { width: 4 })
            }
            0x0c | 0x0d => {
                let rd = field(instruction, 20, 0x1f);
                let ra = field(instruction, 15, 0x1f);
                let immediate = (instruction >> 1) & 0x3fff;
                let result = if opcode == 0x0c {
                    self.registers[ra] & immediate
                } else {
                    self.registers[ra] | immediate
                };
                self.registers[rd] = result;
                if instruction & 1 != 0 {
                    self.set_logic_flags(result);
                }
                Ok(StepOutcome::Advanced { width: 4 })
            }
            0x10..=0x17 => self.execute_load_store(instruction, bus, opcode as u8 - 0x10),
            0x18 => Ok(StepOutcome::Advanced { width: 4 }),
            _ => self.unsupported(instruction, 4),
        }
    }

    fn execute_special(&mut self, instruction: u32) -> Result<StepOutcome, EmulatorError> {
        let rd = field(instruction, 20, 0x1f);
        let ra = field(instruction, 15, 0x1f);
        let rb = field(instruction, 10, 0x1f);
        let function = (instruction >> 1) & 0x3f;
        let update = instruction & 1 != 0;
        let a = self.registers[ra];
        let b = self.registers[rb];

        match function {
            0x00 => {}
            0x01 => {
                self.enter_exception(CpuException::Syscall);
                return Ok(StepOutcome::Exception {
                    cause: CpuException::Syscall,
                    width: 4,
                });
            }
            0x02 => {
                if self.check_condition(rb as u8) {
                    self.enter_exception(CpuException::Trap);
                    return Ok(StepOutcome::Exception {
                        cause: CpuException::Trap,
                        width: 4,
                    });
                }
            }
            0x04 => {
                if self.check_branch_condition(rb as u8) {
                    if update {
                        self.registers[3] = self.pc;
                    }
                    self.pc = a;
                }
            }
            0x08 | 0x09 => {
                let carry = u32::from(function == 0x09 && self.flag(FLAG_C));
                let operand = b.wrapping_add(carry);
                let result = a.wrapping_add(operand);
                if update {
                    self.set_add_flags(a, operand, result);
                }
                self.registers[rd] = result;
            }
            0x0a | 0x0b => {
                let borrow = u32::from(function == 0x0b && !self.flag(FLAG_C));
                let operand = b.wrapping_add(borrow);
                let result = a.wrapping_sub(operand);
                if update {
                    self.set_sub_flags(a, operand, result);
                }
                self.registers[rd] = result;
            }
            0x0c | 0x0d => {
                let left = if function == 0x0d { 0 } else { a };
                let result = left.wrapping_sub(b);
                if update {
                    self.set_sub_flags(left, b, result);
                    if rd & 3 <= 1 {
                        self.set_flag(
                            FLAG_T,
                            if rd & 3 == 0 {
                                self.flag(FLAG_Z)
                            } else {
                                self.flag(FLAG_N)
                            },
                        );
                    }
                }
            }
            0x0f => {
                let result = 0_u32.wrapping_sub(b);
                if update {
                    self.set_sub_flags(0, b, result);
                }
                self.registers[rd] = result;
            }
            0x10..=0x13 => {
                let result = match function {
                    0x10 => a & b,
                    0x11 => a | b,
                    0x12 => !a,
                    _ => a ^ b,
                };
                self.registers[rd] = result;
                if update {
                    self.set_logic_flags(result);
                }
            }
            0x14..=0x17 => {
                let mask = 1_u32 << rb;
                if function == 0x16 {
                    if update {
                        self.set_logic_flags(a & mask);
                    }
                } else {
                    let result = match function {
                        0x14 => a & !mask,
                        0x15 => a | mask,
                        _ => a ^ mask,
                    };
                    self.registers[rd] = result;
                    if update {
                        self.set_logic_flags(result);
                    }
                }
            }
            0x18 | 0x1a | 0x1b | 0x1c | 0x1e => {
                let amount = b & 0x1f;
                let result = self.shift(a, amount, function, update);
                self.registers[rd] = result;
            }
            0x20 => {
                if update {
                    return self.unsupported(instruction, 4);
                }
                let result = i64::from(a as i32).wrapping_mul(i64::from(b as i32));
                self.ce_high = (result >> 32) as u32;
                self.ce_low = result as u32;
            }
            0x21 => {
                let result = u64::from(a).wrapping_mul(u64::from(b));
                self.ce_high = (result >> 32) as u32;
                self.ce_low = result as u32;
            }
            0x22 | 0x23 => {
                if b == 0 {
                    return Err(EmulatorError::DivisionByZero {
                        pc: self.previous_pc,
                    });
                }
                if function == 0x22 {
                    let dividend = a as i32;
                    let divisor = b as i32;
                    self.ce_low = dividend.wrapping_div(divisor) as u32;
                    self.ce_high = dividend.wrapping_rem(divisor) as u32;
                } else {
                    self.ce_low = a / b;
                    self.ce_high = a % b;
                }
            }
            0x24 => match rb & 3 {
                1 => self.registers[rd] = self.ce_low,
                2 => self.registers[rd] = self.ce_high,
                3 => {
                    self.registers[rd] = self.ce_high;
                    self.registers[ra] = self.ce_low;
                }
                _ => return self.unsupported(instruction, 4),
            },
            0x25 => match rb & 3 {
                1 => self.ce_low = self.registers[rd],
                2 => self.ce_high = self.registers[rd],
                3 => {
                    self.ce_high = self.registers[rd];
                    self.ce_low = self.registers[ra];
                }
                _ => return self.unsupported(instruction, 4),
            },
            0x28 => {
                if rb >= self.special.len() {
                    return self.unsupported(instruction, 4);
                }
                self.registers[ra] = self.special[rb];
            }
            0x29 => {
                if rb >= self.special.len() {
                    return self.unsupported(instruction, 4);
                }
                self.special[rb] = self.registers[ra];
            }
            0x2a => self.set_flag(FLAG_T, self.check_condition(rb as u8)),
            0x2b => {
                if self.check_condition(rb as u8) {
                    self.registers[rd] = a;
                }
            }
            0x2c..=0x2f => {
                let result = match function {
                    0x2c => (a as u8 as i8 as i32) as u32,
                    0x2d => (a as u16 as i16 as i32) as u32,
                    0x2e => a & 0xff,
                    _ => a & 0xffff,
                };
                self.registers[rd] = result;
                if update {
                    self.set_logic_flags(result);
                }
            }
            0x38 | 0x3a | 0x3b | 0x3c | 0x3e => {
                let result = self.shift(a, rb as u32, function - 0x20, update);
                self.registers[rd] = result;
            }
            _ => return self.unsupported(instruction, 4),
        }

        Ok(StepOutcome::Advanced { width: 4 })
    }

    fn execute_immediate(
        &mut self,
        instruction: u32,
        shifted: bool,
    ) -> Result<StepOutcome, EmulatorError> {
        let rd = field(instruction, 20, 0x1f);
        let function = (instruction >> 17) & 7;
        let raw = (instruction >> 1) & 0xffff;
        let operand = if shifted {
            raw << 16
        } else {
            sign_extend(raw, 16)
        };
        let logical = if shifted { raw << 16 } else { raw };
        let update = instruction & 1 != 0;
        match function {
            0 => {
                let result = self.registers[rd].wrapping_add(operand);
                if update {
                    self.set_add_flags(self.registers[rd], operand, result);
                }
                self.registers[rd] = result;
            }
            2 => {
                if update {
                    let result = self.registers[rd].wrapping_sub(operand);
                    self.set_sub_flags(self.registers[rd], operand, result);
                }
            }
            4 | 5 => {
                let result = if function == 4 {
                    self.registers[rd] & logical
                } else {
                    self.registers[rd] | logical
                };
                self.registers[rd] = result;
                if update {
                    self.set_logic_flags(result);
                }
            }
            6 if !update => self.registers[rd] = operand,
            _ => return self.unsupported(instruction, 4),
        }
        Ok(StepOutcome::Advanced { width: 4 })
    }

    fn execute_control(&mut self, instruction: u32) -> Result<StepOutcome, EmulatorError> {
        let rd = field(instruction, 20, 0x1f);
        let cr = field(instruction, 15, 0x1f);
        match instruction & 0xff {
            0x00 => self.control[cr] = self.registers[rd],
            0x01 => self.registers[rd] = self.control[cr],
            0x84 => {
                self.control[CR_PSR] =
                    (self.control[CR_PSR] & !3) | ((self.control[CR_PSR] >> 2) & 3);
                self.control[CR_FLAGS] =
                    (self.control[CR_FLAGS] & !0x1f) | ((self.control[CR_FLAGS] >> 5) & 0x1f);
                self.pc = self.control[CR_EPC];
            }
            _ => return self.unsupported(instruction, 4),
        }
        Ok(StepOutcome::Advanced { width: 4 })
    }

    fn execute_indexed(
        &mut self,
        instruction: u32,
        bus: &mut Bus,
        pre_increment: bool,
    ) -> Result<StepOutcome, EmulatorError> {
        let rd = field(instruction, 20, 0x1f);
        let ra = field(instruction, 15, 0x1f);
        let displacement = sign_extend((instruction >> 3) & 0x0fff, 12);
        if pre_increment {
            self.registers[ra] = self.registers[ra].wrapping_add(displacement);
        }
        self.access_memory(bus, instruction & 7, rd, self.registers[ra])?;
        if !pre_increment {
            self.registers[ra] = self.registers[ra].wrapping_add(displacement);
        }
        Ok(StepOutcome::Advanced { width: 4 })
    }

    fn execute_load_store(
        &mut self,
        instruction: u32,
        bus: &mut Bus,
        function: u8,
    ) -> Result<StepOutcome, EmulatorError> {
        let rd = field(instruction, 20, 0x1f);
        let ra = field(instruction, 15, 0x1f);
        let displacement = sign_extend(instruction & 0x7fff, 15);
        let address = self.registers[ra].wrapping_add(displacement);
        self.access_memory(bus, u32::from(function), rd, address)?;
        Ok(StepOutcome::Advanced { width: 4 })
    }

    fn access_memory(
        &mut self,
        bus: &mut Bus,
        function: u32,
        rd: usize,
        address: u32,
    ) -> Result<(), EmulatorError> {
        match function {
            0 => self.registers[rd] = bus.read_u32(address)?,
            1 => self.registers[rd] = (bus.read_u16(address)? as i16 as i32) as u32,
            2 => self.registers[rd] = u32::from(bus.read_u16(address)?),
            3 => self.registers[rd] = (bus.read_u8(address)? as i8 as i32) as u32,
            4 => bus.write_u32(address, self.registers[rd])?,
            5 => bus.write_u16(address, self.registers[rd] as u16)?,
            6 => self.registers[rd] = u32::from(bus.read_u8(address)?),
            7 => bus.write_u8(address, self.registers[rd] as u8)?,
            _ => unreachable!(),
        }
        Ok(())
    }

    fn execute16(&mut self, instruction: u16, bus: &mut Bus) -> Result<StepOutcome, EmulatorError> {
        let opcode = (instruction >> 12) & 7;
        match opcode {
            0 => self.execute16_register(instruction),
            2 => self.execute16_alu_memory(instruction, bus),
            3 => {
                if instruction & 1 != 0 {
                    self.registers[3] = self.pc;
                }
                self.pc = (self.previous_pc & 0xffff_f000)
                    | (u32::from((instruction >> 1) & 0x07ff) << 1);
                Ok(StepOutcome::Advanced { width: 2 })
            }
            4 => {
                let condition = ((instruction >> 8) & 0x0f) as u8;
                if self.check_branch_condition(condition) {
                    let displacement = sign_extend(u32::from(instruction & 0xff), 8) << 1;
                    self.pc = self.previous_pc.wrapping_add(displacement);
                }
                Ok(StepOutcome::Advanced { width: 2 })
            }
            5 => {
                let rd = usize::from((instruction >> 8) & 0x0f);
                self.registers[rd] = u32::from(instruction & 0xff);
                Ok(StepOutcome::Advanced { width: 2 })
            }
            6 => self.execute16_immediate(instruction),
            7 => self.execute16_bp_memory(instruction, bus),
            _ => self.unsupported(u32::from(instruction), 2),
        }
    }

    fn execute16_register(&mut self, instruction: u16) -> Result<StepOutcome, EmulatorError> {
        let rd = usize::from((instruction >> 8) & 0x0f);
        let ra = usize::from((instruction >> 4) & 0x0f);
        let function = instruction & 0x0f;
        match function {
            0 => {}
            1 => self.registers[rd] = self.registers[16 + ra],
            2 => self.registers[16 + rd] = self.registers[ra],
            3 => self.registers[rd] = self.registers[ra],
            4 | 12 => {
                if self.check_branch_condition(rd as u8) {
                    if function == 12 {
                        self.registers[3] = self.pc;
                    }
                    self.pc = self.registers[ra];
                }
            }
            5 => self.set_flag(FLAG_T, self.check_condition(rd as u8)),
            8 | 10 | 11 => {
                let amount = self.registers[ra] & 0x1f;
                self.registers[rd] = self.shift(
                    self.registers[rd],
                    amount,
                    if function == 8 {
                        0x18
                    } else if function == 10 {
                        0x1a
                    } else {
                        0x1b
                    },
                    false,
                );
            }
            9 => {
                self.registers[rd] = self.registers[rd]
                    .wrapping_add(self.registers[ra])
                    .wrapping_add(u32::from(self.flag(FLAG_C)));
            }
            _ => return self.unsupported(u32::from(instruction), 2),
        }
        Ok(StepOutcome::Advanced { width: 2 })
    }

    fn execute16_alu_memory(
        &mut self,
        instruction: u16,
        bus: &mut Bus,
    ) -> Result<StepOutcome, EmulatorError> {
        let rd = usize::from((instruction >> 8) & 0x0f);
        let ra = usize::from((instruction >> 4) & 0x0f);
        let function = instruction & 0x0f;
        let a = self.registers[rd];
        let b = self.registers[ra];
        match function {
            0 => {
                let result = a.wrapping_add(b);
                self.set_add_flags(a, b, result);
                self.registers[rd] = result;
            }
            1 | 3 => {
                let result = a.wrapping_sub(b);
                self.set_sub_flags(a, b, result);
                if function == 1 {
                    self.registers[rd] = result;
                }
            }
            2 => {
                let result = 0_u32.wrapping_sub(b);
                self.set_sub_flags(0, b, result);
                self.registers[rd] = result;
            }
            4..=7 => {
                let result = match function {
                    4 => a & b,
                    5 => a | b,
                    6 => !b,
                    _ => a ^ b,
                };
                self.registers[rd] = result;
                self.set_logic_flags(result);
            }
            8 => self.registers[rd] = bus.read_u32(b)?,
            9 => self.registers[rd] = (bus.read_u16(b)? as i16 as i32) as u32,
            10 => {
                let data_register =
                    usize::from(((instruction >> 8) & 0x0f) | ((instruction >> 3) & 0x10));
                let address_register = usize::from((instruction >> 4) & 7);
                self.registers[data_register] = bus.read_u32(self.registers[address_register])?;
                self.registers[address_register] = self.registers[address_register].wrapping_add(4);
            }
            11 => self.registers[rd] = u32::from(bus.read_u8(b)?),
            12 => bus.write_u32(b, a)?,
            13 => bus.write_u16(b, a as u16)?,
            14 => {
                let data_register =
                    usize::from(((instruction >> 8) & 0x0f) | ((instruction >> 3) & 0x10));
                let address_register = usize::from((instruction >> 4) & 7);
                self.registers[address_register] = self.registers[address_register].wrapping_sub(4);
                bus.write_u32(
                    self.registers[address_register],
                    self.registers[data_register],
                )?;
            }
            15 => bus.write_u8(b, a as u8)?,
            _ => unreachable!(),
        }
        Ok(StepOutcome::Advanced { width: 2 })
    }

    fn execute16_immediate(&mut self, instruction: u16) -> Result<StepOutcome, EmulatorError> {
        let rd = usize::from((instruction >> 8) & 0x0f);
        let immediate = u32::from((instruction >> 3) & 0x1f);
        let function = instruction & 7;
        match function {
            0 => {
                let delta = 1_u32 << (immediate & 0x0f);
                self.registers[rd] = if immediate & 0x10 == 0 {
                    self.registers[rd].wrapping_add(delta)
                } else {
                    self.registers[rd].wrapping_sub(delta)
                };
                self.set_logic_flags(self.registers[rd]);
            }
            1 | 3 => {
                let left = function == 1;
                self.registers[rd] = self.shift(
                    self.registers[rd],
                    immediate,
                    if left { 0x18 } else { 0x1a },
                    true,
                );
            }
            4 | 5 | 7 => {
                let mask = 1_u32 << immediate;
                self.registers[rd] = match function {
                    4 => self.registers[rd] & !mask,
                    5 => self.registers[rd] | mask,
                    _ => self.registers[rd] ^ mask,
                };
                self.set_logic_flags(self.registers[rd]);
            }
            6 => {
                self.set_flag(FLAG_N, (self.registers[rd] as i32) < 0);
                self.set_flag(FLAG_Z, self.registers[rd] & (1 << immediate) == 0);
            }
            _ => return self.unsupported(u32::from(instruction), 2),
        }
        Ok(StepOutcome::Advanced { width: 2 })
    }

    fn execute16_bp_memory(
        &mut self,
        instruction: u16,
        bus: &mut Bus,
    ) -> Result<StepOutcome, EmulatorError> {
        let rd = usize::from((instruction >> 8) & 0x0f);
        let immediate = u32::from((instruction >> 3) & 0x1f);
        let function = u32::from(instruction & 7);
        let scale = match function {
            0 | 4 => 4,
            1 | 5 => 2,
            _ => 1,
        };
        let address = self.registers[2].wrapping_add(immediate * scale);
        match function {
            0 => self.registers[rd] = bus.read_u32(address)?,
            1 => self.registers[rd] = (bus.read_u16(address)? as i16 as i32) as u32,
            3 => self.registers[rd] = u32::from(bus.read_u8(address)?),
            4 => bus.write_u32(address, self.registers[rd])?,
            5 => bus.write_u16(address, self.registers[rd] as u16)?,
            7 => bus.write_u8(address, self.registers[rd] as u8)?,
            _ => return self.unsupported(u32::from(instruction), 2),
        }
        Ok(StepOutcome::Advanced { width: 2 })
    }

    fn shift(&mut self, value: u32, amount: u32, function: u32, update: bool) -> u32 {
        let amount = amount & 0x1f;
        if amount == 0 {
            if update {
                self.set_logic_flags(value);
            }
            return value;
        }
        let result = match function {
            0x18 => {
                if update {
                    self.set_flag(FLAG_C, value & (1 << (32 - amount)) != 0);
                }
                value << amount
            }
            0x1a => {
                if update {
                    self.set_flag(FLAG_C, value & (1 << (amount - 1)) != 0);
                }
                value >> amount
            }
            0x1b => {
                if update {
                    self.set_flag(FLAG_C, value & (1 << (amount - 1)) != 0);
                }
                ((value as i32) >> amount) as u32
            }
            0x1c => value.rotate_right(amount),
            0x1e => value.rotate_left(amount),
            _ => unreachable!(),
        };
        if update {
            self.set_logic_flags(result);
        }
        result
    }

    fn check_branch_condition(&mut self, condition: u8) -> bool {
        if condition & 0x0f == 14 {
            if self.special[0] == 0 {
                return false;
            }
            self.special[0] -= 1;
            return true;
        }
        self.check_condition(condition)
    }

    fn check_condition(&self, condition: u8) -> bool {
        let carry = self.flag(FLAG_C);
        let zero = self.flag(FLAG_Z);
        let negative = self.flag(FLAG_N);
        let overflow = self.flag(FLAG_V);
        match condition & 0x0f {
            0 => carry,
            1 => !carry,
            2 => carry && !zero,
            3 => !carry || zero,
            4 => zero,
            5 => !zero,
            6 => !zero && negative == overflow,
            7 => zero || negative != overflow,
            8 => negative == overflow,
            9 => negative != overflow,
            10 => negative,
            11 => !negative,
            12 => overflow,
            13 => !overflow,
            14 => self.special[0] > 0,
            15 => true,
            _ => unreachable!(),
        }
    }

    fn set_add_flags(&mut self, a: u32, b: u32, result: u32) {
        self.set_logic_flags(result);
        self.set_flag(FLAG_C, a.checked_add(b).is_none());
        self.set_flag(FLAG_V, ((a ^ result) & (b ^ result)) >> 31 != 0);
    }

    fn set_sub_flags(&mut self, a: u32, b: u32, result: u32) {
        self.set_logic_flags(result);
        self.set_flag(FLAG_C, a >= b);
        self.set_flag(FLAG_V, ((a ^ b) & (a ^ result)) >> 31 != 0);
    }

    fn set_logic_flags(&mut self, result: u32) {
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_N, (result as i32) < 0);
    }

    fn flag(&self, mask: u32) -> bool {
        self.control[CR_FLAGS] & mask != 0
    }

    fn set_flag(&mut self, mask: u32, value: bool) {
        if value {
            self.control[CR_FLAGS] |= mask;
        } else {
            self.control[CR_FLAGS] &= !mask;
        }
    }

    fn enter_exception(&mut self, cause: CpuException) {
        self.control[CR_ECR] = (self.control[CR_ECR] & !0x1f) | u32::from(cause as u8);
        self.control[CR_PSR] =
            (self.control[CR_PSR] & !0x0f) | ((self.control[CR_PSR] << 2) & 0x0c);
        self.control[CR_FLAGS] =
            (self.control[CR_FLAGS] & !0x03ff) | ((self.control[CR_FLAGS] << 5) & 0x03e0);
        self.control[CR_EPC] = self.previous_pc & !1;
        if cause == CpuException::Parity {
            self.control[CR_EMA] = self.control[CR_EPC];
        }
        self.pc = (self.control[CR_EXCEPTION_VECTOR] & 0xffff_0000).wrapping_add(0x200);
    }

    fn enter_interrupt(&mut self, source: u8) {
        self.enter_exception(CpuException::Interrupt);
        self.control[CR_ECR] = (self.control[CR_ECR] & !0x00fc_0000) | (u32::from(source) << 18);
        let shift = if self.control[CR_EXCEPTION_VECTOR] & 1 != 0 {
            4
        } else {
            2
        };
        self.pc = (self.control[CR_EXCEPTION_VECTOR] & 0xffff_0000)
            .wrapping_add(0x200)
            .wrapping_add(u32::from(source) << shift);
    }

    fn unsupported<T>(&self, instruction: u32, width: u8) -> Result<T, EmulatorError> {
        Err(EmulatorError::UnsupportedInstruction {
            pc: self.previous_pc,
            instruction,
            width,
        })
    }
}

fn field(instruction: u32, shift: u32, mask: u32) -> usize {
    ((instruction >> shift) & mask) as usize
}

fn sign_extend(value: u32, bits: u32) -> u32 {
    (((value << (32 - bits)) as i32) >> (32 - bits)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Firmware, BIOS_ROM_SIZE, INTERNAL_ROM_SIZE};

    fn test_bus() -> Bus {
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[0] = 1;
        bios[0] = 1;
        Bus::new(Firmware::from_parts(&internal, &bios).unwrap())
    }

    fn pack32(instruction: u32) -> u32 {
        0x8000_8000 | (instruction & 0x7fff) | ((instruction & 0x3fff_8000) << 1)
    }

    fn instruction_i(opcode: u32, rd: u32, function: u32, immediate: u16, update: bool) -> u32 {
        (opcode << 25)
            | (rd << 20)
            | (function << 17)
            | (u32::from(immediate) << 1)
            | u32::from(update)
    }

    #[test]
    fn reset_uses_boot_window_and_clears_state() {
        let mut cpu = Score7::new();
        cpu.set_register(4, 123).unwrap();
        cpu.set_pc(0x100);
        cpu.reset();

        assert_eq!(cpu.pc(), RESET_PC);
        assert_eq!(cpu.control_register(CR_EXCEPTION_VECTOR), Some(RESET_PC));
        assert_eq!(cpu.control_register(29), Some(0x2000_0000));
        assert_eq!(cpu.register(4), Some(0));
    }

    #[test]
    fn executes_32_bit_immediate_and_alu_instructions() {
        let mut bus = test_bus();
        let ldi = instruction_i(1, 4, 6, 0x7fff, false);
        let addi = instruction_i(1, 4, 0, 1, true);
        bus.write_u32(0, pack32(ldi)).unwrap();
        bus.write_u32(4, pack32(addi)).unwrap();
        let mut cpu = Score7::new();
        cpu.set_pc(0);

        assert_eq!(
            cpu.step(&mut bus).unwrap(),
            StepOutcome::Advanced { width: 4 }
        );
        assert_eq!(cpu.register(4), Some(0x7fff));
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.register(4), Some(0x8000));
        assert_eq!(cpu.pc(), 8);
        assert_eq!(cpu.cycles(), 12);
    }

    #[test]
    fn executes_load_store_and_sign_extension() {
        let mut bus = test_bus();
        let store_byte = (0x17 << 25) | (5 << 20) | (4 << 15);
        let load_byte = (0x13 << 25) | (6 << 20) | (4 << 15);
        bus.write_u32(0, pack32(store_byte)).unwrap();
        bus.write_u32(4, pack32(load_byte)).unwrap();
        let mut cpu = Score7::new();
        cpu.set_pc(0);
        cpu.set_register(4, 0x100).unwrap();
        cpu.set_register(5, 0x80).unwrap();

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(bus.read_u8(0x100).unwrap(), 0x80);
        assert_eq!(cpu.register(6), Some(0xffff_ff80));
    }

    #[test]
    fn decodes_two_compact_instructions_from_one_word() {
        let mut bus = test_bus();
        let ldiu_r4 = (5_u16 << 12) | (4 << 8) | 7;
        let ldiu_r5 = (5_u16 << 12) | (5 << 8) | 9;
        bus.write_u32(0, u32::from(ldiu_r4) | (u32::from(ldiu_r5) << 16))
            .unwrap();
        let mut cpu = Score7::new();
        cpu.set_pc(0);

        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();

        assert_eq!(cpu.register(4), Some(7));
        assert_eq!(cpu.register(5), Some(9));
        assert_eq!(cpu.pc(), 4);
    }

    #[test]
    fn reports_unsupported_instruction_with_location() {
        let mut bus = test_bus();
        let unsupported = 0x09 << 25;
        bus.write_u32(0, pack32(unsupported)).unwrap();
        let mut cpu = Score7::new();
        cpu.set_pc(0);

        assert_eq!(
            cpu.step(&mut bus),
            Err(EmulatorError::UnsupportedInstruction {
                pc: 0,
                instruction: unsupported,
                width: 4,
            })
        );
    }

    #[test]
    fn parity_format_enters_exception_vector() {
        let mut bus = test_bus();
        bus.write_u32(0, 0x0000_8000).unwrap();
        let mut cpu = Score7::new();
        cpu.set_pc(0);

        assert_eq!(
            cpu.step(&mut bus).unwrap(),
            StepOutcome::Exception {
                cause: CpuException::Parity,
                width: 4,
            }
        );
        assert_eq!(cpu.pc(), 0x9f00_0200);
        assert_eq!(cpu.control_register(CR_EPC), Some(0));
    }

    #[test]
    fn services_highest_pending_interrupt_when_enabled() {
        let mut bus = test_bus();
        let mut cpu = Score7::new();
        cpu.set_pc(0x100);
        cpu.set_control_register(CR_PSR, 1).unwrap();
        cpu.request_interrupt(39).unwrap();
        cpu.request_interrupt(56).unwrap();

        assert_eq!(
            cpu.step(&mut bus).unwrap(),
            StepOutcome::Exception {
                cause: CpuException::Interrupt,
                width: 0,
            }
        );
        assert_eq!(cpu.pc(), 0x9f00_0000 + 0x200 + 56 * 4);
        assert_eq!(cpu.control_register(CR_EPC), Some(0x100));
        assert_eq!(cpu.control_register(CR_ECR).unwrap() >> 18 & 0x3f, 56);
    }
}
