// Copyright 2023 Alex Crawford
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use core::arch::asm;
use core::cell::UnsafeCell;
use core::fmt::Write;
use core::ops::Range;
use core::{mem, str};
use ignore_result::Ignore;
use InterpreterMode::*;
use InterpreterState::*;

macro_rules! output {
    ($w:expr, $fmt:literal $(, $( $args:expr ),+ )?) => {
        write!($w, $fmt $(, $( $args ),+ )?)
            .map_err(|err| log::warn!("interpretter write failed: {err}"))
            .ignore()
    };
    ($w:expr, $str:expr) => {
        write!($w, "{}", $str)
            .map_err(|err| log::warn!("interpretter write failed: {err}"))
            .ignore()
    };
}

macro_rules! outputln {
    ($w:expr) => {
        output!($w, "\r\n")
    };
    ($w:expr, $arg:tt) => {{
        output!($w, $arg);
        outputln!($w)
    }};
    ($w:expr, $fmt:literal $(, $( $args:expr ),+ )?) => {{
        output!($w, $fmt $(, $( $args ),+ )?);
        outputln!($w)
    }};
}

const HELP_STR: &str = "Command Interpreter

Available commands:

  get <hex address>                Read address
  set <hex address> <hex value>    Write value to address
  read <hex address> <length>      Read bytes starting at address
  erase <hex address> <length>     Erase flash (address and length must be page-aligned)
  write <hex address> <length>     Write input to address
  call <hex address>               Call function at address
  prog addr                        Display the start address of program space
  prog write <length>              Write input to program space
  prog run                         Call function in program space
  help                             Display this help text";

const PROMPT_STR: &str = "> ";

#[repr(transparent)]
pub struct ProgramSpace<const SIZE: usize>(UnsafeCell<[u8; SIZE]>);

unsafe impl<const SIZE: usize> Sync for ProgramSpace<SIZE> {}

impl<const SIZE: usize> ProgramSpace<SIZE> {
    const fn new() -> Self {
        ProgramSpace(UnsafeCell::new([0; SIZE]))
    }

    fn as_ptr(&self) -> *const [u8; SIZE] {
        self.0.get()
    }
}

static PROGRAM_SPACE: ProgramSpace<512> = ProgramSpace::new();

#[derive(Clone, Copy)]
pub enum InterpreterMode {
    Command,
    Data,
}

enum InterpreterState {
    Idle,
    Writing(Range<usize>),
}

pub struct Interpreter {
    state: InterpreterState,
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Interpreter { state: Idle }
    }

    pub fn mode(&self) -> InterpreterMode {
        match self.state {
            Idle => Command,
            Writing(_) => Data,
        }
    }

    pub fn exec<W: Write>(&mut self, input: &[u8], output: &mut W) {
        for line in input.split_inclusive(|b| b == &b'\n') {
            self.state = match self.state {
                Idle => {
                    let cmd = str::from_utf8(line).unwrap_or_else(|err| {
                        log::warn!("failed to parse input ({line:?}): {err}");
                        ""
                    });
                    exec_command(cmd, output)
                }
                Writing(ref region) => write_data(line, region, output),
            }
        }
    }

    pub fn abort<W: Write>(&mut self, output: &mut W) {
        self.state = Idle;
        outputln!(output);
        output!(output, PROMPT_STR);
    }
}

fn exec_command<S, W>(input: S, output: &mut W) -> InterpreterState
where
    S: AsRef<str>,
    W: Write,
{
    let mut tokens = input.as_ref().trim().split(' ');
    'parse: {
        macro_rules! token_hex_u32 {
            ($name:literal) => {
                match tokens.next() {
                    Some(arg) => match arg.strip_prefix("0x") {
                        Some(val) => match u32::from_str_radix(val, 16) {
                            Ok(val) => val,
                            Err(err) => {
                                let name = $name;
                                outputln!(output, "Failed to parse '{name}' ({val}): {err}");
                                break 'parse;
                            }
                        },
                        None => {
                            let name = $name;
                            outputln!(output, "Hexadecimal argument '{name}' must begin with '0x'");
                            break 'parse;
                        }
                    },
                    None => {
                        outputln!(output, HELP_STR);
                        break 'parse;
                    }
                }
            };
        }

        macro_rules! token_hex_usize {
            ($name:literal) => {
                token_hex_u32!($name) as usize
            };
        }

        macro_rules! token_hex_ptr {
            ($name:literal) => {
                token_hex_u32!($name) as *const u32
            };
        }

        macro_rules! word_aligned {
            ($var:expr) => {
                if $var as usize % mem::size_of::<u32>() != 0 {
                    let var = $var as usize;
                    outputln!(output, "Argument '{var}' must be word-aligned");
                    break 'parse;
                }
            };
        }

        macro_rules! page_aligned {
            ($var:expr) => {
                if $var as usize % 512 != 0 {
                    let var = $var as usize;
                    outputln!(output, "Argument '{var}' must be page-aligned");
                    break 'parse;
                }
            };
        }

        match tokens.next() {
            Some("") | None => {}
            Some("help") => outputln!(output, HELP_STR),
            Some("get") => {
                let addr = token_hex_ptr!("addr");
                match (addr as usize) % mem::size_of::<u32>() {
                    0 => {
                        let data = unsafe { *addr };
                        outputln!(output, "0x{data:08X}");
                    }
                    2 => {
                        let data = unsafe { *(addr as *const u16) };
                        outputln!(output, "0x{data:04X}");
                    }
                    1 | 3 => {
                        let data = unsafe { *(addr as *const u8) };
                        outputln!(output, "0x{data:02X}");
                    }
                    _ => unreachable!(),
                }
            }
            Some("set") => {
                let addr = token_hex_u32!("addr");
                let value = token_hex_u32!("value");
                unsafe { *(addr as *mut u32) = value };
            }
            Some("read") => {
                let start = token_hex_ptr!("start");
                word_aligned!(start);
                let length = token_hex_usize!("length");
                word_aligned!(length);

                let len = length / 4;
                for i in 0..len {
                    if i % 4 == 0 {
                        output!(output, "{i:02X}: ");
                    }

                    output!(output, "{:08X}", unsafe { *start.add(i) });

                    if i % 4 == 3 {
                        outputln!(output)
                    } else {
                        output!(output, " ")
                    }
                }
                if len % 4 != 0 {
                    outputln!(output);
                }

                let len = length / 2;
                let start = start as *const u16;
                for i in 0..len {
                    if i % 8 == 0 {
                        output!(output, "{i:02X}: ");
                    }

                    output!(output, "{:04X}", unsafe { *start.add(i) });

                    if i % 8 == 7 {
                        outputln!(output)
                    } else {
                        output!(output, " ")
                    }
                }
                if length % 8 != 0 {
                    outputln!(output);
                }

                let len = length;
                let start = start as *const u8;
                for i in 0..len {
                    if i % 16 == 0 {
                        output!(output, "{i:02X}: ");
                    }

                    output!(output, "{:02X}", unsafe { *start.add(i) });

                    if i % 16 == 15 {
                        outputln!(output)
                    } else {
                        output!(output, " ")
                    }
                }
                if length % 16 != 0 {
                    outputln!(output);
                }
            }
            Some("erase") => {
                let start = token_hex_u32!("addr");
                page_aligned!(start);
                let length = token_hex_u32!("len");
                page_aligned!(length);
                let _ = (start, length);
                outputln!(output, "Unimplemented");
            }
            Some("write") => {
                let start = token_hex_usize!("addr");
                word_aligned!(start);
                let length = token_hex_usize!("len");
                if length > 512 {
                    outputln!(output, "Write is limited to 512 bytes at a time");
                    break 'parse;
                }
                return Writing(Range {
                    start,
                    end: start + length,
                });
            }
            Some("call") => {
                let addr = token_hex_u32!("addr");
                let ret: u32;
                unsafe {
                    asm!("blx {0}",
                         "mov {1}, r0",
                         in(reg) addr,
                         out(reg) ret
                    );
                }
                outputln!(output, "Return value (may not be valid): 0x{ret:08X}");
            }
            Some("prog") => match tokens.next() {
                Some("addr") => outputln!(output, "{:p}", PROGRAM_SPACE.as_ptr()),
                Some("write") => {
                    let length = token_hex_usize!("len");
                    if length > 512 {
                        outputln!(output, "Program write is limited to 512 bytes at a time");
                        break 'parse;
                    }
                    let start = PROGRAM_SPACE.as_ptr();
                    return Writing(Range {
                        start: start as usize,
                        end: start as usize + length,
                    });
                }
                Some("run") => {
                    let addr = PROGRAM_SPACE.as_ptr() as usize | 0b1;
                    let ret: u32;
                    unsafe {
                        asm!("blx {0}",
                             "mov {1}, r0",
                             in(reg) addr,
                             out(reg) ret
                        );
                    }
                    outputln!(output, "Return value (may not be valid): 0x{ret:08X}");
                }
                Some(command) => {
                    outputln!(output, "Unrecognized subcommand: {command} (try 'help')")
                }
                None => outputln!(output, "Unspecified subcommand (try 'help')"),
            },
            Some(command) => outputln!(output, "Unrecognized command: {command} (try 'help')"),
        }
    }

    output!(output, PROMPT_STR);
    Idle
}

// XXX: Does not unescape
/// Write data from input, encoded in hex, to the region provided
fn write_data<W: Write>(input: &[u8], region: &Range<usize>, output: &mut W) -> InterpreterState {
    const HEX_LEN: usize = 2;

    if input.is_empty() {
        return Writing(region.clone());
    }

    let input = input.strip_suffix(b"\r").unwrap_or(input);
    let input = input.strip_suffix(b"\n").unwrap_or(input);
    let input_len = input.len();
    let expected_len = region.len() * HEX_LEN;

    'process: {
        if input_len != expected_len {
            outputln!(
                output,
                "Data isn't the expected length ({input_len} vs {expected_len} bytes)"
            );
            break 'process;
        }

        for (hex, dest) in input.chunks(HEX_LEN).zip(region.clone()) {
            let hex = match str::from_utf8(hex) {
                Ok(text) => text,
                Err(err) => {
                    log::warn!("failed to parse input ({hex:?}): {err}");
                    break 'process;
                }
            };

            match u8::from_str_radix(hex, 16) {
                Ok(byte) => unsafe { *(dest as *mut u8) = byte },
                Err(err) => {
                    outputln!(output, "Invalid word '{hex}': {err}");
                    break 'process;
                }
            };
        }
    }

    output!(output, PROMPT_STR);
    Idle
}
