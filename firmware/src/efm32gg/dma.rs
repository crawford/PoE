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
use core::fmt;

#[repr(align(4))]
pub struct RxRegion(pub [u8; 1536]);
#[repr(align(4))]
pub struct TxRegion(pub [u8; 1536]);

#[derive(Debug, PartialEq)]
pub enum BufferDescriptorOwnership {
    Software,
    Hardware,
}

#[derive(Debug, PartialEq)]
pub enum BufferDescriptorListWrap {
    NoWrap,
    Wrap,
}

pub trait BufferDescriptor {
    fn new(address: *mut u8) -> Self;
    fn end_of_list(self) -> Self;
    fn address(&self) -> u32;
    fn ownership(&self) -> BufferDescriptorOwnership;
    fn release(&mut self);
    fn wrapping(&self) -> BufferDescriptorListWrap;
}

pub struct RxBuffer<'a> {
    data: UnsafeCell<&'a mut [u8; 128 * 12]>,
    descriptor_list: [RxBufferDescriptor; 12],
}

impl<'a> RxBuffer<'a> {
    pub fn new(data: &'a mut RxRegion) -> RxBuffer<'a> {
        RxBuffer {
            descriptor_list: [
                RxBufferDescriptor::new(&mut data.0[128 * 0] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 1] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 2] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 3] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 4] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 5] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 6] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 7] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 8] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 9] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 10] as *mut u8),
                RxBufferDescriptor::new(&mut data.0[128 * 11] as *mut u8).end_of_list(),
            ],
            data: UnsafeCell::new(&mut data.0),
        }
    }

    pub fn descriptors(&mut self) -> &[RxBufferDescriptor] {
        &self.descriptor_list
    }

    pub fn descriptors_mut(&mut self) -> &mut [RxBufferDescriptor] {
        &mut self.descriptor_list
    }

    pub fn address(&self) -> *const RxBufferDescriptor {
        self.descriptor_list.as_ptr()
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
    fn new(address: *mut u8) -> RxBufferDescriptor {
        debug_assert!((address as u32 & 0x0000_0003) == 0);

        RxBufferDescriptor {
            address: UnsafeCell::new(
                address as u32
                    | RxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::NoWrap)
                    | RxBufferDescriptor::ownership_to_word(BufferDescriptorOwnership::Hardware),
            ),
            status: UnsafeCell::new(0),
        }
    }

    fn end_of_list(mut self) -> RxBufferDescriptor {
        self.address = UnsafeCell::new(
            unsafe { *self.address.get() }
                | RxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::Wrap),
        );
        self
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
}

impl RxBufferDescriptor {
    pub fn start_of_frame(&self) -> bool {
        unsafe { (*self.status.get()) & 0x0000_4000 != 0 }
    }

    pub fn end_of_frame(&self) -> bool {
        unsafe { (*self.status.get()) & 0x0000_8000 != 0 }
    }

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
    data: UnsafeCell<&'a mut [u8; 768 * 2]>,
    descriptor_list: [TxBufferDescriptor; 2],
}

impl<'a> TxBuffer<'a> {
    pub fn new(data: &'a mut TxRegion) -> TxBuffer<'a> {
        TxBuffer {
            descriptor_list: [
                TxBufferDescriptor::new(&mut data.0[768 * 0] as *mut u8),
                TxBufferDescriptor::new(&mut data.0[768 * 1] as *mut u8).end_of_list(),
            ],
            data: UnsafeCell::new(&mut data.0),
        }
    }

    pub fn descriptors(&mut self) -> &[TxBufferDescriptor] {
        &self.descriptor_list
    }

    pub fn descriptors_mut(&mut self) -> &mut [TxBufferDescriptor] {
        &mut self.descriptor_list
    }

    pub fn address(&self) -> *const TxBufferDescriptor {
        self.descriptor_list.as_ptr()
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
    fn new(address: *mut u8) -> TxBufferDescriptor {
        TxBufferDescriptor {
            address: address as u32,
            status: UnsafeCell::new(
                TxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::NoWrap)
                    | TxBufferDescriptor::ownership_to_word(BufferDescriptorOwnership::Software),
            ),
        }
    }

    fn end_of_list(mut self) -> TxBufferDescriptor {
        self.status = UnsafeCell::new(
            unsafe { *self.status.get() }
                | TxBufferDescriptor::wrapping_to_word(BufferDescriptorListWrap::Wrap),
        );
        self
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn ownership(&self) -> BufferDescriptorOwnership {
        TxBufferDescriptor::ownership_from_word(unsafe { *self.status.get() })
    }

    fn release(&mut self) {
        // XXX: Improve this
        self.status = UnsafeCell::new(unsafe { *self.status.get() } & !0x8000_0000);
    }

    fn wrapping(&self) -> BufferDescriptorListWrap {
        TxBufferDescriptor::wrapping_from_word(unsafe { *self.status.get() })
    }
}

impl TxBufferDescriptor {
    pub fn set_length(&mut self, length: usize) {
        self.status =
            UnsafeCell::new((unsafe { *self.status.get() } & !0x0000_3FFF) | length as u32);
    }

    pub fn set_last_buffer(&mut self, last: bool) {
        self.status = UnsafeCell::new(
            (unsafe { *self.status.get() } & !0x0000_8000)
                | if last { 0x0000_8000 } else { 0x0000_0000 },
        );
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
