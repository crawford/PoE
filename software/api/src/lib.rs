// Copyright 2025 Alex Crawford
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

#![no_std]

use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use core::{ffi, fmt, mem};

#[repr(u32)]
#[non_exhaustive]
pub enum TcpSocketEvent {
    Opened,
    Closed,
}

/// Fat pointer to an array of bytes.
///
/// The lower half is the length, and the upper is a pointer to the data.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes(u64);
pub type BytesLength = usize;
pub type BytesData = *mut u8;

impl Bytes {
    #[inline(always)]
    pub fn new(data: BytesData, length: BytesLength) -> Self {
        Self((data as u64) << 32 | length as u64)
    }

    #[inline(always)]
    pub fn length(&self) -> BytesLength {
        (self.0 & 0xFFFF) as BytesLength
    }

    #[inline(always)]
    pub fn data(&self) -> BytesData {
        (self.0 >> 32) as BytesData
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &'static [u8] {
        unsafe { slice_from_raw_parts(self.data(), self.length()).as_ref_unchecked() }
    }

    #[inline(always)]
    pub fn as_mut_slice(&self) -> &'static mut [u8] {
        unsafe { slice_from_raw_parts_mut(self.data(), self.length()).as_mut_unchecked() }
    }
}

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Bytes")
            .field(&self.length())
            .field(&self.data())
            .finish()
    }
}

pub type TcpSocketControlCallback = extern "C" fn(state: TcpSocketEvent);
pub type TcpSocketDataCallback = extern "C" fn(data: Bytes) -> Bytes;

pub type Handler = extern "C" fn();

pub const OPEN_TCP_SOCKET: u32 = Procedure::OpenTcpSocket as u32;
pub const REGISTER_HANDLER: u32 = Procedure::RegisterHandler as u32;
pub const TRIGGER_EVENT: u32 = Procedure::TriggerEvent as u32;
pub const PRINT_STRING: u32 = Procedure::PrintString as u32;
pub const RETURN_VALUE: u32 = Procedure::ReturnValue as u32;

#[repr(u32)]
pub enum Procedure {
    OpenTcpSocket = 0x8BD6C7FF,
    RegisterHandler = 0xD35DBF5A,
    TriggerEvent = 0x65438A43,
    PrintString = 0x0A066986,
    ReturnValue = 0x1035E6E8,
}

impl TryFrom<u32> for Procedure {
    type Error = ();

    fn try_from(id: u32) -> Result<Self, Self::Error> {
        match id {
            OPEN_TCP_SOCKET => Ok(Procedure::OpenTcpSocket),
            REGISTER_HANDLER => Ok(Procedure::RegisterHandler),
            TRIGGER_EVENT => Ok(Procedure::TriggerEvent),
            PRINT_STRING => Ok(Procedure::PrintString),
            RETURN_VALUE => Ok(Procedure::ReturnValue),
            _ => Err(()),
        }
    }
}

pub enum Args {
    OpenTcpSocket(OpenTcpSocketArgs),
    RegisterHandler {
        event_id: u32,
        handler: Handler,
    },
    PrintString {
        str: *const ffi::c_char,
    },
    ReturnValue {
        value: u32,
    },
}

#[repr(u32)]
pub enum OpenTcpSocket {
    Queued = 0,
    NoSpace = 1,
}

#[repr(u32)]
pub enum RegisterHandler {
    Registered = 0,
    NoSpace = 1,
}

#[repr(u32)]
pub enum TriggerEvent {
    Triggered = 0,
}

#[repr(u32)]
pub enum PrintString {
    Printed = 0,
    Failed = 1,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OpenTcpSocketArgs {
    pub control_callback: TcpSocketControlCallback,
    pub data_callback: TcpSocketDataCallback,
    pub port: u16,
    pub rx_bytes: Bytes,
    pub tx_bytes: Bytes,
}

#[inline(always)]
pub extern "C" fn open_tcp_socket(args: &OpenTcpSocketArgs) -> OpenTcpSocket {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "svc 0",

            in("r0") OPEN_TCP_SOCKET,
            in("r1") args,

            lateout("r0") ret,

            clobber_abi("C"),
        )
    }
    unsafe { mem::transmute(ret) }
}

#[inline(always)]
pub extern "C" fn register_handler(event_id: u32, handler: Handler) -> RegisterHandler {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "svc 0",

            in("r0") REGISTER_HANDLER,
            in("r1") event_id,
            in("r2") handler,

            lateout("r0") ret,

            clobber_abi("C"),
        )
    }
    unsafe { mem::transmute(ret) }
}

#[inline(always)]
pub extern "C" fn trigger_event(event_id: u32) -> TriggerEvent {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "svc 0",

            in("r0") TRIGGER_EVENT,
            in("r1") event_id,

            lateout("r0") ret,

            clobber_abi("C"),
        )
    }
    unsafe { mem::transmute(ret) }
}

#[inline(always)]
pub extern "C" fn print_string(str: *const u8) -> PrintString {
    let ret: u32;
    unsafe {
        core::arch::asm!(
            "svc 0",

            in("r0") PRINT_STRING,
            in("r1") str,

            lateout("r0") ret,

            clobber_abi("C"),
        )
    }
    unsafe { mem::transmute(ret) }
}
