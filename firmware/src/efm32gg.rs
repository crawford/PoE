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

use core::cell::UnsafeCell;
use core::fmt::{self, Write};
use cortex_m::{asm, interrupt};
use cortex_m_semihosting::hio;
use efm32gg11b820::{self, Interrupt, CMU, ETH, GPIO, NVIC};
use smoltcp::{self, phy, time};

pub struct MAC<'a> {
    rx_buffer: Buffer<'a>,
    tx_buffer: Buffer<'a>,
}

impl<'a> MAC<'a> {
    // TODO: This should probably accept a PHY so that this doesn't need to understand how to
    // configure the PHY.
    pub fn new(rx_buffer: Buffer<'a>, tx_buffer: Buffer<'a>) -> MAC<'a> {
        MAC {
            rx_buffer,
            tx_buffer,
        }
    }

    /// This assumes that the PHY will be interfaced via RMII with the EFM providing the ethernet
    /// clock.
    pub fn configure(&self, eth: &ETH, cmu: &CMU, gpio: &GPIO, nvic: &mut NVIC) {
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
            reg.txpbufsize().set_bit();
            reg.rxpbufsize().size3();
            reg
        });

        // Set the RX buffer descriptor queue address
        eth.rxqptr.write(|reg| unsafe {
            reg.dmarxqptr()
                .bits(self.rx_buffer.descriptor_list.as_ptr() as u32 >> 2)
        });

        // Set the hardware address filter, starting with the bottom register first
        eth.specaddr1bottom
            .write(|reg| unsafe { reg.addr().bits(0x00_00_00_02) });
        eth.specaddr1top
            .write(|reg| unsafe { reg.addr().bits(0x00_00_02_00) });

        // Clear pending interrupts
        nvic.clear_pending(Interrupt::ETH);
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

        // Enable receiving and the management interface
        eth.networkctrl.write(|reg| {
            reg.enbrx().set_bit();
            reg.manporten().set_bit();
            reg
        });

        // Release the PHY reset
        gpio.ph_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() | (1 << 7)) });

        // Enable the global clock
        eth.ctrl.write(|reg| reg.gblclken().set_bit());

        // XXX: Wait for the PHY
        for _ in 0..1000 {
            asm::nop();
        }

        let oui = (self.miim_read(eth, 0x00, 0x02) as u32) << 6
            | (self.miim_read(eth, 0x00, 0x03) as u32) >> 10;

        writeln!(
            hio::hstdout().unwrap(),
            "OUI: {:02X}:{:02X}:{:02X}",
            (oui >> 16) as u8,
            (oui >> 8) as u8,
            oui as u8,
        ).unwrap();
    }

    fn miim_read(&self, eth: &ETH, address: u8, register: u8) -> u16 {
        eth.phymngmnt.write(|reg| {
            unsafe { reg.phyaddr().bits(address) };
            unsafe { reg.phyrwdata().bits(0x00) };
            unsafe { reg.regaddr().bits(register) };
            unsafe { reg.operation().bits(0b10) };

            unsafe { reg.write10().bits(0b10) };
            reg.write1().set_bit();
            reg.write0().clear_bit();
            reg
        });

        while eth.networkstatus.read().mandone().bit_is_clear() {}

        eth.phymngmnt.read().phyrwdata().bits()
    }

    fn miim_write(&self, eth: &ETH, address: u8, register: u8, data: u16) {
        eth.phymngmnt.write(|reg| {
            unsafe { reg.phyaddr().bits(address) };
            unsafe { reg.phyrwdata().bits(data) };
            unsafe { reg.regaddr().bits(register) };
            unsafe { reg.operation().bits(0b01) };

            unsafe { reg.write10().bits(0b10) };
            reg.write1().set_bit();
            reg.write0().clear_bit();
            reg
        });

        while eth.networkstatus.read().mandone().bit_is_clear() {}
    }

    pub fn process(&mut self) {
        self.rx_buffer
            .descriptor_list
            .iter_mut()
            .filter(|d| {
                d.ownership() == BufferDescriptorOwnership::Software
                    && d.wrapping() != BufferDescriptorListWrap::Wrap
            })
            .for_each(|d| d.release());
    }

    pub fn isr(&mut self) {
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
                        .bits((read.dout().bits() & !(0x3F << 10)) | (0x3D << 10))
                });
            }
            if int.rxoverrun().bit_is_set() {
                eth.ifcr.write(|reg| reg.rxoverrun().set_bit());
                gpio.ph_dout.modify(|read, write| unsafe {
                    write
                        .dout()
                        .bits((read.dout().bits() & !(0x3F << 10)) | (0x3C << 10))
                })
            }
            if eth.ifcr.read().bits() != 0 {
                eth.ifcr.write(|reg| reg.respnotok().set_bit());
                gpio.ph_dout.modify(|read, write| unsafe {
                    write
                        .dout()
                        .bits((read.dout().bits() & !(0x3F << 10)) | (0x3E << 10))
                })
            }
        });
    }
}

impl<'a, 'b> phy::Device<'a> for MAC<'b> {
    type RxToken = RxToken<'a>;
    type TxToken = TxToken<'a>;

    fn capabilities(&self) -> phy::DeviceCapabilities {
        let mut caps = phy::DeviceCapabilities::default();
        //caps.max_transmission_unit = self.tx.len();
        caps.checksum.icmpv4 = phy::Checksum::Both;
        caps.checksum.ipv4 = phy::Checksum::Both;
        caps.checksum.tcpv4 = phy::Checksum::Both;
        caps.checksum.udpv4 = phy::Checksum::Both;
        caps
    }

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        //Some((RxToken{ buffer: self.rx }, TxToken{ buffer: self.tx }))
        None
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        //Some(TxToken{ buffer: self.tx })
        None
    }
}

pub struct RxToken<'a> {
    buffer: &'a [u8],
}

impl<'a> phy::RxToken for RxToken<'a> {
    fn consume<R, F>(self, _timestamp: time::Instant, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&[u8]) -> smoltcp::Result<R>,
    {
        f(&self.buffer[..])
    }
}

pub struct TxToken<'a> {
    buffer: &'a [u8],
}

impl<'a> phy::TxToken for TxToken<'a> {
    fn consume<R, F>(self, _timestamp: time::Instant, _len: usize, _f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&mut [u8]) -> smoltcp::Result<R>,
    {
        panic!("Not implemented")
    }
}

pub struct Buffer<'a> {
    data: UnsafeCell<&'a mut [u8; 128 * 16]>,
    descriptor_list: [BufferDescriptor; 17],
}

impl<'a> Buffer<'a> {
    pub fn new(data: &'a mut [u8; 128 * 16]) -> Buffer<'a> {
        Buffer {
            descriptor_list: [
                BufferDescriptor::new(&mut data[128 * 0] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 1] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 2] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 3] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 4] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 5] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 6] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 7] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 8] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 9] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 10] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 11] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 12] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 13] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 14] as *mut u8),
                BufferDescriptor::new(&mut data[128 * 15] as *mut u8),
                BufferDescriptor::end_of_list(),
            ],
            data: UnsafeCell::new(data),
        }
    }
}

#[repr(C, align(8))]
struct BufferDescriptor {
    address: UnsafeCell<u32>,
    _status: UnsafeCell<u32>,
}

impl fmt::Debug for BufferDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Descriptor {{ {:#10X} {:?} {:?} }}",
            self.address(),
            self.ownership(),
            self.wrapping()
        )
    }
}

impl BufferDescriptor {
    fn new(address: *mut u8) -> BufferDescriptor {
        debug_assert!((address as u32 & 0x00000003) == 0);

        BufferDescriptor {
            address: UnsafeCell::new(
                address as u32 | BufferDescriptorListWrap::NoWrap.to_word()
                    | BufferDescriptorOwnership::Hardware.to_word(),
            ),
            _status: UnsafeCell::new(0),
        }
    }

    fn end_of_list() -> BufferDescriptor {
        BufferDescriptor {
            address: UnsafeCell::new(
                BufferDescriptorListWrap::Wrap.to_word()
                    | BufferDescriptorOwnership::Software.to_word(),
            ),
            _status: UnsafeCell::new(0),
        }
    }

    fn wrapping(&self) -> BufferDescriptorListWrap {
        BufferDescriptorListWrap::from_word(unsafe { *self.address.get() })
    }

    fn ownership(&self) -> BufferDescriptorOwnership {
        BufferDescriptorOwnership::from_word(unsafe { *self.address.get() })
    }

    fn address(&self) -> u32 {
        unsafe { (*self.address.get()) & 0xFFFFFFFC }
    }

    fn release(&mut self) {
        self.address = UnsafeCell::new(
            self.address() | self.wrapping().to_word()
                | BufferDescriptorOwnership::Hardware.to_word(),
        )
    }
}

#[derive(Debug, PartialEq)]
enum BufferDescriptorListWrap {
    NoWrap,
    Wrap,
}

impl BufferDescriptorListWrap {
    fn from_word(byte: u32) -> BufferDescriptorListWrap {
        match byte & 0x00000002 {
            0 => BufferDescriptorListWrap::NoWrap,
            _ => BufferDescriptorListWrap::Wrap,
        }
    }

    fn to_word(&self) -> u32 {
        match *self {
            BufferDescriptorListWrap::NoWrap => 0x00000000,
            BufferDescriptorListWrap::Wrap => 0x00000002,
        }
    }
}

#[derive(Debug, PartialEq)]
enum BufferDescriptorOwnership {
    Software,
    Hardware,
}

impl BufferDescriptorOwnership {
    fn from_word(byte: u32) -> BufferDescriptorOwnership {
        match byte & 0x00000001 {
            0 => BufferDescriptorOwnership::Hardware,
            _ => BufferDescriptorOwnership::Software,
        }
    }

    fn to_word(&self) -> u32 {
        match *self {
            BufferDescriptorOwnership::Hardware => 0x00000000,
            BufferDescriptorOwnership::Software => 0x00000001,
        }
    }
}
