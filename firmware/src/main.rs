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

mod efm32gg;
mod ksz8091;
mod mac;
mod network;
mod phy;

use crate::efm32gg::dma;
use crate::ksz8091::KSZ8091;
use core::fmt::Write;
use core::pin::Pin;
use cortex_m::{asm, interrupt, peripheral};
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{pins, EFM32Pin, GPIOExt, Output};
use ignore_result::Ignore;
use led::rgb::{self, Color};
use led::LED;
use panic_itm as _;
use rtic::cyccnt::U32Ext;
use smoltcp::iface::{InterfaceBuilder, Neighbor, NeighborCache};
use smoltcp::socket::{SocketSet, SocketSetItem, TcpSocket, TcpSocketBuffer};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

type LED0 = rgb::CommonAnodeLED<pins::PH10<Output>, pins::PH11<Output>, pins::PH12<Output>>;
type LED1 = rgb::CommonAnodeLED<pins::PH13<Output>, pins::PH14<Output>, pins::PH15<Output>>;

#[cfg(feature = "logging")]
type Logger = cortex_m_log::log::Logger<cortex_m_log::printer::itm::InterruptSync>;

#[rtic::app(device = efm32gg11b820, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        led0: LED0,
        led1: LED1,
        #[cfg(feature = "logging")]
        logger: Logger,
        network: network::Resources,
        rtc: efm32gg11b820::RTC,
    }

    #[init]
    fn init(mut cx: init::Context) -> init::LateResources {
        static mut ETH_RX_REGION: dma::RxRegion = dma::RxRegion([0; 1536]);
        static mut ETH_TX_REGION: dma::TxRegion = dma::TxRegion([0; 1536]);
        static mut ETH_RX_DESCRIPTORS: dma::RxDescriptors = dma::RxDescriptors::new();
        static mut ETH_TX_DESCRIPTORS: dma::TxDescriptors = dma::TxDescriptors::new();
        static mut TCP_RX_PAYLOAD: [u8; 128] = [0; 128];
        static mut TCP_TX_PAYLOAD: [u8; 128] = [0; 128];

        static mut NEIGHBORS: [Option<(IpAddress, Neighbor)>; 8] = [None; 8];
        static mut SOCKETS: [Option<SocketSetItem<'static>>; 1] = [None; 1];
        static mut IP_ADDRESSES: [IpCidr; 1] = [IpCidr::Ipv4(Ipv4Cidr::new(
            Ipv4Address::new(192, 168, 0, 3),
            24,
        ))];

        // Enable the HFXO
        cx.device.CMU.oscencmd.write(|reg| reg.hfxoen().set_bit());
        // Wait for HFX0 to stabilize
        while cx.device.CMU.status.read().hfxordy().bit_is_clear() {}

        // Update the EMU configuration
        let _ = cx.device.CMU.status.read().bits();

        // Allow access to low energy peripherals with a clock speed greater than 50MHz
        cx.device.CMU.ctrl.write(|reg| reg.wshfle().set_bit());

        // Set the appropriate read delay for flash
        cx.device.MSC.readctrl.write(|reg| reg.mode().ws2());

        // Switch to high frequency oscillator
        cx.device.CMU.hfclksel.write(|reg| reg.hf().hfxo());

        // Use the high frequency clock for the ITM
        cx.device.CMU.dbgclksel.write(|reg| reg.dbg().hfclk());

        // Update the EMU configuration
        let _ = cx.device.CMU.status.read().bits();

        // Enable the GPIOs and the low energy peripheral interface
        cx.device.CMU.hfbusclken0.write(|reg| {
            // TODO: Gets set by cx.device.GPIO.split(), but is needed because of the write to routepen before.
            reg.gpio().set_bit();
            reg.le().set_bit();
            reg
        });

        // Enable the Serial Wire Viewer (ITM on SWO)
        cx.device.GPIO.routepen.write(|reg| reg.swvpen().set_bit());

        // Enable the RTC and set it to 1000Hz
        cx.device.CMU.lfaclksel.write(|reg| reg.lfa().ulfrco());
        cx.device.CMU.lfaclken0.write(|reg| reg.rtc().set_bit());
        cx.device.RTC.ctrl.write(|reg| reg.en().set_bit());

        // Enable the DWT (needed by monotonic timer)
        cx.core.DWT.enable_cycle_counter();
        cx.core.DCB.enable_trace();

        let gpio = cx.device.GPIO.split(cx.device.CMU.constrain().split().gpio);
        let _swo = gpio.pf2.as_output();

        let mut led0 = rgb::CommonAnodeLED::new(
            gpio.ph10.as_opendrain(),
            gpio.ph11.as_opendrain(),
            gpio.ph12.as_opendrain(),
        );

        let mut led1 = rgb::CommonAnodeLED::new(
            gpio.ph13.as_opendrain(),
            gpio.ph14.as_opendrain(),
            gpio.ph15.as_opendrain(),
        );

        led0.set(Color::Black);
        led1.set(Color::Black);

        let interface = InterfaceBuilder::new(
            efm32gg::EFM32GG::new(
                dma::RxBuffer::new(Pin::new(ETH_RX_REGION), Pin::new(ETH_RX_DESCRIPTORS)),
                dma::TxBuffer::new(Pin::new(ETH_TX_REGION), Pin::new(ETH_TX_DESCRIPTORS)),
                cx.device.ETH,
                efm32gg::Pins {
                    rmii_rxd0: gpio.pd9.as_input(),
                    rmii_refclk: gpio.pd10.as_output(),
                    rmii_crsdv: gpio.pd11.as_input(),
                    rmii_rxer: gpio.pd12.as_input(),
                    rmii_mdio: gpio.pd13.as_output(),
                    rmii_mdc: gpio.pd14.as_output(),
                    rmii_txd0: gpio.pf6.as_output(),
                    rmii_txd1: gpio.pf7.as_output(),
                    rmii_txen: gpio.pf8.as_output(),
                    rmii_rxd1: gpio.pf9.as_input(),
                    phy_reset: gpio.ph7.as_output(),
                    phy_enable: gpio.pi10.as_output(),
                },
                KSZ8091::new,
            )
            .expect("unable to create MACPHY"),
        )
        .ethernet_addr(EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]))
        .neighbor_cache(NeighborCache::new(NEIGHBORS.as_mut()))
        .ip_addrs(IP_ADDRESSES.as_mut())
        .finalize();

        #[cfg(feature = "logging")]
        let logger = {
            use cortex_m_log::destination::Itm;
            use cortex_m_log::log::trick_init;
            use cortex_m_log::printer::itm::InterruptSync;

            let logger = Logger {
                inner: InterruptSync::new(Itm::new(cx.core.ITM)),
                level: log::LevelFilter::Debug,
            };

            unsafe { trick_init(&logger) }.unwrap();
            log::debug!("Logger online!");

            logger
        };

        let mut sockets = SocketSet::new(SOCKETS.as_mut());
        let tcp_handle = sockets.add(TcpSocket::new(
            TcpSocketBuffer::new(TCP_RX_PAYLOAD.as_mut()),
            TcpSocketBuffer::new(TCP_TX_PAYLOAD.as_mut()),
        ));

        init::LateResources {
            led0,
            led1,
            #[cfg(feature = "logging")]
            logger,
            network: network::Resources {
                interface,
                sockets,
                tcp_handle,
            },
            rtc: cx.device.RTC,
        }
    }

    #[task(resources = [network, rtc], schedule = [handle_network])]
    fn handle_network(cx: handle_network::Context) {
        log::trace!("Handling network...");

        let network::Resources {
            interface,
            sockets,
            tcp_handle,
        } = cx.resources.network;

        let timestamp = Instant::from_millis(cx.resources.rtc.cnt.read().cnt().bits());
        match interface.poll(sockets, timestamp) {
            Ok(false) => log::trace!("Nothing to do"),
            Ok(true) => {
                log::trace!("Handling sockets...");

                let mut socket = sockets.get::<TcpSocket>(*tcp_handle);
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
            Err(err) => log::error!("Failed to poll network interface: {}", err),
        }

        if let Some(delay) = interface.poll_delay(sockets, timestamp) {
            cx.schedule
                // TODO: 80_000 cycles shouldn't be hard-coded (I'm not sure if it's right either)
                .handle_network(cx.scheduled + (delay.millis as u32 * 80_000).cycles())
                .ignore();

            log::trace!("Scheduled network handling in {}", delay);
        }
        log::trace!("Finished handling network");
    }

    #[task(binds = ETH, resources = [network, led0, led1], spawn = [handle_network])]
    fn eth_irq(cx: eth_irq::Context) {
        let eth_irq::Resources {
            led0,
            led1,
            network,
        } = cx.resources;

        log::trace!("Interrupt - ETH");

        interrupt::free(|_| network.interface.device_mut().irq(led0, led1));

        cx.spawn.handle_network().ignore();
    }

    extern "C" {
        fn CAN0();
        fn CAN1();
    }
};

// Light up both LEDs red, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn DefaultHandler(irqn: i16) {
    interrupt::disable();

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
    interrupt::disable();

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

/// Steals the LEDs so they may be used directly.
///
/// # Safety
///
/// This overrides any existing configuration.
pub unsafe fn steal_leds() -> (LED0, LED1) {
    let periph = efm32gg11b820::Peripherals::steal();
    let gpio = periph.GPIO.split(periph.CMU.constrain().split().gpio);

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
