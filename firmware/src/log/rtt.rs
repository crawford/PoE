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

#![cfg(feature = "rtt")]

use core::fmt::Write;
use core::mem::{self, MaybeUninit};
use core::str;
use ignore_result::Ignore;
use rtt_target::{DownChannel, UpChannel};

pub fn new(level: log::LevelFilter) -> Logger {
    Logger::new(level)
}

pub struct Logger {
    pub level: log::LevelFilter,
}

impl Logger {
    #[rustfmt::skip::macros(rtt_init)]
    fn new(level: log::LevelFilter) -> Logger {
        let channels = rtt_target::rtt_init! {
            up: {
                0: {
                    size: 1024
                    mode: NoBlockTrim
                    name: "terminal"
                }
                1: {
                    size: 4096
                    mode: NoBlockTrim
                    name: "logs"
                }
            }
            down: {
                0: {
                    size: 1024
                    mode: NoBlockTrim
                    name: "terminal"
                }
            }
        };

        rtt_target::set_print_channel(channels.up.1);
        unsafe {
            TERMINAL = MaybeUninit::new(Terminal {
                input: channels.down.0,
                output: channels.up.0,
            });
        }

        Logger { level }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            rtt_target::rprintln!(
                "{:<5} {}:{} - {}",
                record.level(),
                record.file().unwrap_or("UNKNOWN"),
                record.line().unwrap_or(0),
                record.args()
            )
        }
    }

    fn flush(&self) {}
}

static mut TERMINAL: MaybeUninit<Terminal> = MaybeUninit::uninit();

pub struct Terminal {
    output: UpChannel,
    input: DownChannel,
}

macro_rules! output {
    ($writer:expr, $fmt:literal) => {
        write!($writer, $fmt)
            .map_err(|err| log::warn!("terminal write failed: {err}"))
            .ignore()
    };
    ($writer:expr, $str:expr) => {
        write!($writer, "{}", $str)
            .map_err(|err| log::warn!("terminal write failed: {err}"))
            .ignore()
    };
}

macro_rules! outputln {
    ($writer:expr, $fmt:literal) => {{
        output!($writer, $fmt);
        outputln!($writer)
    }};
    ($writer:expr, $str:expr) => {{
        output!($writer, $str);
        outputln!($writer)
    }};
    ($writer:expr) => {
        output!($writer, "\n\r")
    };
}

impl Terminal {
    const HELP_STR: &'static str = "Terminal Help

Available commands:

  get <hex address>                Read address
  set <hex address> <hex value>    Write value to address
  help                             Display this help text";
    const PROMPT_STR: &'static str = "> ";

    pub fn new() -> &'static mut Terminal {
        let terminal = unsafe { TERMINAL.assume_init_mut() };

        // Clear the buffer
        let mut input = [0u8; 1024];
        terminal.input.read(&mut input);

        outputln!(terminal.output);
        output!(terminal.output, Self::PROMPT_STR);
        terminal
    }

    pub fn poll(&mut self) {
        let mut input = [0u8; 1024];
        let len = self.input.read(&mut input);
        if len == 0 {
            return;
        }

        let mut tokens = match str::from_utf8(&input[0..len]) {
            Ok(text) => text,
            Err(err) => {
                log::warn!("failed parsing terminal input: {err}");
                return;
            }
        }
        .trim()
        .split(' ');

        macro_rules! token_u32 {
            ($name:literal) => {
                match tokens.next() {
                    Some(val) => match u32::from_str_radix(val, 16) {
                        Ok(val) => val,
                        Err(err) => {
                            output!(self.output, concat!("Failed to parse ", $name));
                            outputln!(self.output, " ({val}): {err}");
                            return;
                        }
                    },
                    None => {
                        outputln!(self.output, Self::HELP_STR);
                        return;
                    }
                }
            };
        }

        match tokens.next() {
            Some("") | None => {}
            Some("help") => outputln!(self.output, Self::HELP_STR),
            Some("get") => {
                let addr = token_u32!("addr") as usize;
                match addr % mem::size_of::<u32>() {
                    0 => {
                        let data = unsafe { *(addr as *const u32) };
                        outputln!(self.output, "0x{data:08X}");
                    }
                    2 => {
                        let data = unsafe { *(addr as *const u16) };
                        outputln!(self.output, "0x{data:04X}");
                    }
                    1 | 3 => {
                        let data = unsafe { *(addr as *const u8) };
                        outputln!(self.output, "0x{data:02X}");
                    }
                    val => log::error!("unhandled val: {val}"),
                }
            }
            Some("set") => {
                let addr = token_u32!("addr");
                let value = token_u32!("value");
                unsafe { *(addr as *mut u32) = value };
            }
            Some(command) => outputln!(self.output, "Unrecognized command: {command} (try 'help')"),
        }

        output!(self.output, Self::PROMPT_STR);
    }
}
