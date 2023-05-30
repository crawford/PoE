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

use core::{fmt::Write, mem, str};
use ignore_result::Ignore;

const HELP_STR: &str = "Command Interpreter

Available commands:

  get <hex address>                Read address
  set <hex address> <hex value>    Write value to address
  help                             Display this help text";
const PROMPT_STR: &str = "> ";

pub fn interpret<S, W>(command: S, output: &mut W)
where
    S: AsRef<str>,
    W: Write,
{
    macro_rules! output {
        ($fmt:literal) => {
            write!(output, $fmt)
                .map_err(|err| log::warn!("interpretter write failed: {err}"))
                .ignore()
        };
        ($str:expr) => {
            write!(output, "{}", $str)
                .map_err(|err| log::warn!("interpretter write failed: {err}"))
                .ignore()
        };
    }

    macro_rules! outputln {
        () => {
            output!("\n\r")
        };
        ($arg:tt) => {{
            output!($arg);
            outputln!()
        }};
    }

    let mut tokens = command.as_ref().trim().split(' ');
    'parse: {
        macro_rules! token_hex_u32 {
            ($name:literal) => {
                match tokens.next() {
                    Some(arg) => match arg.strip_prefix("0x") {
                        Some(val) => match u32::from_str_radix(val, 16) {
                            Ok(val) => val,
                            Err(err) => {
                                let name = $name;
                                outputln!("Failed to parse {name} ({val}): {err}");
                                break 'parse;
                            }
                        },
                        None => {
                            outputln!("Hexadecimal argument must begin with '0x'");
                            break 'parse;
                        }
                    },
                    None => {
                        outputln!(HELP_STR);
                        break 'parse;
                    }
                }
            };
        }

        match tokens.next() {
            Some("") | None => {}
            Some("help") => outputln!(HELP_STR),
            Some("get") => {
                let addr = token_hex_u32!("addr") as usize;
                match addr % mem::size_of::<u32>() {
                    0 => {
                        let data = unsafe { *(addr as *const u32) };
                        outputln!("0x{data:08X}");
                    }
                    2 => {
                        let data = unsafe { *(addr as *const u16) };
                        outputln!("0x{data:04X}");
                    }
                    1 | 3 => {
                        let data = unsafe { *(addr as *const u8) };
                        outputln!("0x{data:02X}");
                    }
                    _ => unreachable!(),
                }
            }
            Some("set") => {
                let addr = token_hex_u32!("addr");
                let value = token_hex_u32!("value");
                unsafe { *(addr as *mut u32) = value };
            }
            Some(command) => outputln!("Unrecognized command: {command} (try 'help')"),
        }
    }

    output!(PROMPT_STR);
}
