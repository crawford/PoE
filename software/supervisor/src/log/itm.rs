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

#![cfg(feature = "itm")]

use efm32gg11b820::{CMU, GPIO, ITM};

pub type Logger = cortex_m_log::log::Logger<cortex_m_log::printer::itm::InterruptSync>;

pub fn new(level: log::LevelFilter, cmu: &CMU, gpio: &GPIO, itm: ITM) -> Logger {
    use cortex_m_log::destination::Itm;
    use cortex_m_log::printer::itm::InterruptSync;

    // Enable the Serial Wire Viewer (ITM on SWO)
    gpio.routepen.write(|reg| reg.swvpen().set_bit());
    gpio.pf_model.modify(|_, w| w.mode2().pushpull());

    // Use the HFRCO divided by two (9.5 MHz) for the ITM
    cmu.dbgclksel.write(|reg| reg.dbg().hfrcodiv2());

    Logger {
        inner: InterruptSync::new(Itm::new(itm)),
        level,
    }
}
