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

#![feature(attr_literals, lang_items)]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_semihosting;
#[macro_use]
extern crate efm32gg11b820;
extern crate smoltcp;

mod efm32gg;

use cortex_m::{asm, interrupt};

static mut MAC: Option<efm32gg::MAC> = None;
static mut RX_BUFFER: [u8; 2048] = [0; 2048];
static mut TX_BUFFER: [u8; 2048] = [0; 2048];

fn main() {
    let peripherals = efm32gg11b820::Peripherals::take().unwrap();
    let cmu = peripherals.CMU;
    let eth = peripherals.ETH;
    let gpio = peripherals.GPIO;
    let msc = peripherals.MSC;
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
    gpio.ph_dout
        .write(|reg| unsafe { reg.dout().bits(0x3F << 10) });
    gpio.ph_modeh.write(|reg| {
        reg.mode10().wiredand();
        reg.mode11().wiredand();
        reg.mode12().wiredand();
        reg.mode13().wiredand();
        reg.mode14().wiredand();
        reg.mode15().wiredand();
        reg
    });

    let mac = efm32gg::MAC::new(
        efm32gg::Buffer::new(unsafe { &mut RX_BUFFER }),
        efm32gg::Buffer::new(unsafe { &mut TX_BUFFER }),
    );

    unsafe {
        MAC = Some(mac);
        MAC.as_mut()
            .unwrap()
            .configure(&eth, &cmu, &gpio, &mut nvic);
    }

    loop {
        asm::wfe();
        unsafe { MAC.as_mut().unwrap().process() }
    }
}

interrupt!(ETH, isr_eth);
fn isr_eth() {
    unsafe { MAC.as_mut() }.unwrap().isr()
}

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
