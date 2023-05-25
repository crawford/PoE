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
                    size: 4096
                    mode: NoBlockTrim
                    name: "logs"
                }
            }
        };

        rtt_target::set_print_channel(channels.up.0);

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
