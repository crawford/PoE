// Copyright 2018 Alex Crawford
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(lang_items)]
#![no_std]

extern crate cortex_m;
#[macro_use]
extern crate efm32gg11b820;

use cortex_m::{asm, interrupt};

fn main() {
    let peripherals = efm32gg11b820::Peripherals::take().unwrap();
    let cmu = peripherals.CMU;
    let gpio = peripherals.GPIO;
    let msc = peripherals.MSC;
    let timer = peripherals.TIMER0;
    let mut nvic = efm32gg11b820::CorePeripherals::take().unwrap().NVIC;

    // Enable the HFXO
    cmu.oscencmd.write(|reg| reg.hfxoen().set_bit());
    // Wait for HFX0 to stabilize
    while cmu.status.read().hfxordy().bit_is_clear() {}

    // Update the EMU configuration
    let _ = cmu.status.read().bits();

    // Allow access to low energy peripherals with a clock speed greater than 50MHz
    cmu.ctrl.write(|reg| reg.wshfle().set_bit());

    // Set the appropriate read delay for flash
    msc.readctrl.write(|reg| reg.mode().ws2());

    // Switch to selected oscillator
    cmu.hfclksel.write(|reg| reg.hf().hfxo());

    // Update the EMU configuration
    let _ = cmu.status.read().bits();

    cmu.hfbusclken0.write(|reg| reg.gpio().set_bit());
    cmu.hfperclken0.write(|reg| reg.timer0().set_bit());

    gpio.ph_dout.reset();
    gpio.ph_modeh.write(|reg| {
        reg.mode10().wiredand();
        reg.mode11().wiredand();
        reg.mode12().wiredand();
        reg.mode13().wiredand();
        reg.mode14().wiredand();
        reg.mode15().wiredand();
        reg
    });

    timer.cmd.write(|reg| reg.stop().set_bit());
    timer.cnt.reset();
    timer.ctrl.write(|reg| {
        reg.presc().div1024();
        reg.clksel().preschfperclk();
        reg.falla().none();
        reg.risea().none();
        reg.mode().up();
        reg.debugrun().clear_bit();
        reg.dmaclract().clear_bit();
        reg.qdm().clear_bit();
        reg.osmen().clear_bit();
        reg.x2cnt().clear_bit();
        reg.ati().clear_bit();
        reg
    });
    timer
        .top
        .write(|reg| unsafe { reg.top().bits(50_000_000 / (1024 * 10)) });
    timer.ifc.write(|reg| {
        reg.of().set_bit();
        reg.uf().set_bit();
        reg.dirchg().set_bit();
        reg.cc0().set_bit();
        reg.cc1().set_bit();
        reg.cc2().set_bit();
        reg.cc3().set_bit();
        reg.icbof0().set_bit();
        reg.icbof1().set_bit();
        reg.icbof2().set_bit();
        reg.icbof3().set_bit();
        reg
    });
    timer.ien.write(|reg| reg.of().set_bit());

    nvic.clear_pending(efm32gg11b820::Interrupt::TIMER0);
    nvic.enable(efm32gg11b820::Interrupt::TIMER0);

    timer.cmd.write(|reg| reg.start().set_bit());

    loop {
        asm::wfe();
    }
}

fn isr_timer0() {
    interrupt::free(|_| unsafe {
        (*efm32gg11b820::TIMER0::ptr())
            .ifc
            .write(|reg| reg.of().set_bit());
        (*efm32gg11b820::GPIO::ptr()).ph_dout.modify(|read, write| {
            write
                .dout()
                .bits((((read.dout().bits() >> 10) + 0x0B) & 0x3F) << 10)
        });
    })
}

interrupt!(TIMER0, isr_timer0);

// Light up both LEDs yellow, trigger a breakpoint, and loop
#[lang = "panic_fmt"]
#[no_mangle]
pub fn rust_begin_panic(_msg: core::fmt::Arguments, _file: &'static str) -> ! {
    interrupt::disable();

    unsafe {
        (*efm32gg11b820::GPIO::ptr()).ph_dout.modify(|read, write| {
            write
                .dout()
                .bits((read.dout().bits() & !(0x3F << 10)) | (0x24 << 10))
        })
    };

    asm::bkpt();
    loop {}
}

// Light up both LEDs red, trigger a breakpoint, and loop
default_handler!(ex_default);
fn ex_default() {
    interrupt::disable();

    unsafe {
        (*efm32gg11b820::GPIO::ptr()).ph_dout.modify(|read, write| {
            write
                .dout()
                .bits((read.dout().bits() & !(0x3F << 10)) | (0x36 << 10))
        })
    };

    asm::bkpt();
    loop {}
}
