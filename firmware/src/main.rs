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

#![no_main]
#![no_std]

extern crate panic_itm;

mod efm32gg;
mod ksz8091;
mod mac;
mod phy;

use crate::efm32gg::dma;
use crate::ksz8091::KSZ8091;
use core::fmt::Write;
use cortex_m::{asm, peripheral};
use efm32gg11b820::interrupt;
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{pins, EFM32Pin, GPIOExt, Output};
use led::rgb::{self, Color};
use led::LED;
use smoltcp::iface::{InterfaceBuilder, NeighborCache};
use smoltcp::socket::{SocketSet, TcpSocket, TcpSocketBuffer};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr};

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut rx_region = dma::RxRegion([0; 1536]);
    let mut tx_region = dma::TxRegion([0; 1536]);
    let mut rx_descriptors = dma::RxDescriptors::new();
    let mut tx_descriptors = dma::TxDescriptors::new();
    let rx_buffer = dma::RxBuffer::new(&mut rx_region, &mut rx_descriptors);
    let tx_buffer = dma::TxBuffer::new(&mut tx_region, &mut tx_descriptors);

    let peripherals = efm32gg11b820::Peripherals::take().unwrap();
    let cmu = peripherals.CMU;
    let eth = peripherals.ETH;
    let gpio = peripherals.GPIO;
    let msc = peripherals.MSC;
    let rtc = peripherals.RTC;

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

    // Switch to high frequency oscillator
    cmu.hfclksel.write(|reg| reg.hf().hfxo());

    // Use the high frequency clock for the ITM
    cmu.dbgclksel.write(|reg| reg.dbg().hfclk());

    // Update the EMU configuration
    let _ = cmu.status.read().bits();

    // Enable the GPIOs and the low energy peripheral interface
    cmu.hfbusclken0.write(|reg| {
        reg.gpio().set_bit();
        reg.le().set_bit();
        reg
    });

    // Enable the RTC and set it to 1000Hz
    cmu.lfaclksel.write(|reg| reg.lfa().ulfrco());
    cmu.lfaclken0.write(|reg| reg.rtc().set_bit());
    rtc.ctrl.write(|reg| reg.en().set_bit());

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

    gpio.routepen.write(|reg| reg.swvpen().set_bit());
    gpio.routeloc0.write(|reg| reg.swvloc().loc0());
    gpio.pf_model.write(|reg| reg.mode2().pushpull());

    #[cfg(feature = "logging")]
    let _logger = {
        use cortex_m_log::destination::Itm;
        use cortex_m_log::log::Logger;
        use cortex_m_log::printer::itm::InterruptSync;

        let itm = cortex_m::peripheral::Peripherals::take().unwrap().ITM;

        let logger = Logger::<InterruptSync> {
            inner: InterruptSync::new(Itm::new(itm)),
            level: log::LevelFilter::Trace,
        };
        unsafe { cortex_m_log::log::trick_init(&logger) }.unwrap();
        log::debug!("Logger online!");

        logger
    };

    let ethernet_addr = EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]);
    let mut neighbor_cache = [None; 8];
    let mut ip_addrs = [IpCidr::new(IpAddress::v4(10, 1, 0, 3), 24)];

    let mut iface = InterfaceBuilder::new(
        efm32gg::EFM32GG::new(rx_buffer, tx_buffer, eth, &cmu, &gpio, KSZ8091::new)
            .expect("unable to create MACPHY"),
    )
    .ethernet_addr(ethernet_addr)
    .neighbor_cache(NeighborCache::new(neighbor_cache.as_mut()))
    .ip_addrs(ip_addrs.as_mut())
    .finalize();

    let mut tcp_rx_payload = [0; 128];
    let mut tcp_tx_payload = [0; 128];
    let tcp_socket = TcpSocket::new(
        TcpSocketBuffer::new(tcp_rx_payload.as_mut()),
        TcpSocketBuffer::new(tcp_tx_payload.as_mut()),
    );

    let mut socket_array = [None];
    let mut sockets = SocketSet::new(socket_array.as_mut());
    let tcp_handle = sockets.add(tcp_socket);

    loop {
        log::trace!("WFE");
        asm::wfe();

        let timestamp = Instant::from_millis(rtc.cnt.read().cnt().bits());
        match iface.poll(&mut sockets, timestamp) {
            Ok(_) => {}
            Err(err) => log::error!("Failed to poll: {}", err),
        }

        {
            let mut socket = sockets.get::<TcpSocket>(tcp_handle);
            if !socket.is_open() {
                socket.listen(6969).unwrap();
            }

            if socket.can_send() {
                log::debug!("tcp:6969 send greeting");
                writeln!(socket, "hello").unwrap();
                log::debug!("tcp:6969 close");
                socket.close();
            }
        }
    }
}

#[efm32gg11b820::interrupt]
fn ETH() {
    efm32gg::isr()
}

// Light up both LEDs red, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn DefaultHandler(irqn: i16) {
    cortex_m::interrupt::disable();

    log::error!("Default Handler: irq {}", irqn);
    let (mut led0, mut led1) = unsafe { steal_leds() };
    led0.set(Color::Red);
    led1.set(Color::Red);

    if peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }

    loop {
        asm::wfe();
    }
}

// Light up both LEDs red, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn HardFault(_frame: &cortex_m_rt::ExceptionFrame) -> ! {
    cortex_m::interrupt::disable();

    let (mut led0, mut led1) = unsafe { steal_leds() };
    led0.set(Color::Red);
    led1.set(Color::Red);

    if peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }

    loop {
        asm::wfe();
    }
}

type LED0 = rgb::CommonAnodeLED<pins::PH10<Output>, pins::PH11<Output>, pins::PH12<Output>>;
type LED1 = rgb::CommonAnodeLED<pins::PH13<Output>, pins::PH14<Output>, pins::PH15<Output>>;

unsafe fn steal_leds() -> (LED0, LED1) {
    let gpio = (&*efm32gg11b820::GPIO::ptr())
        .split((&*efm32gg11b820::CMU::ptr()).constrain().split().gpio);
    let led0 = rgb::CommonAnodeLED::new(
        gpio.ph10.as_opendrain(),
        gpio.ph11.as_opendrain(),
        gpio.ph12.as_opendrain(),
    );
    let led1 = rgb::CommonAnodeLED::new(
        gpio.ph13.as_opendrain(),
        gpio.ph14.as_opendrain(),
        gpio.ph15.as_opendrain(),
    );

    (led0, led1)
}
