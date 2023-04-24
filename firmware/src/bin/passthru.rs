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

#![no_main]
#![no_std]

/// Firmware for the PoE+ gated passthrough
///
/// This firmware implements the following:
/// - identify - Send a "0" or a "1" over TCP to the control port to disable or enable,
///              respectively, the flashing "Identify" LED.
use cortex_m::{asm, interrupt, peripheral};
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{pins, EFM32Pin, GPIOExt, Output};
use led::mono::{self, CommonAnodeLED};
use smoltcp::time::Instant;

type IdentifyLed = CommonAnodeLED<pins::PE4<Output>>;
type NetworkLed = CommonAnodeLED<pins::PE5<Output>>;

#[rtic::app(
    dispatchers = [ CAN0, CAN1, LCD ],
    device = efm32gg11b820,
    peripherals = true,
)]
mod app {
    use poe::efm32gg::{self, dma, EFM32GG};
    use poe::ksz8091::KSZ8091;
    use poe::network;

    use core::pin::Pin;
    use cortex_m::{delay::Delay, interrupt};
    use efm32gg_hal::cmu::CMUExt;
    use efm32gg_hal::gpio::{EFM32Pin, GPIOExt};
    use ignore_result::Ignore;
    use led::mono::{self, CommonAnodeLED};
    use smoltcp::iface::{InterfaceBuilder, Neighbor, NeighborCache, Route, Routes, SocketStorage};
    use smoltcp::socket::{Dhcpv4Socket, TcpSocket, TcpSocketBuffer};
    use smoltcp::time::Instant;
    use smoltcp::wire::{IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};

    #[monotonic(binds = SysTick, default = true)]
    type Monotonic = dwt_systick_monotonic::DwtSystick<25_000_000>; // 25 MHz

    macro_rules! schedule {
        ($name:ident, $duration:expr) => {
            $name::spawn_after($duration).expect(concat!("scheduling ", stringify!($name)))
        };
    }

    #[shared]
    struct SharedResources {
        led_identify: IdentifyLed,
        led_network: NetworkLed,
        network: network::Resources,
        rtc: efm32gg11b820::RTC,
    }

    #[local]
    struct LocalResources {
        spawn: Option<handle_network::SpawnHandle>,
    }

    pub struct IdentifyLed {
        led: crate::IdentifyLed,
        state: mono::State,
        spawn: Option<flash_identify_led::SpawnHandle>,
    }

    impl IdentifyLed {
        fn new(led: crate::IdentifyLed) -> IdentifyLed {
            IdentifyLed {
                spawn: None,
                led,
                state: mono::State::Off,
            }
        }

        fn enable(&mut self, en: bool) {
            match en {
                true => flash_identify_led::spawn().expect("spawning flash_identify_led"),
                false => {
                    if let Some(handle) = self.spawn.take() {
                        handle.cancel().expect("cancelling flash_identify_led");
                    }
                    self.led.set(mono::State::Off);
                    self.state = mono::State::Off;
                }
            }
            self.led.set(mono::State::Off);
            self.state = mono::State::Off;
        }
    }

    #[task(priority = 8, shared = [led_identify])]
    fn flash_identify_led(mut cx: flash_identify_led::Context) {
        use dwt_systick_monotonic::fugit::ExtU32;
        use mono::State::*;

        cx.shared.led_identify.lock(|id| {
            id.state = match id.state {
                On => Off,
                Off => On,
            };
            id.led.set(id.state);
            id.spawn = Some(schedule!(flash_identify_led, 250u32.millis()));
        });
    }

    pub struct NetworkLed {
        spawn: Option<occult_network_led::SpawnHandle>,
        led: crate::NetworkLed,
        state: mono::State,
        network: network::State,
        flashes: u8,
    }

    impl NetworkLed {
        fn new(led: crate::NetworkLed) -> NetworkLed {
            NetworkLed {
                spawn: None,
                led,
                state: mono::State::On,
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

    #[task(priority = 8, shared = [led_network])]
    fn occult_network_led(mut cx: occult_network_led::Context) {
        use dwt_systick_monotonic::fugit::ExtU32;
        use mono::State::*;
        use network::State::*;

        cx.shared.led_network.lock(|net| {
            match (net.network, net.flashes) {
                (Uninit, _) => net.state = On,
                (Operational, _) => net.state = Off,
                (network, 0) => {
                    net.flashes = match network {
                        Uninit | Operational => 0,
                        NoLink => 1,
                        NoDhcp => 2,
                        NoGateway => 3,
                    };
                    net.state = On;
                    net.spawn = Some(schedule!(occult_network_led, 1000u32.millis()));
                }
                (_, flashes) => match net.state {
                    Off => {
                        net.state = On;
                        net.flashes = flashes.saturating_sub(1);
                        net.spawn = Some(schedule!(occult_network_led, 250u32.millis()));
                    }
                    On => {
                        net.state = Off;
                        net.spawn = Some(schedule!(occult_network_led, 250u32.millis()));
                    }
                },
            }
            net.led.set(net.state);
        });
    }

    #[init(
        local = [
             eth_rx_region: dma::RxRegion = dma::RxRegion([0; 1536]),
             eth_tx_region: dma::TxRegion = dma::TxRegion([0; 1536]),
             eth_rx_descriptors: dma::RxDescriptors = dma::RxDescriptors::new(),
             eth_tx_descriptors: dma::TxDescriptors = dma::TxDescriptors::new(),
             tcp_rx_payload: [u8; 128] = [0; 128],
             tcp_tx_payload: [u8; 128] = [0; 128],

             neighbors: [Option<(IpAddress, Neighbor)>; 8] = [None; 8],
             sockets: [SocketStorage<'static>; 2] = [SocketStorage::EMPTY; 2],
             ip_addresses: [IpCidr; 1] =
                [IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0))],
            routes: [Option<(IpCidr, Route)>; 4] = [None; 4],
        ]
    )]
    fn init(mut cx: init::Context) -> (SharedResources, LocalResources, init::Monotonics) {
        use log::LevelFilter::*;

        let cmu = cx.device.CMU;
        let emu = cx.device.EMU;
        let gpio = cx.device.GPIO;
        let rtc = cx.device.RTC;

        // Switch to Power Configuration 1 (section 9.3.4.2) - power the digital LDO from DVDD
        emu.pwrctrl.write(|reg| reg.regpwrsel().set_bit());

        // Enable the HFRCO
        cmu.oscencmd.write(|reg| reg.hfrcoen().set_bit());
        while cmu.status.read().hfrcoens().bit_is_clear() {}

        // Wait for HFRCO to stabilize
        while cmu.status.read().hfrcordy().bit_is_clear() {}

        // Enable the GPIO peripheral
        cmu.hfbusclken0.write(|reg| reg.gpio().set_bit());

        // Initialize logging
        let logger = poe::log::init();
        #[cfg(feature = "rtt")]
        logger.add_rtt(poe::log::rtt::new(Debug));
        #[cfg(feature = "itm")]
        logger.add_itm(poe::log::itm::new(Info, &cmu, &gpio, cx.core.ITM));

        // Configure the HFXO's tuning capacitance to 10 pF
        cmu.hfxostartupctrl
            .modify(|_, w| unsafe { w.ctune().bits(15) });
        cmu.hfxosteadystatectrl
            .modify(|_, w| unsafe { w.ctune().bits(15) });

        // Enable the HFXO
        log::trace!("Enabling HFXO...");
        cmu.oscencmd.write(|reg| reg.hfxoen().set_bit());
        while cmu.status.read().hfxoens().bit_is_clear() {}

        // Wait for HFX0 to stabilize
        log::trace!("Waiting for HFXO to stabilize...");
        while cmu.status.read().hfxordy().bit_is_clear() {}

        log::trace!("Waiting for HXFO tuning...");
        while cmu.status.read().hfxopeakdetrdy().bit_is_clear() {}

        log::trace!("Waiting for valid IBTRIMXOCORE...");
        while cmu.hfxotrimstatus.read().valid().bit_is_clear() {}
        let hfxotrim = cmu.hfxotrimstatus.read().ibtrimxocore().bits();
        // TODO: Expect to see 0x532 (5*1.28 mA + 50*2 μA)
        log::debug!("IBTRIMXOCORE: 0x{:04X}", hfxotrim);

        log::trace!("Disabling HFXO...");
        cmu.oscencmd.write(|reg| reg.hfxodis().set_bit());
        while cmu.status.read().hfxoens().bit_is_set() {}
        while cmu.status.read().hfxordy().bit_is_set() {}

        log::trace!("Applying HXFO trim...");
        cmu.hfxoctrl.modify(|_, w| w.peakdetmode().cmd());
        cmu.hfxosteadystatectrl
            .modify(|_, w| unsafe { w.ibtrimxocore().bits(hfxotrim) });

        log::trace!("Re-enabling HFXO...");
        cmu.oscencmd.write(|reg| reg.hfxoen().set_bit());
        while cmu.status.read().hfxoens().bit_is_clear() {}

        // Wait for HFX0 to stabilize
        log::trace!("Waiting for HFXO to stabilize...");
        while cmu.status.read().hfxordy().bit_is_clear() {}
        log::trace!("HFXO configured!");

        // Update the EMU configuration
        let _ = cmu.status.read().bits();

        // Allow access to low energy peripherals with a clock speed greater than 50MHz
        cmu.ctrl.write(|reg| reg.wshfle().set_bit());

        // Set the appropriate read delay for flash
        cx.device.MSC.readctrl.write(|reg| reg.mode().ws2());

        // Switch to high frequency oscillator
        log::trace!("Switiching to HFXO...");
        cmu.hfclksel.write(|reg| reg.hf().hfxo());
        log::trace!("Using HFXO");

        // Update the EMU configuration
        let _ = cmu.status.read().bits();

        // Enable the RTC and set it to 1000Hz
        cmu.lfaclksel.write(|reg| reg.lfa().ulfrco());
        cmu.lfaclken0.write(|reg| reg.rtc().set_bit());
        rtc.ctrl.write(|reg| reg.en().set_bit());

        // Enable the TRNG and generate a random seed
        let seed = {
            let trng = &cx.device.TRNG0;

            cmu.hfperclken0.modify(|_, reg| reg.trng0().set_bit());
            trng.control.modify(|_, reg| reg.enable().set_bit());

            while trng.fifolevel.read().bits() < 2 {}
            let seed =
                u64::from(trng.fifo.read().bits()) << 32 | u64::from(trng.fifo.read().bits());

            trng.control.modify(|_, reg| reg.enable().clear_bit());

            log::trace!("TRNG produced: 0x{:08X}", seed);

            seed
        };

        let mut gpio_clk = cmu.constrain().split().gpio;
        gpio_clk.enable();

        // TODO: Move into efm32gg-hal.
        // Configure PE13 as an input with pull-up, and enable interrupts on the falling edge. This
        // is connected to INTRP on the PHY.
        gpio.pe_modeh.modify(|_, w| w.mode13().input());
        gpio.extipselh.modify(|_, w| w.extipsel13().porte());
        gpio.extipinselh.modify(|_, w| w.extipinsel13().pin13());
        gpio.extifall
            .modify(|_, w| unsafe { w.extifall().bits(1 << 13) });
        gpio.ifc.write(|w| unsafe { w.ext().bits(1 << 13) });
        efm32gg11b820::NVIC::unpend(efm32gg11b820::Interrupt::GPIO_ODD);
        gpio.ien.write(|w| unsafe { w.ext().bits(1 << 13) });

        let gpio = gpio.split(gpio_clk);
        let _swo = gpio.pf2.as_output();

        let mut led_identify = IdentifyLed::new(CommonAnodeLED::new(gpio.pe4.as_opendrain()));
        let mut led_network = NetworkLed::new(CommonAnodeLED::new(gpio.pe5.as_opendrain()));

        led_identify.enable(false);

        let mut delay = Delay::new(cx.core.SYST, 19_000_000);
        let (mac_phy, mac_addr) = EFM32GG::new(
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
                rmii_rxd0: &mut gpio.pa2.as_input(),
                rmii_refclk: &mut gpio.pa3.as_output(),
                rmii_crsdv: &mut gpio.pa4.as_input(),
                rmii_rxer: &mut gpio.pa5.as_input(),
                rmii_mdio: &mut gpio.pb3.as_output(),
                rmii_mdc: &mut gpio.pb4.as_output(),
                rmii_txd0: &mut gpio.pe15.as_output(),
                rmii_txd1: &mut gpio.pe14.as_output(),
                rmii_txen: &mut gpio.pa0.as_output(),
                rmii_rxd1: &mut gpio.pa1.as_input(),
                phy_reset: &mut gpio.pe11.as_output(),
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

        let dhcp_handle = interface.add_socket(Dhcpv4Socket::new());
        led_network.show(network::State::NoLink);

        let syst = delay.free();
        (
            SharedResources {
                led_identify,
                led_network,
                network: network::Resources {
                    interface,
                    dhcp_handle,
                    tcp_handle,
                },
                rtc,
            },
            LocalResources { spawn: None },
            init::Monotonics(Monotonic::new(
                &mut cx.core.DCB,
                cx.core.DWT,
                syst,
                25_000_000,
            )),
        )
    }

    #[task(capacity = 2, local = [spawn], shared = [led_identify, led_network, network, rtc])]
    fn handle_network(mut cx: handle_network::Context) {
        log::trace!("Handling network...");

        let timestamp = Instant::from_millis(cx.shared.rtc.lock(|rtc| rtc.cnt.read().cnt().bits()));
        let spawn = cx.local.spawn;
        let mut led_id = cx.shared.led_identify;
        let mut led_net = cx.shared.led_network;
        let mut network = cx.shared.network;

        match network.lock(|network| network.interface.poll(timestamp)) {
            Ok(true) => {
                log::trace!("Handling sockets...");

                network.lock(|network| {
                    network.handle_sockets(
                        |state| led_net.lock(|led| led.show(state)),
                        |en| led_id.lock(|led| led.enable(en)),
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
            *spawn = spawn
                .take()
                .and_then(|h| h.reschedule_after(delay).ok())
                .or_else(|| Some(schedule!(handle_network, delay)));
        }

        log::trace!("Handled sockets: {}", timestamp);
    }

    #[task(binds = ETH, shared = [network])]
    fn eth_irq(mut cx: eth_irq::Context) {
        interrupt::free(|_| {
            cx.shared.network.lock(|network| {
                network.interface.device_mut().mac_irq();
            })
        });

        handle_network::spawn().ignore();
    }

    #[task(binds = GPIO_ODD, shared = [led_network, network])]
    fn gpio_odd_irq(mut cx: gpio_odd_irq::Context) {
        use dwt_systick_monotonic::ExtU32;
        use network::State::*;

        // Clear the PHY interrupt
        (unsafe { &*efm32gg11b820::GPIO::ptr() })
            .ifc
            .write(|w| unsafe { w.ext().bits(1 << 13) });

        // TODO: This probably should be deferred since it's reading from the PHY
        let mut led = cx.shared.led_network;
        cx.shared.network.lock(|network| {
            led.lock(|led| {
                let device = network.interface.device_mut();
                device.phy_irq();

                match (device.link_state().is_some(), led.network) {
                    (true, NoLink) => {
                        log::debug!("Link acquired");
                        led.show(NoDhcp);
                        network.reset_dhcp();
                    }
                    (false, _) => {
                        log::debug!("Link lost");
                        led.show(NoLink);
                    }
                    _ => {}
                }
            });
        });
        // TODO: Why is the one-second delay necessary? 100 ms doesn't work.
        handle_network::spawn_after(1000u32.millis()).ignore();
    }
}

// Light up both LEDs, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn DefaultHandler(irqn: i16) {
    use mono::State::*;

    interrupt::disable();

    log::error!("Default Handler: irq {}", irqn);
    let (mut id, mut net) = unsafe { steal_leds() };
    id.set(On);
    net.set(On);

    if peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }

    loop {
        asm::wfe();
    }
}

// Light up both LEDs, trigger a breakpoint, and loop
#[cortex_m_rt::exception]
fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    use mono::State::*;

    interrupt::disable();

    log::error!("Hard Fault: {:?}", frame);
    let (mut id, mut net) = unsafe { steal_leds() };
    id.set(On);
    net.set(On);

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
pub unsafe fn steal_leds() -> (IdentifyLed, NetworkLed) {
    let periph = efm32gg11b820::Peripherals::steal();
    let gpio = periph.GPIO.split(periph.CMU.constrain().split().gpio);

    let id = CommonAnodeLED::new(gpio.pe4.as_opendrain());
    let net = CommonAnodeLED::new(gpio.pe5.as_opendrain());

    (id, net)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use mono::State::*;

    let rtc = unsafe { &*efm32gg11b820::RTC::ptr() };
    let itm = unsafe { &mut *cortex_m::peripheral::ITM::ptr() };

    cortex_m::interrupt::disable();

    let now = Instant::from_millis(rtc.cnt.read().cnt().bits());
    let stim = &mut itm.stim[0];

    log::error!("Panic at {}", now);
    cortex_m::iprintln!(stim, "{}", info);

    let (mut id, mut net) = unsafe { steal_leds() };
    id.set(On);
    net.set(On);

    if cortex_m::peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }

    loop {}
}
