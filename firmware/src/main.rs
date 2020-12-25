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

#![warn(clippy::all)]
#![no_main]
#![no_std]

mod efm32gg;
mod ksz8091;
mod mac;
mod network;
mod phy;
#[cfg(feature = "logging")]
mod semihosting;

use crate::efm32gg::dma;
use crate::ksz8091::KSZ8091;
use core::convert::TryInto;
use core::fmt::Write;
use core::panic::PanicInfo;
use cortex_m::{asm, peripheral};
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{EFM32Pin, GPIOExt};
use led::rgb::{self, Color};
use led::LED;
use smoltcp::iface::{EthernetInterfaceBuilder, NeighborCache};
use smoltcp::socket::{SocketSet, TcpSocket, TcpSocketBuffer};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr};

#[cfg(feature = "logging")]
static LOGGER: semihosting::Logger = semihosting::Logger;

// TODO: Implement rtfm::Monotonic with RTC
#[rtfm::app(device = efm32gg11b820, peripherals = true, monotonic = rtfm::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        network: network::Resources,
        rtc: efm32gg11b820::RTC,
    }

    #[init(spawn = [poll_interface])]
    fn init(ctx: init::Context) -> init::LateResources {
        let cmu = ctx.device.CMU;
        let gpio = ctx.device.GPIO;
        let msc = ctx.device.MSC;
        let rtc = ctx.device.RTC;

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

        #[cfg(feature = "logging")]
        {
            log::set_logger(&LOGGER).unwrap();
            log::set_max_level(log::LevelFilter::Warn);
        }

        let mut network = network::ResourceBuilder::new(dma::RxBuffer::new(), dma::TxBuffer::new());
        
        let ethernet_addr = EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]);
        let mut ip_addrs = [IpCidr::new(IpAddress::v4(10, 43, 128, 5), 24)];

        let network = network.add_iface(
            EthernetInterfaceBuilder::new(
                efm32gg::EFM32GG::create(
                    &mut network.rx_buffer,
                    &mut network.tx_buffer,
                    ctx.device.ETH,
                    &cmu,
                    &gpio,
                    KSZ8091::new,
                )
                .expect("unable to create MACPHY"),
            )
            .ethernet_addr(ethernet_addr)
            .neighbor_cache(NeighborCache::new(network.neighbor_cache.as_mut()))
            .ip_addrs(ip_addrs.as_mut())
            .finalize(),
        );

        let tcp_socket = TcpSocket::new(
            TcpSocketBuffer::new(network.inner.tcp_rx_payload.as_mut()),
            TcpSocketBuffer::new(network.inner.tcp_tx_payload.as_mut()),
        );

        let mut socket_array = [None];
        let network = network.add_sockets(SocketSet::new(socket_array.as_mut()));

        let network = network.add_tcp_handle(network.sockets.add(tcp_socket));

        ctx.spawn.poll_interface().unwrap();
        init::LateResources { rtc, network }
    }

    #[task(schedule = [poll_interface], resources = [network, rtc])]
    fn poll_interface(ctx: poll_interface::Context) {
        let now = Instant::from_millis(ctx.resources.rtc.cnt.read().cnt().bits());
        if let Err(err) = ctx
            .resources
            .network
            .iface
            .poll(&mut ctx.resources.network.sockets, now)
        {
            log::error!("Failed to poll: {}", err)
        };

        {
            let mut socket = ctx
                .resources
                .network
                .sockets
                .get::<TcpSocket>(ctx.resources.network.tcp_handle);
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

        if let Some(delay) = ctx
            .resources
            .network
            .iface
            .poll_delay(&mut ctx.resources.network.sockets, now)
        {
            // TODO: Millis to cycles? There's got to be a cleaner way.
            ctx.schedule
                .poll_interface(
                    rtfm::cyccnt::Instant::now()
                        + rtfm::cyccnt::Duration::from_cycles(
                            delay.total_millis().try_into().unwrap(),
                        ),
                )
                .unwrap()
        }
    }

    #[task(binds = ETH)]
    fn eth(_: eth::Context) {
        efm32gg::isr()
    }

    extern "C" {
        fn CAN0();
        fn CAN1();
    }
};

// Light up both LEDs yellow, trigger a breakpoint, and loop
#[panic_handler]
pub fn panic(_info: &PanicInfo) -> ! {
    cortex_m::interrupt::disable();

    let gpio = (unsafe { &*efm32gg11b820::GPIO::ptr() }).split(
        (unsafe { &*efm32gg11b820::CMU::ptr() })
            .constrain()
            .split()
            .gpio,
    );
    let mut led0 = rgb::CommonAnodeLED::new(
        gpio.ph10.as_output(),
        gpio.ph11.as_output(),
        gpio.ph12.as_output(),
    );
    let mut led1 = rgb::CommonAnodeLED::new(
        gpio.ph13.as_output(),
        gpio.ph14.as_output(),
        gpio.ph15.as_output(),
    );

    led0.set(Color::Yellow);
    led1.set(Color::Yellow);

    if unsafe { (*peripheral::DCB::ptr()).dhcsr.read() & 0x0000_0001 } != 0 {
        asm::bkpt();
    }
    loop {
        asm::wfe();
    }
}

// Light up both LEDs red, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn DefaultHandler(_irqn: i16) {
    cortex_m::interrupt::disable();

    let gpio = (unsafe { &*efm32gg11b820::GPIO::ptr() }).split(
        (unsafe { &*efm32gg11b820::CMU::ptr() })
            .constrain()
            .split()
            .gpio,
    );
    let mut led0 = rgb::CommonAnodeLED::new(
        gpio.ph10.as_output(),
        gpio.ph11.as_output(),
        gpio.ph12.as_output(),
    );
    let mut led1 = rgb::CommonAnodeLED::new(
        gpio.ph13.as_output(),
        gpio.ph14.as_output(),
        gpio.ph15.as_output(),
    );

    led0.set(Color::Red);
    led1.set(Color::Red);

    if unsafe { (*peripheral::DCB::ptr()).dhcsr.read() & 0x0000_0001 } != 0 {
        asm::bkpt();
    }
    loop {
        asm::wfe();
    }
}

#[cortex_m_rt::exception]
fn HardFault(_frame: &cortex_m_rt::ExceptionFrame) -> ! {
    cortex_m::interrupt::disable();

    let gpio = (unsafe { &*efm32gg11b820::GPIO::ptr() }).split(
        (unsafe { &*efm32gg11b820::CMU::ptr() })
            .constrain()
            .split()
            .gpio,
    );
    let mut led0 = rgb::CommonAnodeLED::new(
        gpio.ph10.as_output(),
        gpio.ph11.as_output(),
        gpio.ph12.as_output(),
    );
    let mut led1 = rgb::CommonAnodeLED::new(
        gpio.ph13.as_output(),
        gpio.ph14.as_output(),
        gpio.ph15.as_output(),
    );

    led0.set(Color::Red);
    led1.set(Color::Red);

    if unsafe { (*peripheral::DCB::ptr()).dhcsr.read() & 0x0000_0001 } != 0 {
        asm::bkpt();
    }
    loop {
        asm::wfe();
    }
}
