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

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::pin::Pin;
use core::{fmt, slice};

macro_rules! test_status_bit_fn {
    ($vis:vis $name:ident, $pos:literal) => {
        $vis fn $name(&self) -> bool {
            (unsafe { *self.status.get() } & (1 << $pos)) != 0
        }
    };
}

#[repr(align(4))]
pub struct RxRegion(pub [u8; 1536]);
#[repr(align(4))]
pub struct TxRegion(pub [u8; 1536]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BufferDescriptorOwnership {
    Software,
    Hardware,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BufferDescriptorListWrap {
    NoWrap,
    Wrap,
}

/// Transmit IP/TCP/UDP checksum generation offload errors
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TxChecksumGenerationError {
    /// The Packet was identified as a VLAN type, but the header was not fully complete, or had an
    /// error in it
    VlanBadHeader,
    /// The Packet was identified as a SNAP type, but the header was not fully complete, or had an
    /// error in it
    SnapBadHeader,
    /// The Packet was not of an IP type, or the IP packet was invalidly short, or the IP was not of
    /// type IPv4/IPv6
    IpBadPacket,
    /// The Packet was not identified as VLAN, SNAP or IP
    NotIdentified,
    /// Non supported packet fragmentation occurred. For IPv4 packets, the IP checksum was generated
    /// and inserted
    Fragmentation,
    /// Packet type detected was not TCP or UDP. TCP/UDP checksum was therefore not generated. For
    /// IPv4 packets, the IP checksum was generated and inserted
    NotTcpUdp,
    /// A premature end of packet was detected and the TCP/UDP checksum could not be generated
    EndOfPacket,
}

impl TxChecksumGenerationError {
    pub fn as_str(&self) -> &str {
        use TxChecksumGenerationError::*;
        match self {
            VlanBadHeader => "VLAN bad header",
            SnapBadHeader => "SNAP bad header",
            IpBadPacket => "IP bad packet",
            NotIdentified => "not VLAN, SNAP, or IP",
            Fragmentation => "non-supported packet fragmentation",
            NotTcpUdp => "not TCP or UDP",
            EndOfPacket => "premature end of packet",
        }
    }
}

pub trait BufferDescriptor {
    fn new(address: &mut [u8]) -> Self;
    fn end_of_list(self) -> Self;
    fn address(&self) -> u32;
    fn ownership(&self) -> BufferDescriptorOwnership;
    fn release(&mut self);
    fn wrapping(&self) -> BufferDescriptorListWrap;
    fn end_of_frame(&self) -> bool;
}

pub struct RxBuffer<'a> {
    descriptors: Pin<&'a mut RxDescriptors>,
    region: PhantomData<&'a mut RxRegion>,
}

impl<'a> RxBuffer<'a> {
    #[allow(clippy::identity_op, clippy::erasing_op)]
    pub fn new(
        mut region: Pin<&'a mut RxRegion>,
        mut descriptors: Pin<&'a mut RxDescriptors>,
    ) -> RxBuffer<'a> {
        descriptors.0[0] = RxBufferDescriptor::new(&mut region.0[128 * 0..][..128]);
        descriptors.0[1] = RxBufferDescriptor::new(&mut region.0[128 * 1..][..128]);
        descriptors.0[2] = RxBufferDescriptor::new(&mut region.0[128 * 2..][..128]);
        descriptors.0[3] = RxBufferDescriptor::new(&mut region.0[128 * 3..][..128]);
        descriptors.0[4] = RxBufferDescriptor::new(&mut region.0[128 * 4..][..128]);
        descriptors.0[5] = RxBufferDescriptor::new(&mut region.0[128 * 5..][..128]);
        descriptors.0[6] = RxBufferDescriptor::new(&mut region.0[128 * 6..][..128]);
        descriptors.0[7] = RxBufferDescriptor::new(&mut region.0[128 * 7..][..128]);
        descriptors.0[8] = RxBufferDescriptor::new(&mut region.0[128 * 8..][..128]);
        descriptors.0[9] = RxBufferDescriptor::new(&mut region.0[128 * 9..][..128]);
        descriptors.0[10] = RxBufferDescriptor::new(&mut region.0[128 * 10..][..128]);
        descriptors.0[11] = RxBufferDescriptor::new(&mut region.0[128 * 11..][..128]).end_of_list();

        RxBuffer {
            descriptors,
            region: PhantomData,
        }
    }

    pub fn descriptors(&self) -> &[RxBufferDescriptor] {
        &self.descriptors.0
    }

    pub fn descriptors_mut(&mut self) -> &mut [RxBufferDescriptor] {
        &mut self.descriptors.0
    }

    pub fn address(&self) -> *const RxBufferDescriptor {
        self.descriptors.0.as_ptr()
    }
}

pub struct RxDescriptors([RxBufferDescriptor; 12]);

impl RxDescriptors {
    pub const fn new() -> RxDescriptors {
        RxDescriptors([
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
            RxBufferDescriptor {
                address: UnsafeCell::new(0),
                status: UnsafeCell::new(0),
            },
        ])
    }
}

#[repr(C, align(8))]
pub struct RxBufferDescriptor {
    address: UnsafeCell<u32>,
    status: UnsafeCell<u32>,
}

impl fmt::Debug for RxBufferDescriptor {
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

impl BufferDescriptor for RxBufferDescriptor {
    fn new(address: &mut [u8]) -> RxBufferDescriptor {
        debug_assert!((address.as_ptr() as u32).trailing_zeros() >= 2);

        RxBufferDescriptor {
            address: UnsafeCell::new(
                address.as_ptr() as u32
                    | RxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::NoWrap)
                    | RxBufferDescriptor::ownership_to_word(BufferDescriptorOwnership::Hardware),
            ),
            status: UnsafeCell::new(0),
        }
    }

    fn end_of_list(self) -> RxBufferDescriptor {
        RxBufferDescriptor {
            address: UnsafeCell::new(
                unsafe { *self.address.get() }
                    | RxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::Wrap),
            ),
            status: self.status,
        }
    }

    fn address(&self) -> u32 {
        unsafe { (*self.address.get()) & 0xFFFF_FFFC }
    }

    fn ownership(&self) -> BufferDescriptorOwnership {
        RxBufferDescriptor::ownership_from_word(unsafe { *self.address.get() })
    }

    fn release(&mut self) {
        self.address = UnsafeCell::new(
            self.address()
                | RxBufferDescriptor::wrapping_to_word(self.wrapping())
                | RxBufferDescriptor::ownership_to_word(BufferDescriptorOwnership::Hardware),
        )
    }

    fn wrapping(&self) -> BufferDescriptorListWrap {
        RxBufferDescriptor::wrapping_from_word(unsafe { *self.address.get() })
    }

    test_status_bit_fn!(end_of_frame, 15);
}

impl RxBufferDescriptor {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.address() as *const u8, 128) }
    }

    test_status_bit_fn!(pub start_of_frame, 14);

    fn ownership_from_word(byte: u32) -> BufferDescriptorOwnership {
        match byte & 0x0000_0001 {
            0 => BufferDescriptorOwnership::Hardware,
            _ => BufferDescriptorOwnership::Software,
        }
    }

    fn ownership_to_word(ownership: BufferDescriptorOwnership) -> u32 {
        match ownership {
            BufferDescriptorOwnership::Hardware => 0x0000_0000,
            BufferDescriptorOwnership::Software => 0x0000_0001,
        }
    }

    fn wrapping_from_word(byte: u32) -> BufferDescriptorListWrap {
        match byte & 0x0000_0002 {
            0 => BufferDescriptorListWrap::NoWrap,
            _ => BufferDescriptorListWrap::Wrap,
        }
    }

    fn wrapping_to_word(wrapping: BufferDescriptorListWrap) -> u32 {
        match wrapping {
            BufferDescriptorListWrap::NoWrap => 0x0000_0000,
            BufferDescriptorListWrap::Wrap => 0x0000_0002,
        }
    }
}

pub struct TxBuffer<'a> {
    descriptors: Pin<&'a mut TxDescriptors>,
    region: PhantomData<&'a mut TxRegion>,
}

impl<'a> TxBuffer<'a> {
    #[allow(clippy::identity_op, clippy::erasing_op)]
    pub fn new(
        mut region: Pin<&'a mut TxRegion>,
        mut descriptors: Pin<&'a mut TxDescriptors>,
    ) -> TxBuffer<'a> {
        descriptors.0[0] = TxBufferDescriptor::new(&mut region.0[128 * 0..][..128]);
        descriptors.0[1] = TxBufferDescriptor::new(&mut region.0[128 * 1..][..128]);
        descriptors.0[2] = TxBufferDescriptor::new(&mut region.0[128 * 2..][..128]);
        descriptors.0[3] = TxBufferDescriptor::new(&mut region.0[128 * 3..][..128]);
        descriptors.0[4] = TxBufferDescriptor::new(&mut region.0[128 * 4..][..128]);
        descriptors.0[5] = TxBufferDescriptor::new(&mut region.0[128 * 5..][..128]);
        descriptors.0[6] = TxBufferDescriptor::new(&mut region.0[128 * 6..][..128]);
        descriptors.0[7] = TxBufferDescriptor::new(&mut region.0[128 * 7..][..128]);
        descriptors.0[8] = TxBufferDescriptor::new(&mut region.0[128 * 8..][..128]);
        descriptors.0[9] = TxBufferDescriptor::new(&mut region.0[128 * 9..][..128]);
        descriptors.0[10] = TxBufferDescriptor::new(&mut region.0[128 * 10..][..128]);
        descriptors.0[11] = TxBufferDescriptor::new(&mut region.0[128 * 11..][..128]).end_of_list();

        TxBuffer {
            descriptors,
            region: PhantomData,
        }
    }

    pub fn descriptors_mut(&mut self) -> &mut [TxBufferDescriptor] {
        &mut self.descriptors.0
    }

    pub fn address(&self) -> *const TxBufferDescriptor {
        self.descriptors.0.as_ptr()
    }
}

pub struct TxDescriptors([TxBufferDescriptor; 12]);

impl TxDescriptors {
    pub const fn new() -> TxDescriptors {
        TxDescriptors([
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
            TxBufferDescriptor {
                address: 0,
                status: UnsafeCell::new(0),
            },
        ])
    }
}

#[repr(C, align(8))]
pub struct TxBufferDescriptor {
    address: u32,
    status: UnsafeCell<u32>,
}

impl fmt::Debug for TxBufferDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Descriptor {{ {:#10X} {:#10X} }}",
            self.address,
            unsafe { *self.status.get() },
        )
    }
}

impl BufferDescriptor for TxBufferDescriptor {
    fn new(address: &mut [u8]) -> TxBufferDescriptor {
        TxBufferDescriptor {
            address: address.as_ptr() as u32,
            status: UnsafeCell::new(
                TxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::NoWrap)
                    | TxBufferDescriptor::ownership_to_word(BufferDescriptorOwnership::Software),
            ),
        }
    }

    fn end_of_list(self) -> TxBufferDescriptor {
        TxBufferDescriptor {
            address: self.address,
            status: UnsafeCell::new(
                unsafe { *self.status.get() }
                    | TxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::Wrap),
            ),
        }
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn ownership(&self) -> BufferDescriptorOwnership {
        TxBufferDescriptor::ownership_from_word(unsafe { *self.status.get() })
    }

    fn release(&mut self) {
        self.status = UnsafeCell::new(
            unsafe { *self.status.get() }
                & !Self::ownership_to_word(BufferDescriptorOwnership::Software),
        );
    }

    fn wrapping(&self) -> BufferDescriptorListWrap {
        TxBufferDescriptor::wrapping_from_word(unsafe { *self.status.get() })
    }

    test_status_bit_fn!(end_of_frame, 15);
}

impl TxBufferDescriptor {
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.address() as *mut u8, 128) }
    }

    pub fn length(&self) -> usize {
        ((unsafe { *self.status.get() }) & 0x0000_3FFF) as usize
    }

    pub fn set_length(&mut self, length: usize) {
        self.status = UnsafeCell::new(
            (unsafe { *self.status.get() } & !0x0000_3FFF) | (length as u32 & 0x0000_3FFF),
        );
    }

    pub fn set_last_buffer(&mut self, last: bool) {
        self.status = UnsafeCell::new(
            (unsafe { *self.status.get() } & !0x0000_8000)
                | if last { 0x0000_8000 } else { 0x0000_0000 },
        );
    }

    pub fn claim(&mut self) {
        self.status = UnsafeCell::new(
            unsafe { *self.status.get() }
                | Self::ownership_to_word(BufferDescriptorOwnership::Software),
        );
    }

    test_status_bit_fn!(pub error_retry_limit, 29);
    test_status_bit_fn!(pub error_tx_underrun, 28);
    test_status_bit_fn!(pub error_frame_corrupt, 27);
    test_status_bit_fn!(pub error_late_collision, 26);

    pub fn error_checksum_generation(&self) -> Option<TxChecksumGenerationError> {
        use TxChecksumGenerationError::*;
        match (unsafe { *self.status.get() } & (0b111 << 20)) {
            0b001 => Some(VlanBadHeader),
            0b010 => Some(SnapBadHeader),
            0b011 => Some(IpBadPacket),
            0b100 => Some(NotIdentified),
            0b101 => Some(Fragmentation),
            0b110 => Some(NotTcpUdp),
            0b111 => Some(EndOfPacket),
            _ => None,
        }
    }

    fn ownership_from_word(byte: u32) -> BufferDescriptorOwnership {
        match byte & 0x8000_0000 {
            0 => BufferDescriptorOwnership::Hardware,
            _ => BufferDescriptorOwnership::Software,
        }
    }

    fn ownership_to_word(ownership: BufferDescriptorOwnership) -> u32 {
        match ownership {
            BufferDescriptorOwnership::Hardware => 0x0000_0000,
            BufferDescriptorOwnership::Software => 0x8000_0000,
        }
    }

    fn wrapping_from_word(byte: u32) -> BufferDescriptorListWrap {
        match byte & 0x4000_0000 {
            0 => BufferDescriptorListWrap::NoWrap,
            _ => BufferDescriptorListWrap::Wrap,
        }
    }

    fn wrapping_to_word(wrapping: BufferDescriptorListWrap) -> u32 {
        match wrapping {
            BufferDescriptorListWrap::NoWrap => 0x0000_0000,
            BufferDescriptorListWrap::Wrap => 0x4000_0000,
        }
    }
}
