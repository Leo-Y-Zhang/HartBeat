//! `hartbeat <program.elf> [--steps N] [--dump-regs]`
//!
//! Loads an ELF32 RISC-V executable and runs it until it halts or the step
//! budget runs out.

use std::process::ExitCode;

use hartbeat::{elf, Hart, Stop, Trap};

fn usage() -> &'static str {
    "usage: hartbeat <program.elf> [--steps N] [--dump-regs]\n\
     \n\
     Runs an RV32IM program. Exit 0 if it halted at ecall or ebreak, 1 if it\n\
     trapped, 2 if the input could not be read or the budget ran out."
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage());
        return ExitCode::from(2);
    }

    let mut path: Option<String> = None;
    let mut steps: u64 = 10_000_000;
    let mut dump = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--steps" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse::<u64>().ok()) {
                    Some(n) => steps = n,
                    None => {
                        eprintln!("--steps needs a number");
                        return ExitCode::from(2);
                    }
                }
            }
            "--dump-regs" => dump = true,
            other if path.is_none() => path = Some(other.to_string()),
            other => {
                eprintln!("unexpected argument: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    let Some(path) = path else {
        eprintln!("{}", usage());
        return ExitCode::from(2);
    };

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::from(2);
        }
    };

    let mut hart = Hart::new(0);
    let entry = match elf::load(&bytes, &mut hart.mem) {
        Ok(entry) => entry,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::from(2);
        }
    };
    hart.pc = entry;

    let stop = hart.run(steps);

    if dump {
        for (i, value) in hart.regs().iter().enumerate() {
            println!("x{i:<2} {value:#010x}");
        }
        println!("pc  {:#010x}", hart.pc);
    }

    match stop {
        Stop::Halted(Trap::EnvironmentCall { .. }) | Stop::Halted(Trap::Breakpoint { .. }) => {
            println!("halted after {} instructions", hart.steps());
            ExitCode::SUCCESS
        }
        Stop::Halted(trap) => {
            eprintln!("trapped after {} instructions: {trap}", hart.steps());
            ExitCode::from(1)
        }
        Stop::StepLimit => {
            eprintln!("step budget of {steps} exhausted");
            ExitCode::from(2)
        }
    }
}
