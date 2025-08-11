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

use core::cell::UnsafeCell;
use core::convert::TryFrom;
use core::{ffi, fmt, mem};

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
enum Procedure {
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

enum Args {
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

#[macro_export]
macro_rules! svcall {
    () => {
        #[unsafe(naked)]
        extern "C" fn call(a: u32, b: u32, c: u32, d: u32, e: u32) {
            core::arch::naked_asm!("svc 0")
        }

        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn SVCall(id: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) {
            core::arch::naked_asm!(
                // Pass-through non TriggerEvent calls
                "movw r12, #:lower16:{0}",
                "movt r12, #:upper16:{0}",
                "cmp  r12, r0",
                "bne  handle",

                // Look up the exception handler
                "push {{ lr }}",
                "mov  r0, r1",
                "bl   find_handler_by_id",
                "pop  {{ lr }}",

                // Check for null
                "cmp r0,  #0",
                "bne 1f",

                "mov r0,  #1",  // return 1
                "mrs r12, msp",
                "str r0,  [r12, #0]",
                "bx  lr",

                "1:",

                // Manipulate the stack
                "mrs r12, msp", // zero registers
                "mov r1, #0",
                "str r1, [r12, #0]",  // R0
                "str r1, [r12, #4]",  // R1
                "str r1, [r12, #8]",  // R2
                "str r1, [r12, #12]", // R3
                "str r1, [r12, #16]", // R12

                "ldr r1, [r12, #24]", // put PC into LR
                "orr r1, r1, #1",
                "str r1, [r12, #20]",

                "str r0, [r12, #24]", // put handler into PC

                // Return to the event handler
                "bx lr",

                const poe::call::TRIGGER_EVENT,
            )
        }
    };
}
pub use svcall;

#[unsafe(no_mangle)]
extern "C" fn find_handler_by_id(id: EventIdent) -> Option<Handler> {
    STORE.find(id)
}

pub type EventIdent = u32;

struct HandlerStoreEntry {
    inner: UnsafeCell<Option<(EventIdent, Handler)>>,
}

impl HandlerStoreEntry {
    const fn new() -> Self {
        HandlerStoreEntry {
            inner: UnsafeCell::new(None),
        }
    }

    fn is_set(&self) -> bool {
        unsafe { *self.inner.get() }.is_some()
    }

    fn set(&self, id: EventIdent, handler: Handler) {
        unsafe { *self.inner.get() = Some((id, handler)) };
    }

    fn get(&self) -> Option<(EventIdent, Handler)> {
        unsafe { *self.inner.get() }
    }
}

impl fmt::Debug for HandlerStoreEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match unsafe { *self.inner.get() } {
            Some((id, handler)) => write!(f, "Handler: {id}, {handler:p}"),
            None => write!(f, "Handler: <free>"),
        }
    }
}

pub const HANDLERS_COUNT: usize = 32;

#[derive(Debug)]
struct HandlerStore {
    handlers: [HandlerStoreEntry; HANDLERS_COUNT],
}

impl HandlerStore {
    const fn new() -> Self {
        const DEFAULT: HandlerStoreEntry = HandlerStoreEntry::new();

        HandlerStore {
            handlers: [DEFAULT; HANDLERS_COUNT],
        }
    }

    fn next_free(&self) -> Option<&HandlerStoreEntry> {
        self.handlers.iter().find(|entry| !entry.is_set())
    }

    fn find(&self, id: EventIdent) -> Option<Handler> {
        self.handlers
            .iter()
            .filter_map(|entry| entry.get())
            .filter_map(move |(eid, handler)| if eid == id { Some(handler) } else { None })
            .next()
    }
}

unsafe impl Sync for HandlerStore {}

static STORE: HandlerStore = HandlerStore::new();

#[unsafe(no_mangle)]
extern "C" fn handle(id: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) {
    let Some(args) = capture_call(id, arg0, arg1, arg2, arg3) else {
        log::warn!("ignoring API call ({id:#010x})");
        return;
    };

    match args {
        Args::OpenSocket {
            remote_addr,
            remote_port,
            control_callback,
            data_callback,
        } => {
            log::info!(
                "OpenSocket({remote_addr:?}, {remote_port}, {control_callback:p} {data_callback:p})"
            );
        }
        Args::RegisterHandler { event_id, handler } => match STORE.next_free() {
            Some(entry) => {
                log::debug!("RegisterHandler({event_id:#06x}, {handler:p}) @ {entry:p}");
                let _ = entry.set(event_id, handler);
            }
            None => {
                log::warn!("failed to register handler: no space")
            }
        },
        Args::PrintString { str } => match unsafe { ffi::CStr::from_ptr(str) }.to_str() {
            Ok(str) => log::info!("{str}"),
            Err(err) => log::warn!("PrintString failed: {err:?}"),
        },
    }
}

fn capture_call(id: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> Option<Args> {
    match Procedure::try_from(id).ok()? {
        Procedure::OpenSocket => {
            let remote_addr = arg0.to_ne_bytes();
            let remote_port = arg1 as u16;
            let control_callback = unsafe { mem::transmute(arg2 as usize as *const ()) };
            let data_callback = unsafe { mem::transmute(arg3 as usize as *const ()) };

            Some(Args::OpenSocket {
                remote_addr,
                remote_port,
                control_callback,
                data_callback,
            })
        }
        Procedure::RegisterHandler => {
            let id: u32 = arg0;
            let func: u32 = arg1;

            Some(Args::RegisterHandler {
                event_id: id,
                handler: unsafe { core::mem::transmute(func) },
            })
        }
        Procedure::TriggerEvent => panic!("TriggerEvent failed"),
        Procedure::PrintString => {
            let str: u32 = arg0;

            Some(Args::PrintString {
                str: str as *const ffi::c_char,
            })
        }
    }
}
