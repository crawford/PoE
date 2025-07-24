// Copyright 2025 Alex Crawford
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

use core::panic::PanicInfo;
use cortex_m::peripheral::SCB;
use cortex_m::{asm, interrupt};
use cortex_m_rt::ExceptionFrame;
use smoltcp::time::Instant;

// Light up both LEDs red, trigger a breakpoint, and loop
pub fn handle_default(irqn: i16, init: impl FnOnce(i16)) -> ! {
    interrupt::disable();

    init(irqn);

    log::error!("Default Handler: irq {}", irqn);

    unsafe { end() }
}

pub fn handle_hardfault(frame: &ExceptionFrame, init: impl FnOnce(&ExceptionFrame)) -> ! {
    interrupt::disable();

    init(frame);

    log::error!("*** HARD FAULT ***");
    print_registers(frame);
    print_fault_status_registers();
    print_hint(frame);

    unsafe { end() }
}

fn print_registers(frame: &ExceptionFrame) {
    use log::warn;

    warn!("Registers:");
    warn!(" r0   = {:#010X}", frame.r0);
    warn!(" r1   = {:#010X}", frame.r1);
    warn!(" r2   = {:#010X}", frame.r2);
    warn!(" r3   = {:#010X}", frame.r3);
    warn!(" r12  = {:#010X}", frame.r12);
    warn!(" lr   = {:#010X}", frame.lr);
    warn!(" pc   = {:#010X}", frame.pc);
    warn!(" xpsr = {:#010X}", frame.xpsr);
    warn!("");
}

fn print_fault_status_registers() {
    use log::warn;

    macro_rules! if_set {
        ($reg:expr, $bit:expr, $expr:expr) => {
            if $reg & (1 << $bit) != 0 {
                $expr
            }
        };
    }

    macro_rules! if_not_set {
        ($reg:expr, $bit:expr, $expr:expr) => {
            if $reg & (1 << $bit) == 0 {
                $expr
            }
        };
    }

    let scb = unsafe { &*SCB::ptr() };

    let hfsr = scb.hfsr.read();
    let cfsr = scb.cfsr.read();
    let mmfar = scb.mmfar.read();
    let bfar = scb.bfar.read();

    warn!("Fault Status Registers:");
    warn!(" HFSR = {:#010X}", hfsr);
    if_set!(hfsr, 1, warn!(" - busfault on vector table read"));
    if_set!(hfsr, 30, warn!(" - forced hardfault (from escalation)"));

    warn!(" CFSR = {:#010X}", cfsr);

    // Memory Manage Fault Status Register
    if_set!(cfsr, 0, warn!(" - instruction access violation"));
    if_set!(cfsr, 1, warn!(" - data access violation"));
    if_set!(cfsr, 3, warn!(" - memfault unstacking exception"));
    if_set!(cfsr, 4, warn!(" - memfault stacking exception"));
    if_set!(cfsr, 7, warn!(" - MMFAR valid"));

    // BusFault Status Register
    if_set!(cfsr, 8, warn!(" - instruction fetch error"));
    if_set!(cfsr, 9, warn!(" - precise data bus error"));
    if_set!(cfsr, 10, warn!(" - imprecise error"));
    if_set!(cfsr, 11, warn!(" - busfault unstacking exception"));
    if_set!(cfsr, 12, warn!(" - busfault stacking exception"));
    if_set!(cfsr, 15, warn!(" - BFAR valid"));

    // UsageFault Status Register
    if_set!(cfsr, 16, warn!(" - undefined instruction"));
    if_set!(cfsr, 17, warn!(" - illegal use of EPSR"));
    if_set!(cfsr, 18, warn!(" - invalid EXC_RETURN"));
    if_set!(cfsr, 19, warn!(" - no coprocessor"));
    if_set!(cfsr, 24, warn!(" - unaligned access"));
    if_set!(cfsr, 25, warn!(" - divide by zero"));

    if_set!(cfsr, 7, warn!(" MMFAR = {:#010X}", mmfar));
    if_not_set!(cfsr, 7, warn!(" MMFAR = <invalid>"));

    if_set!(cfsr, 15, warn!(" BFAR = {:#010X}", bfar));
    if_not_set!(cfsr, 15, warn!(" BFAR = <invalid>"));

    warn!("");
}

fn print_hint(frame: &ExceptionFrame) {
    use log::info;

    let scb = unsafe { &*SCB::ptr() };
    let cfsr = scb.cfsr.read();

    if cfsr & (1 << 9 | 1 << 15) != 0 {
        info!(
            "Instruction at {:#010X} tried to read {:#010X}",
            frame.pc,
            scb.bfar.read(),
        );
    }
}

pub fn handle_panic(info: &PanicInfo, init: impl FnOnce(&PanicInfo)) -> ! {
    cortex_m::interrupt::disable();

    init(info);

    let rtc = unsafe { &*efm32gg11b820::RTC::ptr() };
    let now = Instant::from_millis(rtc.cnt.read().cnt().bits());

    log::error!("Panic at {}: {}", now, info);

    unsafe { end() }
}

unsafe fn end() -> ! {
    if cortex_m::peripheral::DCB::is_debugger_attached() {
        asm::bkpt();

        loop {
            asm::wfe();
        }
    } else {
        SCB::sys_reset()
    }
}
