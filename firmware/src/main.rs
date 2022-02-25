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

use cortex_m::{asm, interrupt, peripheral};
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{pins, EFM32Pin, GPIOExt, Output};
use led::rgb::{self, Color};
use led::LED;
use smoltcp::time::Instant;

type LED0 = rgb::CommonAnodeLED<pins::PH10<Output>, pins::PH11<Output>, pins::PH12<Output>>;
type LED1 = rgb::CommonAnodeLED<pins::PH13<Output>, pins::PH14<Output>, pins::PH15<Output>>;

#[rtic::app(
    dispatchers = [ CAN0, CAN1 ],
    device = efm32gg11b820,
    peripherals = true,
)]
mod app {
    use crate::efm32gg::{self, dma};
    use crate::ksz8091::KSZ8091;
    use crate::network;

    use core::pin::Pin;
    use cortex_m::{delay::Delay, interrupt};
    use efm32gg_hal::cmu::CMUExt;
    use efm32gg_hal::gpio::{EFM32Pin, GPIOExt};
    use ignore_result::Ignore;
    use led::rgb::{self, Color};
    use led::LED;
    use smoltcp::iface::{InterfaceBuilder, Neighbor, NeighborCache, Route, Routes, SocketStorage};
    use smoltcp::socket::{Dhcpv4Socket, TcpSocket, TcpSocketBuffer};
    use smoltcp::time::{Duration, Instant};
    use smoltcp::wire::{IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

    #[monotonic(binds = SysTick, default = true)]
    type Monotonic = dwt_systick_monotonic::DwtSystick<50_000_000>; // 50 MHz

    #[shared]
    struct SharedResources {
        led0: crate::LED0,
        led1: crate::LED1,
        network: network::Resources,
        rtc: efm32gg11b820::RTC,
    }

    #[local]
    struct LocalResources {
        spawn_handle: Option<handle_network::SpawnHandle>,
    }

    #[cfg(feature = "logging")]
    type Logger = cortex_m_log::log::Logger<cortex_m_log::printer::itm::InterruptSync>;
    #[cfg(feature = "logging")]
    static mut LOGGER: core::mem::MaybeUninit<Logger> = core::mem::MaybeUninit::uninit();

    #[init(
        local = [
             eth_rx_region: dma::RxRegion = dma::RxRegion([0; 1536]),
             eth_tx_region: dma::TxRegion = dma::TxRegion([0; 1536]),
             eth_rx_descriptors: dma::RxDescriptors = dma::RxDescriptors::new(),
             eth_tx_descriptors: dma::TxDescriptors = dma::TxDescriptors::new(),
             tcp_rx_payload: [u8; 1024] = [0; 1024],
             tcp_tx_payload: [u8; 1024] = [0; 1024],

             neighbors: [Option<(IpAddress, Neighbor)>; 8] = [None; 8],
             sockets: [SocketStorage<'static>; 2] = [SocketStorage::EMPTY; 2],
             ip_addresses: [IpCidr; 1] =
                [IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0))],
            routes: [Option<(IpCidr, Route)>; 1] = [None; 1],
        ]
    )]
    fn init(mut cx: init::Context) -> (SharedResources, LocalResources, init::Monotonics) {
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

        // Enable the TRNG and generate a random seed
        let seed = {
            let cmu = &cx.device.CMU;
            let trng = &cx.device.TRNG0;

            cmu.hfperclken0.modify(|_, reg| reg.trng0().set_bit());
            trng.control.modify(|_, reg| reg.enable().set_bit());

            while trng.fifolevel.read().bits() < 2 {}
            let seed =
                u64::from(trng.fifo.read().bits()) << 32 | u64::from(trng.fifo.read().bits());

            trng.control.modify(|_, reg| reg.enable().clear_bit());

            seed
        };

        let mut gpio_clk = cx.device.CMU.constrain().split().gpio;
        gpio_clk.enable();

        // TODO: Move into efm32gg-hal.
        // Configure PG15 as an input with pull-up, and enable interrupts on the falling edge. This is connected
        // to INTRP on the PHY.
        cx.device.GPIO.pg_modeh.modify(|_, w| w.mode15().input());
        cx.device
            .GPIO
            .extipselh
            .modify(|_, w| w.extipsel15().portg());
        cx.device
            .GPIO
            .extipinselh
            .modify(|_, w| w.extipinsel15().pin15());
        cx.device
            .GPIO
            .extifall
            .modify(|_, w| unsafe { w.extifall().bits(1 << 15) });
        cx.device
            .GPIO
            .ifc
            .write(|w| unsafe { w.ext().bits(1 << 15) });
        efm32gg11b820::NVIC::unpend(efm32gg11b820::Interrupt::GPIO_ODD);
        cx.device
            .GPIO
            .ien
            .write(|w| unsafe { w.ext().bits(1 << 15) });

        let gpio = cx.device.GPIO.split(gpio_clk);
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

        #[cfg(feature = "logging")]
        {
            use cortex_m_log::destination::Itm;
            use cortex_m_log::printer::itm::InterruptSync;

            let logger = Logger {
                inner: InterruptSync::new(Itm::new(cx.core.ITM)),
                level: log::LevelFilter::Info,
            };

            unsafe {
                LOGGER = core::mem::MaybeUninit::new(logger);
                cortex_m_log::log::trick_init(LOGGER.assume_init_ref()).unwrap();
            }

            log::info!("Logger online!");
        };

        let mut delay = Delay::new(cx.core.SYST, 50_000_000);
        let (mac_phy, mac_addr) = efm32gg::EFM32GG::new(
            dma::RxBuffer::new(
                Pin::new(cx.local.eth_rx_region),
                Pin::new(cx.local.eth_rx_descriptors),
            ),
            dma::TxBuffer::new(
                Pin::new(cx.local.eth_tx_region),
                Pin::new(cx.local.eth_tx_descriptors),
            ),
            cx.device.ETH,
            &mut delay,
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
        .expect("unable to create MAC/PHY");

        let mut interface = InterfaceBuilder::new(mac_phy, cx.local.sockets.as_mut())
            .hardware_addr(mac_addr.into())
            .neighbor_cache(NeighborCache::new(cx.local.neighbors.as_mut()))
            .ip_addrs(cx.local.ip_addresses.as_mut())
            .routes(Routes::new(cx.local.routes.as_mut()))
            .random_seed(seed)
            .finalize();

        let tcp_handle = interface.add_socket(TcpSocket::new(
            TcpSocketBuffer::new(cx.local.tcp_rx_payload.as_mut()),
            TcpSocketBuffer::new(cx.local.tcp_tx_payload.as_mut()),
        ));

        let mut dhcp_socket = Dhcpv4Socket::new();
        // XXX: just for testing
        dhcp_socket.set_max_lease_duration(Some(Duration::from_secs(60)));
        let dhcp_handle = interface.add_socket(dhcp_socket);

        let syst = delay.free();
        (
            SharedResources {
                led0,
                led1,
                network: network::Resources {
                    interface,
                    tcp_handle,
                    dhcp_handle,
                },
                rtc: cx.device.RTC,
            },
            LocalResources { spawn_handle: None },
            init::Monotonics(Monotonic::new(
                &mut cx.core.DCB,
                cx.core.DWT,
                syst,
                50_000_000,
            )),
        )
    }

    #[task(capacity = 2, local = [spawn_handle], shared = [network, rtc])]
    fn handle_network(mut cx: handle_network::Context) {
        log::trace!("Handling network...");

        let timestamp = Instant::from_millis(cx.shared.rtc.lock(|rtc| rtc.cnt.read().cnt().bits()));
        let spawn_handle = cx.local.spawn_handle;
        let mut network = cx.shared.network;

        match network.lock(|network| network.interface.poll(timestamp)) {
            Ok(true) => {
                log::trace!("Handling sockets...");

                network.lock(|network| network.handle_sockets());
            }
            Ok(false) => log::trace!("Nothing to do"),
            Err(err) => log::error!("Failed to poll network interface: {}", err),
        }

        if let Some(delay) = network.lock(|network| network.interface.poll_delay(timestamp)) {
            use dwt_systick_monotonic::fugit::ExtU32;
            log::trace!("Scheduling network handling in {}", delay);

            let delay = (delay.total_millis() as u32).millis();
            *spawn_handle = spawn_handle
                .take()
                .and_then(|h| h.reschedule_after(delay).ok())
                .or_else(|| {
                    Some(handle_network::spawn_after(delay).expect("spawning handle_network"))
                });
        }

        log::trace!("Handled sockets: {}", timestamp);
    }

    #[task(binds = ETH, shared = [network, led0, led1])]
    fn eth_irq(cx: eth_irq::Context) {
        let eth_irq::SharedResources {
            mut network,
            mut led0,
            mut led1,
        } = cx.shared;

        interrupt::free(|_| {
            network.lock(|network| {
                led0.lock(|led0| {
                    led1.lock(|led1| {
                        network.interface.device_mut().mac_irq(led0, led1);
                    })
                })
            })
        });

        handle_network::spawn().ignore();
    }

    #[task(binds = GPIO_ODD, shared = [network])]
    fn gpio_odd_irq(mut cx: gpio_odd_irq::Context) {
        use dwt_systick_monotonic::fugit::ExtU32;

        (unsafe { &*efm32gg11b820::GPIO::ptr() })
            .ifc
            .write(|w| unsafe { w.ext().bits(1 << 15) });

        cx.shared.network.lock(|network| {
            network.interface.device_mut().phy_irq();
        });

        // TODO: Why is the one-second delay necessary? 100 ms doesn't work.
        handle_network::spawn_after(1000u32.millis()).ignore()
    }
}

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

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let rtc = unsafe { &*efm32gg11b820::RTC::ptr() };
    let itm = unsafe { &mut *cortex_m::peripheral::ITM::ptr() };

    cortex_m::interrupt::disable();

    let now = Instant::from_millis(rtc.cnt.read().cnt().bits());
    let stim = &mut itm.stim[0];

    log::error!("Panic at {}", now);
    cortex_m::iprintln!(stim, "{}", info);

    if cortex_m::peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }

    loop {}
}
