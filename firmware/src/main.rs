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
mod phy;

use crate::efm32gg::dma;
use crate::ksz8091::KSZ8091;
use core::fmt::Write;
use cortex_m::asm;
use efm32gg11b820::interrupt;
use efm32gg_hal::cmu::CMUExt;
use efm32gg_hal::gpio::{EFM32Pin, GPIOExt};
use led::rgb::{self, Color};
use led::LED;
use smoltcp::iface::{InterfaceBuilder, NeighborCache};
use smoltcp::socket::{Dhcpv4Event, Dhcpv4Socket, SocketSet, TcpSocket, TcpSocketBuffer};
use smoltcp::time::{Duration, Instant};
use smoltcp::wire::{EthernetAddress, IpCidr, Ipv4Address, Ipv4Cidr};

#[cortex_m_rt::entry]
fn main() -> ! {
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
            level: log::LevelFilter::Debug,
        };
        unsafe { cortex_m_log::log::trick_init(&logger) }.unwrap();
        log::debug!("Logger online!");

        logger
    };

    log::debug!("a");
    let ethernet_addr = EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]);
    let mut neighbor_cache = [None; 8];
    let mut ip_addrs = [IpCidr::new(Ipv4Address::UNSPECIFIED.into(), 0)];

    log::debug!("a.1");
    log::debug!("a.1.1");
    let mut rx_region = dma::RxRegion([0; 1536]);
    log::debug!("a.2");
    let mut tx_region = dma::TxRegion([0; 1536]);
    log::debug!("a.3");
    let mut rx_buffer = dma::RxBuffer::new(&mut rx_region);
    let mut tx_buffer = dma::TxBuffer::new(&mut tx_region);

    log::debug!("b");
    let mut iface = InterfaceBuilder::new(
        efm32gg::EFM32GG::create(
            &mut rx_buffer,
            &mut tx_buffer,
            &eth,
            &cmu,
            &gpio,
            KSZ8091::new,
        )
        .expect("unable to create MACPHY"),
    )
    .ethernet_addr(ethernet_addr)
    .neighbor_cache(NeighborCache::new(neighbor_cache.as_mut()))
    .ip_addrs(ip_addrs.as_mut())
    .finalize();

    log::debug!("c");
    let mut tcp_rx_payload = [0; 128];
    let mut tcp_tx_payload = [0; 128];
    let tcp_socket = TcpSocket::new(
        TcpSocketBuffer::new(tcp_rx_payload.as_mut()),
        TcpSocketBuffer::new(tcp_tx_payload.as_mut()),
    );

    let mut dhcp_socket = Dhcpv4Socket::new();
    // XXX: just for testing
    dhcp_socket.set_max_lease_duration(Some(Duration::from_secs(10)));

    let mut socket_array = [None, None];
    let mut sockets = SocketSet::new(socket_array.as_mut());
    let tcp_handle = sockets.add(tcp_socket);
    let dhcp_handle = sockets.add(dhcp_socket);

    // let gpio = gpio.split(cmu.constrain().split().gpio);
    // let mut led0 = rgb::CommonAnodeLED::new(
    //     gpio.ph10.as_output(),
    //     gpio.ph11.as_output(),
    //     gpio.ph12.as_output(),
    // );

    // let mut led1 = rgb::CommonAnodeLED::new(
    //     gpio.ph13.as_output(),
    //     gpio.ph14.as_output(),
    //     gpio.ph15.as_output(),
    // );

    loop {
        asm::wfe();
        log::debug!("Exiting WFE");

        let timestamp = Instant::from_millis(rtc.cnt.read().cnt().bits());
        log::debug!("iface.poll: {}", timestamp);
        if let Err(err) = iface.poll(&mut sockets, timestamp) {
            log::error!("Failed to poll: {}", err)
        }

        match sockets.get::<Dhcpv4Socket>(dhcp_handle).poll() {
            None => {}
            Some(Dhcpv4Event::Configured(config)) => {
                log::debug!("DHCP config acquired!");

                log::debug!("IP address:      {}", config.address);
                iface.update_ip_addrs(|addrs| addrs[0] = IpCidr::Ipv4(config.address));

                if let Some(router) = config.router {
                    log::debug!("Default gateway: {}", router);
                    iface.routes_mut().add_default_ipv4_route(router).unwrap();
                } else {
                    log::debug!("Default gateway: None");
                    iface.routes_mut().remove_default_ipv4_route();
                }

                for (i, s) in config.dns_servers.iter().enumerate() {
                    if let Some(s) = s {
                        log::debug!("DNS server {}:    {}", i, s);
                    }
                }
            }
            Some(Dhcpv4Event::Deconfigured) => {
                log::debug!("DHCP lost config!");
                iface.update_ip_addrs(|addrs| {
                    addrs[0] = IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0))
                });
                iface.routes_mut().remove_default_ipv4_route();
            }
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
        log::debug!(
            "Handled sockets: {}",
            Instant::from_millis(rtc.cnt.read().cnt().bits())
        );
    }
}

#[efm32gg11b820::interrupt]
fn ETH() {
    //let gpio = (unsafe { &*efm32gg11b820::GPIO::ptr() }).split(
    //    (unsafe { &*efm32gg11b820::CMU::ptr() })
    //        .constrain()
    //        .split()
    //        .gpio,
    //);

    //let mut led0 = rgb::CommonAnodeLED::new(
    //    gpio.ph10.as_output(),
    //    gpio.ph11.as_output(),
    //    gpio.ph12.as_output(),
    //);
    //led0.set(Color::Blue);

    efm32gg::isr();

    //led0.set(Color::Black);
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

    if cortex_m::peripheral::DCB::is_debugger_attached() {
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

    if cortex_m::peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }
    loop {
        asm::wfe();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::sync::atomic::{self, Ordering};

    cortex_m::interrupt::disable();

    log::error!(
        "Panic at {}",
        Instant::from_millis(
            (unsafe { efm32gg11b820::Peripherals::steal() })
                .RTC
                .cnt
                .read()
                .cnt()
                .bits()
        )
    );

    let itm = unsafe { &mut *cortex_m::peripheral::ITM::ptr() };
    let stim = &mut itm.stim[0];

    cortex_m::iprintln!(stim, "{}", info);

    if cortex_m::peripheral::DCB::is_debugger_attached() {
        asm::bkpt();
    }

    loop {
        // add some side effect to prevent this from turning into a UDF instruction
        // see rust-lang/rust#28728 for details
        atomic::compiler_fence(Ordering::SeqCst)
    }
}
