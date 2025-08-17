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

use core::ffi;

#[repr(u32)]
#[non_exhaustive]
pub enum SocketEvent {
    Opened,
    Closed,
}

pub struct Socket {}
pub type SocketControlCallback = extern "C" fn(socket: *mut Socket, state: SocketEvent);
pub type SocketDataCallback = extern "C" fn(socket: *mut Socket, data: *const u8, len: usize);

pub type Handler = extern "C" fn();

pub const OPEN_SOCKET: u32 = Procedure::OpenSocket as u32;
pub const REGISTER_HANDLER: u32 = Procedure::RegisterHandler as u32;
pub const TRIGGER_EVENT: u32 = Procedure::TriggerEvent as u32;
pub const PRINT_STRING: u32 = Procedure::PrintString as u32;

#[repr(u32)]
pub enum Procedure {
    OpenSocket = 0x8BD6C7FF,
    RegisterHandler = 0xD35DBF5A,
    TriggerEvent = 0x65438A43,
    PrintString = 0x0A066986,
}

impl TryFrom<u32> for Procedure {
    type Error = ();

    fn try_from(id: u32) -> Result<Self, Self::Error> {
        match id {
            OPEN_SOCKET => Ok(Procedure::OpenSocket),
            REGISTER_HANDLER => Ok(Procedure::RegisterHandler),
            TRIGGER_EVENT => Ok(Procedure::TriggerEvent),
            PRINT_STRING => Ok(Procedure::PrintString),
            _ => Err(()),
        }
    }
}

pub enum Args {
    OpenSocket {
        remote_addr: [u8; 4],
        remote_port: u16,
        control_callback: SocketControlCallback,
        data_callback: SocketDataCallback,
    },
    RegisterHandler {
        event_id: u32,
        handler: Handler,
    },
    PrintString {
        str: *const ffi::c_char,
    },
}

#[inline(always)]
pub extern "C" fn open_socket(remote_addr: u32, remote_port: u16, control_callback: SocketControlCallback, data_callback: SocketDataCallback) {
    unsafe { core::arch::asm!(
        "push {}",

        "svc 0",

        "msr r0, msp",
        "add r0, r0, #4",
        "mrs r0, msp",

        in(reg) data_callback,
        in("r0") OPEN_SOCKET,
        in("r1") remote_addr,
        in("r2") remote_port,
        in("r3") control_callback,
        clobber_abi("C"),
    ) }
}

#[inline(always)]
pub extern "C" fn register_handler(event_id: u32, handler: Handler) {
    unsafe { core::arch::asm!(
        "svc 0",
        in("r0") REGISTER_HANDLER,
        in("r1") event_id,
        in("r2") handler,
        clobber_abi("C"),
    ) }
}

#[inline(always)]
pub extern "C" fn trigger_event(event_id: u32) {
    unsafe { core::arch::asm!(
        "svc 0",
        in("r0") TRIGGER_EVENT,
        in("r1") event_id,
        clobber_abi("C"),
    ) }
}

#[inline(always)]
pub extern "C" fn print_string(str: *const u8) {
    unsafe { core::arch::asm!(
        "svc 0",
        in("r0") PRINT_STRING,
        in("r1") str,
        clobber_abi("C"),
    ) }
}
