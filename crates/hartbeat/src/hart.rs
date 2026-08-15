//! The hart: architectural state, and one instruction's worth of progress.

use crate::decode::decode;
use crate::exec::execute;
use crate::mem::Memory;
use crate::trap::Trap;

/// Why a run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// `ecall` or `ebreak`. The usual way a guest says it is finished.
    Halted(Trap),
    /// The step budget ran out. Not an error -- the caller set the budget.
    StepLimit,
}

/// One RISC-V hardware thread.
#[derive(Clone)]
pub struct Hart {
    regs: [u32; 32],
    pub pc: u32,
    pub mem: Memory,
    steps: u64,
}

impl Hart {
    pub fn new(entry: u32) -> Self {
        Self {
            regs: [0; 32],
            pc: entry,
            mem: Memory::new(),
            steps: 0,
        }
    }

    /// `x0` reads as zero, always. Enforced here rather than at each write site
    /// so that no instruction can forget it.
    #[inline]
    pub fn reg(&self, index: u8) -> u32 {
        let index = index as usize & 31;
        if index == 0 {
            0
        } else {
            self.regs[index]
        }
    }

    /// Writes to `x0` are discarded.
    #[inline]
    pub fn set_reg(&mut self, index: u8, value: u32) {
        let index = index as usize & 31;
        if index != 0 {
            self.regs[index] = value;
        }
    }

    /// The whole register file, `x0` first. For state comparison.
    pub fn regs(&self) -> [u32; 32] {
        let mut out = self.regs;
        out[0] = 0;
        out
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Fetch, decode, execute. `pc` is advanced by the executor, because jumps
    /// and branches need to overwrite rather than adjust it.
    pub fn step(&mut self) -> Result<(), Trap> {
        let pc = self.pc;
        if pc % 4 != 0 {
            return Err(Trap::MisalignedAccess {
                pc,
                addr: pc,
                width: 4,
            });
        }
        let word = self.mem.read(pc, 4, pc)?;
        let instr = decode(word).ok_or(Trap::IllegalInstruction { pc, word })?;
        execute(self, instr)?;
        self.steps += 1;
        Ok(())
    }

    /// Step until something stops it.
    pub fn run(&mut self, max_steps: u64) -> Stop {
        for _ in 0..max_steps {
            if let Err(trap) = self.step() {
                return Stop::Halted(trap);
            }
        }
        Stop::StepLimit
    }
}
