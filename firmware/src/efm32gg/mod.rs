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

pub mod dma;

use crate::mac;
use crate::phy::{probe_addr as probe_phy_addr, LinkState, Phy, Register};
use core::cmp;
use core::convert::TryInto;
use dma::{
    BufferDescriptor, BufferDescriptorOwnership, RxBuffer, RxBufferDescriptor, TxBuffer,
    TxBufferDescriptor,
};
use efm32gg11b820::{self, Interrupt, ETH, NVIC};
use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use ignore_result::Ignore;
use smoltcp::wire::EthernetAddress;
use smoltcp::{self, phy, time, Error};

pub struct EFM32GG<'a, P: Phy> {
    mac: Mac<'a>,
    #[allow(unused)]
    phy: P,
}

impl<'a, P: Phy> EFM32GG<'a, P> {
    pub fn new<F>(
        rx_buffer: RxBuffer<'a>,
        tx_buffer: TxBuffer<'a>,
        eth: ETH,
        delay: &mut dyn DelayMs<u8>,
        pins: Pins,
        new_phy: F,
    ) -> Result<(EFM32GG<'a, P>, EthernetAddress), &'static str>
    where
        F: FnOnce(u8, &mut dyn mac::Mdio) -> P,
    {
        use mac::Mdio;

        let mut rmii = Rmii::new(eth, delay, pins);
        let phy_addr = probe_phy_addr(&rmii).ok_or("Failed to find PHY")?;
        let phy = new_phy(phy_addr, &mut rmii);
        let oui = phy.oui(&rmii);

        // Set the advertisement as follows:
        // - IEEE 802.3
        // - 10BASE-T (Half-Duplex)
        // - No Pause
        // - No next page capability (recommended by data sheet)
        #[allow(clippy::unusual_byte_groupings)]
        rmii.write(phy_addr, Register::AutoAdvertisement, 0b000000_00001_00001);

        let mac_addr = EthernetAddress([oui.0[0], oui.0[1], oui.0[2], 0x00, 0x00, 0x01]);
        let mac = Mac::new(rmii, mac_addr, rx_buffer, tx_buffer);

        log::debug!("MAC/PHY initialized ({}/{})", mac_addr, phy_addr);

        Ok((EFM32GG { mac, phy }, mac_addr))
    }

    pub fn mac_irq(&mut self) {
        self.mac.irq()
    }

    pub fn phy_irq(&mut self) {
        self.phy.irq(&mut self.mac);
    }

    pub fn link_state(&self) -> Option<LinkState> {
        self.phy.link_state(&self.mac)
    }
}

pub struct Pins<'a> {
    pub rmii_refclk: &'a mut dyn OutputPin<Error = ()>,
    pub phy_reset: &'a mut dyn OutputPin<Error = ()>,

    pub rmii_crsdv: &'a mut dyn InputPin<Error = ()>,
    pub rmii_rxd0: &'a mut dyn InputPin<Error = ()>,
    pub rmii_rxd1: &'a mut dyn InputPin<Error = ()>,
    pub rmii_rxer: &'a mut dyn InputPin<Error = ()>,
    pub rmii_txd0: &'a mut dyn OutputPin<Error = ()>,
    pub rmii_txd1: &'a mut dyn OutputPin<Error = ()>,

    pub rmii_mdc: &'a mut dyn OutputPin<Error = ()>,
    pub rmii_mdio: &'a mut dyn OutputPin<Error = ()>,
    pub rmii_txen: &'a mut dyn OutputPin<Error = ()>,
}

struct Rmii {
    eth: ETH,
}

impl Rmii {
    /// Initialize the RMII
    ///
    /// Note: This assumes the following:
    ///       - PHY will be interfaced via RMII
    ///       - EFM provides the clock
    ///       - HFXO is 25 MHz
    fn new(eth: ETH, delay: &mut dyn DelayMs<u8>, pins: Pins) -> Rmii {
        let cmu = unsafe { &*efm32gg11b820::CMU::ptr() };

        // Enable the HFPER clock and source CLKOUT2 from the frequency-doubled HFXO
        cmu.ctrl.modify(|_, reg| {
            reg.hfperclken().set_bit();
            reg.clkoutsel2().hfxox2q();
            reg
        });

        // Enable the clock to the ethernet controller
        cmu.hfbusclken0.modify(|_, reg| reg.eth().set_bit());

        // Scale the MDC down to 1.5625MHz (below the 2.5MHz limit)
        // Enable 1536-byte frames, which are needed to support 802.11Q VLAN tagging
        eth.networkcfg.write(|reg| {
            reg.mdcclkdiv().divby16();
            reg.rx1536byteframes().set_bit();
            reg.rxchksumoffloaden().set_bit();
            reg.speed().clear_bit();
            reg.fullduplex().clear_bit();
            reg
        });

        // Hold the PHY module in reset
        pins.phy_reset.set_low().ignore();

        // Enable the PHY's reference clock on PA3 (CMU_CLK2 #1)
        cmu.routeloc0.modify(|_, reg| reg.clkout2loc().loc1());
        cmu.routepen.modify(|_, reg| reg.clkout2pen().set_bit());

        // Enable the MDC on PB4 (ETH_MDC #0) and MDIO on PB3 (ETH_MDIO #0)
        eth.routeloc1.write(|reg| {
            reg.rmiiloc().loc0();
            reg.mdioloc().loc0();
            reg
        });
        eth.routepen.write(|reg| {
            reg.rmiipen().set_bit();
            reg.mdiopen().set_bit();
            reg
        });

        // Enable the management interface
        eth.networkctrl.write(|reg| reg.manporten().set_bit());

        // Wait for the PHY's supply to stabilize and for it to initialize
        delay.delay_ms(10);

        // Release the PHY reset
        pins.phy_reset.set_high().ignore();

        Rmii { eth }
    }
}

impl mac::Mdio for Rmii {
    fn read(&self, address: u8, register: Register) -> u16 {
        log::trace!("MDIO.read(0x{:02X}, {:?})", address, register);
        mdio_read(&self.eth, address, register)
    }

    fn write(&mut self, address: u8, register: Register, data: u16) {
        log::trace!(
            "MDIO.write(0x{:02X}, {:?}, 0x{:04X})",
            address,
            register,
            data
        );
        mdio_write(&mut self.eth, address, register, data)
    }
}

struct Mac<'a> {
    rx_buffer: RxBuffer<'a>,
    tx_buffer: TxBuffer<'a>,
    eth: ETH,
}

impl<'a> Mac<'a> {
    /// Fully initializes the MAC, starting from the initialized RMII
    fn new(
        rmii: Rmii,
        addr: EthernetAddress,
        rx_buffer: RxBuffer<'a>,
        tx_buffer: TxBuffer<'a>,
    ) -> Mac<'a> {
        let eth = rmii.eth;

        // Set the RX buffer size to 128 bytes
        eth.dmacfg.write(|reg| {
            unsafe { reg.rxbufsize().bits(128 / 64) };
            unsafe { reg.ambabrstlen().bits(0x01) };
            reg.txpbuftcpen().set_bit();
            reg.txpbufsize().set_bit();
            reg.rxpbufsize().size3();
            reg
        });

        // Set the RX buffer descriptor queue address
        eth.rxqptr
            .write(|reg| unsafe { reg.dmarxqptr().bits(rx_buffer.address() as u32 >> 2) });

        // Set the TX buffer descriptor queue address
        eth.txqptr
            .write(|reg| unsafe { reg.dmatxqptr().bits(tx_buffer.address() as u32 >> 2) });

        // Set the hardware address filter, starting with the bottom register first
        eth.specaddr1bottom.write(|reg| unsafe {
            reg.addr()
                .bits(u32::from_be_bytes(addr.0[0..4].try_into().unwrap()).swap_bytes())
        });
        eth.specaddr1top.write(|reg| unsafe {
            reg.addr()
                .bits(u16::from_be_bytes(addr.0[4..6].try_into().unwrap()).swap_bytes())
        });

        // Clear pending interrupts
        NVIC::unpend(Interrupt::ETH);
        eth.ifcr.write(|reg| {
            // reg.mngmntdone().set_bit();
            reg.rxcmplt().set_bit();
            reg.rxusedbitread().set_bit();
            reg.txusedbitread().set_bit();
            reg.txunderrun().set_bit();
            reg.rtrylmtorlatecol().set_bit();
            reg.ambaerr().set_bit();
            reg.txcmplt().set_bit();
            reg.rxoverrun().set_bit();
            reg.respnotok().set_bit();
            reg.nonzeropfrmquant().set_bit();
            reg.pausetimezero().set_bit();
            reg.pfrmtx().set_bit();
            reg.ptpdlyreqfrmrx().set_bit();
            reg.ptpsyncfrmrx().set_bit();
            reg.ptpdlyreqfrmtx().set_bit();
            reg.ptpsyncfrmtx().set_bit();
            reg.ptppdlyreqfrmrx().set_bit();
            reg.ptppdlyrespfrmrx().set_bit();
            reg.ptppdlyreqfrmtx().set_bit();
            reg.ptppdlyrespfrmtx().set_bit();
            reg.tsusecregincr().set_bit();
            reg.rxlpiindc().set_bit();
            reg.wolevntrx().set_bit();
            reg.tsutimercomp().set_bit();
            reg
        });

        // Enable interrupts
        eth.iens.write(|reg| {
            // TODO: Do these operations asynchronously
            // reg.mngmntdone().set_bit();
            reg.rxcmplt().set_bit();
            // TODO: What is this used for?
            //reg.rxusedbitread().set_bit();
            //reg.txusedbitread().set_bit();
            reg.txunderrun().set_bit();
            reg.rtrylmtorlatecol().set_bit();
            reg.ambaerr().set_bit();
            reg.txcmplt().set_bit();
            reg.rxoverrun().set_bit();
            reg.respnotok().set_bit();
            reg.nonzeropfrmquant().set_bit();
            reg.pausetimezero().set_bit();
            reg.pfrmtx().set_bit();
            reg.ptpdlyreqfrmrx().set_bit();
            reg.ptpsyncfrmrx().set_bit();
            reg.ptpdlyreqfrmtx().set_bit();
            reg.ptpsyncfrmtx().set_bit();
            reg.ptppdlyreqfrmrx().set_bit();
            reg.ptppdlyrespfrmrx().set_bit();
            reg.ptppdlyreqfrmtx().set_bit();
            reg.ptppdlyrespfrmtx().set_bit();
            reg.tsusecregincr().set_bit();
            reg.rxlpiindc().set_bit();
            reg.wolevntrx().set_bit();
            reg.tsutimercomp().set_bit();
            reg
        });
        unsafe {
            NVIC::unmask(Interrupt::ETH);
        }

        // Enable transmitting/receiving
        eth.networkctrl.modify(|_, w| {
            w.enbrx().set_bit();
            w.enbtx().set_bit();
            w
        });

        // Enable the global clock
        eth.ctrl.write(|reg| reg.gblclken().set_bit());

        Mac {
            rx_buffer,
            tx_buffer,
            eth,
        }
    }

    fn find_rx_window(&self) -> Option<(usize, usize)> {
        let mut start = None;
        let mut end = None;
        let descriptors = self.rx_buffer.descriptors();

        for _ in 0..2 {
            for (i, d) in descriptors.iter().enumerate() {
                if d.start_of_frame() && d.ownership() == BufferDescriptorOwnership::Software {
                    start = Some(i);
                }
                if d.end_of_frame()
                    && d.ownership() == BufferDescriptorOwnership::Software
                    && start.is_some()
                {
                    end = Some(i);
                    break;
                }
                if d.ownership() == BufferDescriptorOwnership::Hardware {
                    start = None;
                    end = None;
                }
            }

            if start.is_none() || end.is_some() {
                break;
            }
        }

        match (start, end) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => None,
        }
    }

    fn find_tx_window(&mut self) -> Option<(usize, usize)> {
        let queue_ptr = (unsafe { (*ETH::ptr()).txqptr.read().dmatxqptr().bits() << 2 }
            - self.tx_buffer.address() as u32) as usize
            / core::mem::size_of::<TxBufferDescriptor>();
        let descriptors = self.tx_buffer.descriptors_mut();

        // Walk forward from the queue pointer (wrapping around to the beginning of the buffer if
        // necessary), looking for the first unused descriptor. This will be the start of the
        // transmit window.
        let start = {
            let mut start = None;
            for i in 0..descriptors.len() {
                let d = (queue_ptr + i) % descriptors.len();
                if descriptors[d].ownership() == BufferDescriptorOwnership::Software {
                    start = Some(d);
                    break;
                }
            }

            start?
        };

        // Reclaim the descriptors of the previously-used transmit window. Unfortunately, the
        // hardware only clears the ownership flag on the first descriptor for a frame.
        let mut end_of_buffer = true;
        let len = descriptors.len();
        for i in (1..len).map(|i| (len + queue_ptr - i) % len) {
            let d = &mut descriptors[i];
            match d.ownership() {
                BufferDescriptorOwnership::Hardware => {
                    if end_of_buffer && !d.end_of_frame() {
                        log::warn!("Dangling TX desc {}: {:?}", i, d);
                    } else if !end_of_buffer && d.end_of_frame() {
                        log::warn!("Released TX frame {} (unsent?): {:?}", i, d);
                    }
                    end_of_buffer = false;
                    d.claim();
                }
                BufferDescriptorOwnership::Software => {
                    fn error_str(cond: bool, msg: &str) -> &str {
                        match cond {
                            false => "",
                            true => msg,
                        }
                    }

                    log::trace!(
                        "  {:>2} (Done) - {:?} (errors:{}{}{}{}{})",
                        i,
                        d,
                        error_str(d.error_retry_limit(), " 'retry limit exceeded'"),
                        error_str(d.error_tx_underrun(), " underrun"),
                        error_str(d.error_frame_corrupt(), " 'frame corruption'"),
                        error_str(d.error_late_collision(), " 'late collision'"),
                        match d.error_checksum_generation() {
                            Some(ref err) => err.as_str(),
                            None => "",
                        }
                    );

                    if end_of_buffer {
                        log::trace!("    (may be duplicate)");
                        break;
                    }
                    end_of_buffer = true;
                }
            }
        }

        // Walk forward from the start of the transmit window (wrapping around to the beginning of
        // the buffer if necessary), looking for the last unused descriptor. This will be the end
        // of the transmit window.
        let mut len = 1;
        for i in 1..descriptors.len() {
            if descriptors[(start + i) % descriptors.len()].ownership()
                == BufferDescriptorOwnership::Software
            {
                len = i;
            } else {
                break;
            }
        }

        Some((start, len))
    }

    pub fn irq(&mut self) {
        let int = self.eth.ifcr.read();

        macro_rules! bit_str {
            ($reg:ident) => {
                match int.$reg().bit_is_set() {
                    true => concat!(" ", stringify!($reg)),
                    false => "",
                }
            };
        }

        log::trace!(
            "ETH IRQ:{}{}{}{}{}{}",
            bit_str!(mngmntdone),
            bit_str!(rxcmplt),
            bit_str!(rxoverrun),
            bit_str!(txcmplt),
            bit_str!(txunderrun),
            bit_str!(ambaerr),
        );

        if int.mngmntdone().bit_is_set() {
            self.eth.ifcr.write(|reg| reg.mngmntdone().set_bit());
        }
        if int.rxcmplt().bit_is_set() {
            self.eth.ifcr.write(|reg| reg.rxcmplt().set_bit());
        }
        if int.rxoverrun().bit_is_set() {
            self.eth.ifcr.write(|reg| reg.rxoverrun().set_bit());
            log::error!("RX Overrun Interrupt");
        }
        if int.txcmplt().bit_is_set() {
            self.eth.ifcr.write(|reg| reg.txcmplt().set_bit());
        }
        if int.txunderrun().bit_is_set() {
            self.eth.ifcr.write(|reg| reg.txunderrun().set_bit());
            log::error!("TX Underrun Interrupt");
        }
        if int.ambaerr().bit_is_set() {
            self.eth.ifcr.write(|reg| reg.ambaerr().set_bit());
            log::error!("TX AMBA Error Interrupt");
        }

        // XXX: Read from ifcr seems to be racy. I'm guessing its because that register can change
        // values even if interrupts are disabled. I saw the following in a test run, which
        // shouldn't be possible (0x02 is RXCMPLT): Unhandled interrupt (ETH): 0x2
        //
        // let int = self.eth.ifcr.read();
        // if int.bits() != 0 {
        //     log::error!("Unhandled interrupt (ETH): {:#X}", int.bits());
        //     self.eth.ifcr.write(|reg| unsafe { reg.bits(int.bits()) });
        //     led0.set(Color::Cyan);
        //     led1.set(Color::Cyan);
        // }
    }
}

impl mac::Mdio for Mac<'_> {
    fn read(&self, address: u8, register: Register) -> u16 {
        mdio_read(&self.eth, address, register)
    }

    fn write(&mut self, address: u8, register: Register, data: u16) {
        mdio_write(&mut self.eth, address, register, data)
    }
}

fn mdio_read(eth: &ETH, address: u8, register: Register) -> u16 {
    eth.phymngmnt.write(|reg| {
        unsafe { reg.phyaddr().bits(address) };
        unsafe { reg.phyrwdata().bits(0x00) };
        unsafe { reg.regaddr().bits(register.into()) };
        unsafe { reg.operation().bits(0b10) };

        unsafe { reg.write10().bits(0b10) };
        reg.write1().set_bit();
        reg.write0().clear_bit();
        reg
    });

    while eth.networkstatus.read().mandone().bit_is_clear() {}

    eth.phymngmnt.read().phyrwdata().bits()
}

fn mdio_write(eth: &mut ETH, address: u8, register: Register, data: u16) {
    eth.phymngmnt.write(|reg| {
        unsafe { reg.phyaddr().bits(address) };
        unsafe { reg.phyrwdata().bits(data) };
        unsafe { reg.regaddr().bits(register.into()) };
        unsafe { reg.operation().bits(0b01) };

        unsafe { reg.write10().bits(0b10) };
        reg.write1().set_bit();
        reg.write0().clear_bit();
        reg
    });

    while eth.networkstatus.read().mandone().bit_is_clear() {}
}

impl<'a, P: Phy> phy::Device<'a> for EFM32GG<'_, P> {
    type RxToken = RxToken<'a>;
    type TxToken = TxToken<'a>;

    fn capabilities(&self) -> phy::DeviceCapabilities {
        let mut caps = phy::DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps
    }

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        let (rx_start, rx_end) = self.mac.find_rx_window()?;
        let (tx_start, tx_length) = self.mac.find_tx_window()?;

        Some((
            RxToken {
                descriptors: self.mac.rx_buffer.descriptors_mut(),
                start: rx_start,
                end: rx_end,
            },
            TxToken {
                descriptors: self.mac.tx_buffer.descriptors_mut(),
                start: tx_start,
                length: tx_length,
            },
        ))
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        let (start, length) = self.mac.find_tx_window()?;

        Some(TxToken {
            descriptors: self.mac.tx_buffer.descriptors_mut(),
            start,
            length,
        })
    }
}

pub struct RxToken<'a> {
    /// The list of allocated RX buffer descriptors.
    descriptors: &'a mut [RxBufferDescriptor],

    /// The index of the starting RX buffer descriptor.
    start: usize,

    /// The length of the token, in RX buffers.
    end: usize,
}

impl<'a> phy::RxToken for RxToken<'a> {
    fn consume<R, F>(self, _timestamp: time::Instant, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&mut [u8]) -> smoltcp::Result<R>,
    {
        let mut data = [0; 1536];

        let mut orig = self.start;
        let mut dest = 0;

        loop {
            let d = &mut self.descriptors[orig];
            data[(dest * 128)..][..128].copy_from_slice(d.as_slice());
            d.release();

            if orig == self.end {
                break;
            }

            orig = (orig + 1) % self.descriptors.len();
            dest += 1;
        }

        f(&mut data)
    }
}

pub struct TxToken<'a> {
    /// The list of allocated TX buffer descriptors.
    descriptors: &'a mut [TxBufferDescriptor],

    /// The index of the starting TX buffer descriptor.
    start: usize,

    /// The length of the token, in TX buffers.
    length: usize,
}

impl<'a> phy::TxToken for TxToken<'a> {
    fn consume<R, F>(self, _timestamp: time::Instant, len: usize, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&mut [u8]) -> smoltcp::Result<R>,
    {
        if len > (self.length * 128) {
            log::warn!("TX exhausted: buffer={} token={}", len, self.length * 128);
            return Err(Error::Exhausted);
        }

        debug_assert!(len > 0);
        let last_buffer = (len - 1) / 128;

        let mut data = [0; 1536];
        let result = f(&mut data[0..len])?;

        for i in 0..=last_buffer {
            let d = &mut self.descriptors[(self.start + i) % self.descriptors.len()];
            let buffer_len = cmp::min(128, len - i * 128);

            d.as_slice_mut().copy_from_slice(&data[(i * 128)..][..128]);
            d.set_length(buffer_len);
            d.set_last_buffer(i == last_buffer);
            d.release();
        }

        unsafe {
            (*efm32gg11b820::ETH::ptr())
                .networkctrl
                .modify(|_, reg| reg.txstrt().set_bit());
        }

        Ok(result)
    }
}
