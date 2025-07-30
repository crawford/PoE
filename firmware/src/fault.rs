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
    log::error!("******************");
    unsafe { end() }
}

#[repr(u8)]
enum HFSR {
    VectTbl = 1,
    Forced = 30,
    DebugEvt = 31,
}

#[repr(u8)]
enum CFSR {
    // MMFSR
    IAccViol = 0,
    DAccViol = 1,
    //
    MUnstkErr = 3,
    MStkErr = 4,
    MLSPErr = 5,
    //
    MMARValid = 7,

    // BFSR
    IBusErr = 8,
    PrecisErr = 9,
    ImprecisErr = 10,
    UnstkErr = 11,
    StkErr = 12,
    LSPErr = 13,
    //
    BFARValid = 15,

    // UFSR
    UndefinStr = 16,
    InvState = 17,
    InvPC = 18,
    NoCP = 19,
    StkOf = 20,
    //
    Unaligned = 24,
    DivByZero = 25,
}

fn print_registers(frame: &ExceptionFrame) {
    use cortex_m::register::{msp, psp};
    use log::warn;

    warn!("Registers:");
    warn!(" r0   = {:#010x}", frame.r0);
    warn!(" r1   = {:#010x}", frame.r1);
    warn!(" r2   = {:#010x}", frame.r2);
    warn!(" r3   = {:#010x}", frame.r3);
    warn!(" r12  = {:#010x}", frame.r12);
    warn!(" lr   = {:#010x}", frame.lr);
    warn!(" pc   = {:#010x}", frame.pc);
    warn!(" xpsr = {:#010x}", frame.xpsr);
    warn!(" sp   = {:#010x}", frame as *const ExceptionFrame as u32);
    warn!(" msp  = {:#010x}", msp::read());
    warn!(" psp  = {:#010x}", psp::read());
    warn!("");
}

fn print_fault_status_registers() {
    macro_rules! ifs {
        ($reg:expr, $bit:path, $fmt:literal $( , $args:tt )*) => {
            if $reg & (1 << $bit as u8) != 0 {
                warn!(concat!("  ", $fmt), $( $args )*)
            }
        };
    }

    use log::{info, warn};
    use CFSR::*;
    use HFSR::*;

    let scb = unsafe { &*SCB::ptr() };

    let hfsr = scb.hfsr.read();
    let cfsr = scb.cfsr.read();
    let mmfar = scb.mmfar.read();
    let bfar = scb.bfar.read();

    warn!("Fault Status Registers:");
    warn!(" HFSR = {:#010x}", hfsr);

    // HardFault HFSR
    ifs!(hfsr, VectTbl, "busfault on vector table read");
    ifs!(hfsr, Forced, "fault escalated to hard fault");
    ifs!(hfsr, DebugEvt, "breakpoint escalation");

    warn!(" CFSR = {:#010x}", cfsr);

    // MemManage MMFSR
    ifs!(cfsr, IAccViol, "instruction access violation");
    ifs!(cfsr, DAccViol, "direct data access violation");
    ifs!(cfsr, MStkErr, "context stacking, MPU access violation");
    ifs!(cfsr, MUnstkErr, "context unstacking, MPU access violation");
    ifs!(cfsr, MLSPErr, "lazy floating-point state preservation");
    ifs!(cfsr, MMARValid, "MMAR valid ({:#010x})", mmfar);

    // BusFault BFSR
    ifs!(cfsr, IBusErr, "instruction prefetch error");
    ifs!(cfsr, PrecisErr, "precise data access error");
    ifs!(cfsr, ImprecisErr, "imprecise data access error");
    ifs!(cfsr, UnstkErr, "exception unstacking");
    ifs!(cfsr, StkErr, "exception stacking");
    ifs!(cfsr, LSPErr, "lazy floating-point state preservation");
    ifs!(cfsr, BFARValid, "BFAR valid ({:#010x})", bfar);

    // UsageFault Status Register
    ifs!(cfsr, UndefinStr, "undefined instruction");
    ifs!(cfsr, InvState, "attempt to enter invalid instr. set state");
    ifs!(cfsr, InvPC, "invalid EXC_RETURN");
    ifs!(cfsr, NoCP, "attempt to access non-existing coprocessor");
    ifs!(cfsr, StkOf, "stack overflow");
    ifs!(cfsr, Unaligned, "unaligned access");
    ifs!(cfsr, DivByZero, "divide by zero");

    info!("");
}

fn print_hint(frame: &ExceptionFrame) {
    macro_rules! is_set {
        ($reg:expr, $bit:path) => {{
            $reg & (1 << $bit as u8) != 0
        }};
    }

    use log::info;

    let pc = frame.pc;
    let scb = unsafe { &*SCB::ptr() };
    let cfsr = scb.cfsr.read();
    let bfar = scb.bfar.read();

    info!("Hint:");
    match (
        is_set!(cfsr, CFSR::PrecisErr),
        is_set!(cfsr, CFSR::ImprecisErr),
        is_set!(cfsr, CFSR::BFARValid),
    ) {
        (true, _, true) => info!(" Instruction at {pc:#010x} tried to read {bfar:#010x}"),
        (true, _, false) => info!(" Instruction at {pc:#010x} did something"),
        (_, true, true) => info!(" Instruction near {pc:#010x} tried to write {bfar:#010x}"),
        (_, true, false) => info!(" Instruction near {pc:#010x} did something"),
        _ => info!(" Dig out the manual"),
    }

    if cfsr & (1 << CFSR::MMARValid as u8 | 1 << CFSR::BFARValid as u8) != 0 {}
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
