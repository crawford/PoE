// Copyright 2018 Alex Crawford
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

#![no_main]
#![no_std]

/// Sandbox for development on the SLSTK3701A dev board
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{pins, EFM32Pin, GPIOExt, Output};
use ignore_result::Ignore;
use led::rgb::{self, Color};
use poe::fault;

type LED0 = rgb::CommonAnodeLED<pins::PH10<Output>, pins::PH11<Output>, pins::PH12<Output>, ()>;
type LED1 = rgb::CommonAnodeLED<pins::PH13<Output>, pins::PH14<Output>, pins::PH15<Output>, ()>;

#[rtic::app(
    dispatchers = [ CAN0, CAN1 ],
    device = efm32gg11b820,
    peripherals = true,
)]
mod app {
    #[cfg(feature = "telnet")]
    use poe::command::{Interpreter, InterpreterMode};
    use poe::efm32gg::{self, dma};
    use poe::ksz8091::KSZ8091;
    use poe::network;

    use core::pin::Pin;
    use cortex_m::{delay::Delay, interrupt};
    use dwt_systick_monotonic::ExtU32;
    use efm32gg_hal::cmu::CMUExt;
    use efm32gg_hal::gpio::{EFM32Pin, GPIOExt};
    use embedded_hal::digital::v2::OutputPin;
    use ignore_result::Ignore;
    use led::rgb::{self, Color};
    use smoltcp::iface::{InterfaceBuilder, Neighbor, NeighborCache, Route, Routes, SocketStorage};
    use smoltcp::socket::{Dhcpv4Socket, TcpSocket, TcpSocketBuffer};
    use smoltcp::time::{Duration, Instant};
    use smoltcp::wire::{IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

    #[monotonic(binds = SysTick, default = true)]
    type Monotonic = dwt_systick_monotonic::DwtSystick<50_000_000>; // 50 MHz

    macro_rules! schedule {
        ($name:ident, $duration:expr) => {
            $name::spawn_after($duration).expect(concat!("scheduling ", stringify!($name)))
        };
    }

    #[shared]
    struct SharedResources {
        led0: ErrorLed,
        led1: crate::LED1,
        network: network::Resources,
        rtc: efm32gg11b820::RTC,
    }

    #[local]
    struct LocalResources {
        spawn_handle: Option<handle_network::SpawnHandle>,

        #[cfg(feature = "rtt")]
        terminal: &'static mut poe::log::rtt::Terminal,
    }

    #[init(
        local = [
            eth_rx_region: dma::RxRegion = dma::RxRegion([0; 1536]),
            eth_tx_region: dma::TxRegion = dma::TxRegion([0; 1536]),
            eth_rx_descriptors: dma::RxDescriptors = dma::RxDescriptors::new(),
            eth_tx_descriptors: dma::TxDescriptors = dma::TxDescriptors::new(),
            tcp_rx_payload: [u8; 1024] = [0; 1024],
            tcp_tx_payload: [u8; 1024] = [0; 1024],

            #[cfg(feature = "telnet")]
            telnet_rx_payload: [u8; 1024] = [0; 1024],
            #[cfg(feature = "telnet")]
            telnet_tx_payload: [u8; 1024] = [0; 1024],

            neighbors: [Option<(IpAddress, Neighbor)>; 8] = [None; 8],
            sockets: [SocketStorage<'static>; 4] = [SocketStorage::EMPTY; 4],
            ip_addresses: [IpCidr; 1] =
                [IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0))],
            routes: [Option<(IpCidr, Route)>; 1] = [None; 1],
        ]
    )]
    fn init(mut cx: init::Context) -> (SharedResources, LocalResources, init::Monotonics) {
        // Initialize logging
        let logger = poe::log::init();
        #[cfg(feature = "rtt")]
        logger.add_rtt(poe::log::rtt::new(log::LevelFilter::Debug));

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

        // Now that the GPIOs have been configured, enable ITM logging
        #[cfg(feature = "itm")]
        logger.add_itm(poe::log::itm::new(
            log::LevelFilter::Info,
            &cx.device.CMU,
            &cx.device.GPIO,
            cx.core.ITM,
        ));

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
        // Configure PG15 as an input and enable interrupts on the falling edge. This is connected
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

        led0.set(Color::Black).ignore();
        led1.set(Color::Black).ignore();

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

        // Power up the PHY module
        gpio.pi10.as_output().set_high().ignore();

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
                rmii_rxd0: &mut gpio.pd9.as_input(),
                rmii_refclk: &mut gpio.pd10.as_output(),
                rmii_crsdv: &mut gpio.pd11.as_input(),
                rmii_rxer: &mut gpio.pd12.as_input(),
                rmii_mdio: &mut gpio.pd13.as_output(),
                rmii_mdc: &mut gpio.pd14.as_output(),
                rmii_txd0: &mut gpio.pf6.as_output(),
                rmii_txd1: &mut gpio.pf7.as_output(),
                rmii_txen: &mut gpio.pf8.as_output(),
                rmii_rxd1: &mut gpio.pf9.as_input(),
                phy_reset: &mut gpio.ph7.as_output(),
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

        #[cfg(feature = "telnet")]
        let telnet_handle = interface.add_socket(TcpSocket::new(
            TcpSocketBuffer::new(cx.local.telnet_rx_payload.as_mut()),
            TcpSocketBuffer::new(cx.local.telnet_tx_payload.as_mut()),
        ));

        let mut dhcp_socket = Dhcpv4Socket::new();
        // XXX: just for testing
        dhcp_socket.set_max_lease_duration(Some(Duration::from_secs(60)));
        let dhcp_handle = interface.add_socket(dhcp_socket);

        let mut led_err = ErrorLed::new(led0);
        led_err.show(network::State::NoLink);

        #[cfg(feature = "rtt")]
        handle_terminal::spawn().expect("spawn handle_terminal");

        let syst = delay.free();
        (
            SharedResources {
                led0: led_err,
                led1,
                network: network::Resources {
                    interface,
                    tcp_handle,
                    dhcp_handle,

                    #[cfg(feature = "telnet")]
                    telnet_handle,

                    #[cfg(feature = "telnet")]
                    interpreter: Interpreter::new(),
                    #[cfg(feature = "telnet")]
                    prev_mode: InterpreterMode::Command,
                },
                rtc: cx.device.RTC,
            },
            LocalResources {
                spawn_handle: None,

                #[cfg(feature = "rtt")]
                terminal: poe::log::rtt::Terminal::new(),
            },
            init::Monotonics(Monotonic::new(
                &mut cx.core.DCB,
                cx.core.DWT,
                syst,
                50_000_000,
            )),
        )
    }

    #[task(capacity = 2, local = [spawn_handle], shared = [led0, led1, network, rtc])]
    fn handle_network(mut cx: handle_network::Context) {
        log::trace!("Handling network...");

        let timestamp = Instant::from_millis(cx.shared.rtc.lock(|rtc| rtc.cnt.read().cnt().bits()));
        let spawn_handle = cx.local.spawn_handle;
        let mut led0 = cx.shared.led0;
        let mut led1 = cx.shared.led1;
        let mut network = cx.shared.network;

        match network.lock(|network| network.interface.poll(timestamp)) {
            Ok(true) => {
                log::trace!("Handling sockets...");

                network.lock(|network| {
                    network.handle_sockets(
                        |state| led0.lock(|led| led.show(state)),
                        |en| match en {
                            false => led1.lock(|led| led.set(Color::Black).ignore()),
                            true => led1.lock(|led| led.set(Color::Yellow).ignore()),
                        },
                    )
                });
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

    pub struct ErrorLed {
        spawn: Option<occult_network_led::SpawnHandle>,
        led: crate::LED0,
        state: rgb::Color,
        network: network::State,
        flashes: u8,
    }

    impl ErrorLed {
        fn new(led: crate::LED0) -> ErrorLed {
            ErrorLed {
                spawn: None,
                led,
                state: rgb::Color::Red,
                network: network::State::Uninit,
                flashes: 0,
            }
        }

        // This can race - link drops (NoLink) and then DHCP is handled (NoDhcp).
        // Break this into two functions that check direction.
        fn show(&mut self, state: network::State) {
            self.network = state;
            self.flashes = 0;

            if let Some(handle) = self.spawn.take() {
                handle.cancel().ignore();
            }
            occult_network_led::spawn().expect("spawning occult_network_led");
        }
    }

    #[task(priority = 8, shared = [led0])]
    fn occult_network_led(mut cx: occult_network_led::Context) {
        use network::State::*;
        use rgb::Color::*;

        cx.shared.led0.lock(|net| {
            let next = match (net.network, net.flashes) {
                (Uninit, _) => Black,
                (Operational, _) => Green,
                (network, 0) => {
                    net.flashes = match network {
                        Uninit | Operational => 0,
                        NoLink => 1,
                        NoDhcp => 2,
                        NoGateway => 3,
                    };
                    net.spawn = Some(schedule!(occult_network_led, 1000u32.millis()));
                    Red
                }
                (_, flashes) => match net.state {
                    Black => {
                        net.flashes = flashes.saturating_sub(1);
                        net.spawn = Some(schedule!(occult_network_led, 250u32.millis()));
                        Red
                    }
                    _ => {
                        net.spawn = Some(schedule!(occult_network_led, 250u32.millis()));
                        Black
                    }
                },
            };
            net.state = next;
            net.led.set(next).ignore()
        });
    }

    #[task(binds = ETH, shared = [network])]
    fn eth_irq(mut cx: eth_irq::Context) {
        interrupt::free(|_| {
            cx.shared.network.lock(|network| {
                network.interface.device_mut().mac_irq();
            });
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

    #[cfg(feature = "rtt")]
    #[task(local = [terminal])]
    fn handle_terminal(cx: handle_terminal::Context) {
        cx.local.terminal.poll();
        handle_terminal::spawn_after(100u32.millis()).expect("schedule handle_terminal");
    }
}

/// Steals the LEDs so they may be used directly.
///
/// # Safety
///
/// This overrides any existing configuration.
unsafe fn steal_leds() -> (LED0, LED1) {
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

#[cortex_m_rt::exception]
fn DefaultHandler(irqn: i16) -> ! {
    fault::handle_default(irqn, |_irqn| {
        let (mut led0, mut led1) = unsafe { steal_leds() };
        led0.set(Color::Red).ignore();
        led1.set(Color::Red).ignore();
    })
}

// Light up both LEDs red, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    fault::handle_hardfault(frame, |_frame| {
        let (mut led0, mut led1) = unsafe { steal_leds() };
        led0.set(Color::Red).ignore();
        led1.set(Color::Red).ignore();
    })
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    fault::handle_panic(info, |_info| {
        let (mut led0, mut led1) = unsafe { steal_leds() };
        led0.set(Color::Yellow).ignore();
        led1.set(Color::Yellow).ignore();
    })
}
