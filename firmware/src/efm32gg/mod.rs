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

pub mod dma;

pub use dma::RxBuffer;
pub use dma::TxBuffer;

use core::{mem, slice};
use cortex_m::{asm, interrupt};
use dma::{BufferDescriptor, BufferDescriptorOwnership, RxBufferDescriptor, TxBufferDescriptor};
use efm32gg11b820::{self, Interrupt, CMU, ETH, GPIO, NVIC};
use mac;
use phy::{probe_for_phy, Register, PHY};
use smoltcp::{self, phy, time, Error};

pub struct EFM32GG<'a, 'b: 'a, P: PHY> {
    mac: MAC<'a, 'b>,
    phy: P,
}

impl<'a, 'b: 'a, P: PHY> EFM32GG<'a, 'b, P> {
    pub fn create<F>(
        rx_buffer: &'a mut RxBuffer<'b>,
        tx_buffer: &'a mut TxBuffer<'b>,
        eth: &'a ETH,
        cmu: &CMU,
        gpio: &GPIO,
        nvic: &mut NVIC,
        new_phy: F,
    ) -> Result<EFM32GG<'a, 'b, P>, &'static str>
    where
        F: FnOnce(u8) -> P,
    {
        let mac = MAC::create(rx_buffer, tx_buffer, eth, cmu, gpio, nvic);

        // XXX: Wait for the PHY
        for _ in 0..1000 {
            asm::nop();
        }

        let phy = new_phy(probe_for_phy(&mac).ok_or("Failed to find PHY")?);
        debug!("OUI: {}", phy.oui(&mac));

        Ok(EFM32GG { mac, phy })
    }
}

struct MAC<'a, 'b: 'a> {
    rx_buffer: &'a mut RxBuffer<'b>,
    tx_buffer: &'a mut TxBuffer<'b>,
    eth: &'a ETH,
}

impl<'a, 'b: 'a> MAC<'a, 'b> {
    /// This assumes that the PHY will be interfaced via RMII with the EFM providing the ethernet
    /// clock.
    fn create(
        rx_buffer: &'a mut RxBuffer<'b>,
        tx_buffer: &'a mut TxBuffer<'b>,
        eth: &'a ETH,
        cmu: &CMU,
        gpio: &GPIO,
        nvic: &mut NVIC,
    ) -> MAC<'a, 'b> {
        // Enable the HFPER clock and source CLKOUT2 from HFXO
        cmu.ctrl.modify(|_, reg| {
            reg.hfperclken().set_bit();
            reg.clkoutsel2().hfxo();
            reg
        });

        // Enable the clock to the ethernet controller
        cmu.hfbusclken0.modify(|_, reg| reg.eth().set_bit());

        // Scale the MDC down to 1.5625MHz (below the 2.5MHz limit)
        // Enable 1536-byte frames, which are needed to support 802.11Q VLAN tagging
        eth.networkcfg.write(|reg| {
            reg.mdcclkdiv().divby32();
            reg.rx1536byteframes().set_bit();
            reg.rxchksumoffloaden().set_bit();
            reg.speed().set_bit();
            reg
        });

        // TODO: Use BITSET/BITCLEAR instead
        // Hold the PHY module in reset
        gpio.ph_model.modify(|_, reg| reg.mode7().pushpull());
        gpio.ph_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() & !(1 << 7)) });

        // Power up the PHY module
        gpio.pi_modeh.modify(|_, reg| reg.mode10().pushpull());
        gpio.pi_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() | (1 << 10)) });

        // Configure the RMII GPIOs
        gpio.pf_model.write(|reg| {
            reg.mode6().pushpull(); // TXD1
            reg.mode7().pushpull(); // TXD0
            reg
        });
        gpio.pf_modeh.write(|reg| {
            reg.mode8().pushpull(); // TX_EN
            reg.mode9().input(); // RXD1
            reg
        });
        gpio.pd_modeh.write(|reg| {
            reg.mode9().input(); // RXD0
            reg.mode10().pushpull(); // REFCLK
            reg.mode11().input(); // CRS_DV
            reg.mode12().input(); // RX_ER
            reg.mode13().pushpull(); // MDIO
            reg.mode14().pushpull(); // MDC
            reg
        });

        // Enable the PHY's reference clock
        cmu.routeloc0.modify(|_, reg| reg.clkout2loc().loc5());
        cmu.routepen.modify(|_, reg| reg.clkout2pen().set_bit());

        // Enable the RMII and MDIO
        eth.routeloc1.write(|reg| {
            reg.rmiiloc().loc1();
            reg.mdioloc().loc1();
            reg
        });
        eth.routepen.write(|reg| {
            reg.rmiipen().set_bit();
            reg.mdiopen().set_bit();
            reg
        });

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
        eth.specaddr1bottom
            .write(|reg| unsafe { reg.addr().bits(0x00_00_00_02) });
        eth.specaddr1top
            .write(|reg| unsafe { reg.addr().bits(0x00_00_02_00) });

        // Clear pending interrupts
        efm32gg11b820::NVIC::unpend(Interrupt::ETH);
        eth.ifcr.write(|reg| {
            reg.mngmntdone().set_bit();
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
            reg.mngmntdone().set_bit();
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
        nvic.enable(Interrupt::ETH);

        // Enable transmitting/receiving and the management interface
        eth.networkctrl.write(|reg| {
            reg.enbrx().set_bit();
            reg.enbtx().set_bit();
            reg.manporten().set_bit();
            reg
        });

        // Release the PHY reset
        gpio.ph_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() | (1 << 7)) });

        // Enable the global clock
        eth.ctrl.write(|reg| reg.gblclken().set_bit());

        MAC {
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

    fn find_tx_window(&self) -> Option<(usize, usize)> {
        let descriptors = self.tx_buffer.descriptors();
        let queue_ptr = (unsafe {
            (*efm32gg11b820::ETH::ptr())
                .txqptr
                .read()
                .dmatxqptr()
                .bits() << 2
        } - self.tx_buffer.address() as u32) as usize
            / mem::size_of::<TxBufferDescriptor>();

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

            match start {
                Some(idx) => idx,
                None => return None,
            }
        };

        // Walk forward from the start of the transmit window (wrapping around to the beginning of
        // the buffer if necessary), looking for the last unused descriptor. This will be the end
        // of the transmit window.
        let mut len = 0;
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
}

impl<'a, 'b> mac::MAC for MAC<'a, 'b> {
    fn mdio_read(&self, address: u8, register: Register) -> u16 {
        self.eth.phymngmnt.write(|reg| {
            unsafe { reg.phyaddr().bits(address) };
            unsafe { reg.phyrwdata().bits(0x00) };
            unsafe { reg.regaddr().bits(register.into()) };
            unsafe { reg.operation().bits(0b10) };

            unsafe { reg.write10().bits(0b10) };
            reg.write1().set_bit();
            reg.write0().clear_bit();
            reg
        });

        while self.eth.networkstatus.read().mandone().bit_is_clear() {}

        self.eth.phymngmnt.read().phyrwdata().bits()
    }

    fn mdio_write(&mut self, address: u8, register: Register, data: u16) {
        self.eth.phymngmnt.write(|reg| {
            unsafe { reg.phyaddr().bits(address) };
            unsafe { reg.phyrwdata().bits(data) };
            unsafe { reg.regaddr().bits(register.into()) };
            unsafe { reg.operation().bits(0b01) };

            unsafe { reg.write10().bits(0b10) };
            reg.write1().set_bit();
            reg.write0().clear_bit();
            reg
        });

        while self.eth.networkstatus.read().mandone().bit_is_clear() {}
    }
}

pub fn isr() {
    interrupt::free(|_| {
        let eth = unsafe { &(*efm32gg11b820::ETH::ptr()) };
        let gpio = unsafe { &(*efm32gg11b820::GPIO::ptr()) };
        let int = eth.ifcr.read();

        if int.mngmntdone().bit_is_set() {
            eth.ifcr.write(|reg| reg.mngmntdone().set_bit());
        }
        if int.rxcmplt().bit_is_set() {
            eth.ifcr.write(|reg| reg.rxcmplt().set_bit());
            gpio.ph_dout.modify(|read, write| unsafe {
                write
                    .dout()
                    .bits((read.dout().bits() & !(0x07 << 13)) | (0x05 << 13))
            });
        }
        if int.rxoverrun().bit_is_set() {
            eth.ifcr.write(|reg| reg.rxoverrun().set_bit());
            gpio.ph_dout.modify(|read, write| unsafe {
                write
                    .dout()
                    .bits((read.dout().bits() & !(0x07 << 13)) | (0x04 << 13))
            })
        }
        if int.txcmplt().bit_is_set() {
            eth.ifcr.write(|reg| reg.txcmplt().set_bit());
            gpio.ph_dout.modify(|read, write| unsafe {
                write.dout().bits(read.dout().bits() | (0x07 << 10))
            });
        }
        if int.txunderrun().bit_is_set() {
            eth.ifcr.write(|reg| reg.txunderrun().set_bit());
            gpio.ph_dout.modify(|read, write| unsafe {
                write
                    .dout()
                    .bits((read.dout().bits() & !(0x07 << 10)) | (0x04 << 10))
            })
        }

        let int = eth.ifcr.read();
        if int.bits() != 0 {
            debug!("Unhandled interrupt (ETH): {:#X}", int.bits());
            eth.ifcr.write(|reg| unsafe { reg.bits(int.bits()) });
            gpio.ph_dout.modify(|read, write| unsafe {
                write
                    .dout()
                    .bits((read.dout().bits() & !(0x3F << 10)) | (0x3E << 10))
            })
        }
    });
}

impl<'a, 'b, 'c: 'b, P: PHY> phy::Device<'a> for EFM32GG<'b, 'c, P> {
    type RxToken = RxToken<'a>;
    type TxToken = TxToken<'a>;

    fn capabilities(&self) -> phy::DeviceCapabilities {
        let mut caps = phy::DeviceCapabilities::default();
        caps.max_transmission_unit = 1536;
        caps.checksum.icmpv4 = phy::Checksum::Both;
        caps.checksum.ipv4 = phy::Checksum::Both;
        caps.checksum.tcp = phy::Checksum::Both;
        caps.checksum.udp = phy::Checksum::Both;
        caps
    }

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        let (rx_start, rx_end) = match self.mac.find_rx_window() {
            Some((start, end)) => (start, end),
            None => return None,
        };

        let (tx_start, tx_len) = match self.mac.find_tx_window() {
            Some((start, len)) => (start, len),
            None => return None,
        };

        Some((
            RxToken {
                descriptors: self.mac.rx_buffer.descriptors_mut(),
                start: rx_start,
                end: rx_end,
            },
            TxToken {
                descriptors: self.mac.tx_buffer.descriptors_mut(),
                start: tx_start,
                len: tx_len,
            },
        ))
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        let (start, len) = match self.mac.find_tx_window() {
            Some((start, len)) => (start, len),
            None => return None,
        };

        Some(TxToken {
            descriptors: self.mac.tx_buffer.descriptors_mut(),
            start,
            len,
        })
    }
}

pub struct RxToken<'a> {
    descriptors: &'a mut [RxBufferDescriptor],
    start: usize,
    end: usize,
}

impl<'a> phy::RxToken for RxToken<'a> {
    fn consume<R, F>(self, _timestamp: time::Instant, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&[u8]) -> smoltcp::Result<R>,
    {
        let mut data = [0; 1536];

        let mut orig = self.start;
        let mut dest = 0;
        loop {
            {
                let d = &mut self.descriptors[orig];
                data[(dest * 128)..][..128].copy_from_slice(unsafe {
                    slice::from_raw_parts(d.address() as *const u8, 128)
                });
                d.release();
            }

            if orig == self.end {
                break;
            }

            orig = (orig + 1) % self.descriptors.len();
            dest += 1;
        }

        unsafe {
            (*efm32gg11b820::GPIO::ptr())
                .ph_dout
                .modify(|read, write| write.dout().bits(read.dout().bits() | (0x07 << 13)));
        };

        f(&data)
    }
}

pub struct TxToken<'a> {
    descriptors: &'a mut [TxBufferDescriptor],
    start: usize,
    len: usize,
}

impl<'a> phy::TxToken for TxToken<'a> {
    fn consume<R, F>(self, _timestamp: time::Instant, len: usize, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&mut [u8]) -> smoltcp::Result<R>,
    {
        if len > (self.len * 128) {
            return Err(Error::Exhausted);
        }

        debug_assert!(len > 0);
        let last_buffer = (len - 1) / 128;

        let mut data = [0; 1536];
        let result = f(&mut data[0..len])?;

        for i in 0..=last_buffer {
            let d = &mut self.descriptors[(self.start + i) % self.descriptors.len()];
            unsafe { slice::from_raw_parts_mut(d.address() as *mut u8, 128) }
                .copy_from_slice(&data[(i * 128)..][..128]);
            if i == 0 {
                d.set_length(len);
            }
            if i == last_buffer {
                d.set_last_buffer(true);
            }
            d.release();
        }

        unsafe {
            (*efm32gg11b820::ETH::ptr())
                .networkctrl
                .modify(|_, reg| reg.txstrt().set_bit());
        }

        unsafe {
            (*efm32gg11b820::GPIO::ptr()).ph_dout.modify(|read, write| {
                write
                    .dout()
                    .bits((read.dout().bits() & !(0x07 << 10)) | (0x05 << 10))
            });
        }

        Ok(result)
    }
}
