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

//! Supervisor call interface and implementation.
//!
//! Supervisor calls use the SVCall instruction to jump from apps into the supervisor. When this
//! instruction is executed, the ARMv7-M processor stacks the context (R0, R1, R2, R3, R12, LR
//! (R14), PC, and xPSR, in that order from the stack pointer down) and jumps to the handler. For
//! most calls, the exception handler simply branches to [`handle_procedure_call`] and treats the
//! call like any other AAPCS-compliant procedure. The [`TriggerEvent`](Procedure::TriggerEvent)
//! procedure requires some special handling, however.
//!
//! When [`TriggerEvent`](Procedure::TriggerEvent) is called, the handlers registered to that event
//! need to be called one-by-one, but from the context of the app. In order to do this, the stack is
//! manipulated to make it look as if a call was made to [`call_event_handlers`] instead of SVCall.

use core::cell::UnsafeCell;
use core::convert::TryFrom;
use core::{arch, ffi, fmt, mem};

use api::*;

/// Maximum number of handlers which can be triggered by a single event
///
/// Note that this value is chosen so that it, plus the null-terminator, take up 8 words on the
/// stack. This maintains the 8-byte stack alignment of the stacked frame.
pub const MAX_HANDLERS: usize = 7;

#[macro_export]
macro_rules! svcall {
    () => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn SVCall(id: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> u32 {
            core::arch::naked_asm!(
                // pass-through non TriggerEvent calls
                "movw r12, #:lower16:{0}",
                "movt r12, #:upper16:{0}",
                "cmp  r12, r0",
                "beq  handle_trigger_event",

                // call the generic handler
                "push {{ r4, lr }}", // push r4 to maintain alignment
                "bl   handle_procedure_call",
                "pop {{ r4, lr }}",

                // move the result into the stacked r0
                "str r0, [sp, #0]",
                "bx  lr",

                const api::TRIGGER_EVENT,
            )
        }
    };
}
pub use svcall;

#[unsafe(no_mangle)]
extern "C" fn print_trigger(id: EventIdent) {
    log::debug!("TriggerEvent({id:#06x})")
}

#[unsafe(no_mangle)]
extern "C" fn find_handler_by_id(id: EventIdent) -> Option<Handler> {
    STORE.find(id)
}

#[unsafe(no_mangle)]
extern "C" fn find_next_handler_entry_by_id(
    id: EventIdent,
    last: *const HandlerStoreEntry,
) -> *const HandlerStoreEntry {
    log::trace!(" looking for {id:#06x}, after {last:p}");
    match STORE.find_entry_after(id, last) {
        Some(entry) => entry,
        None => core::ptr::null(),
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn handler_from_handler_entry(entry: *const HandlerStoreEntry) -> Handler {
    let handler = unsafe { *(*entry).inner.get() }.unwrap().1;
    log::trace!("  found at {entry:p}: {handler:p}",);
    handler
}

#[unsafe(no_mangle)]
extern "C" fn print_handler(handler: *const Handler) {
    log::trace!("Calling Handler: {handler:p}")
}

#[unsafe(no_mangle)]
extern "C" fn print_handler_done(handler: *const Handler) {
    log::trace!("Called Handler: {handler:p}")
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

    fn find_entry_after(
        &self,
        id: EventIdent,
        last: *const HandlerStoreEntry,
    ) -> Option<&HandlerStoreEntry> {
        self.handlers
            .iter()
            .skip(if last == core::ptr::null() {
                0
            } else {
                usize::try_from(unsafe { last.offset_from(self.handlers.as_ptr()) }).unwrap() + 1
            })
            .filter_map(|entry| match entry.get() {
                Some((eid, _handler)) if eid == id => Some(entry),
                _ => None,
            })
            .next()
    }
}

unsafe impl Sync for HandlerStore {}

static STORE: HandlerStore = HandlerStore::new();

#[unsafe(no_mangle)]
unsafe extern "C" fn handle_procedure_call(id: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) -> u32 {
    let Some(args) = capture_call(id, arg0, arg1, arg2, arg3) else {
        log::warn!("ignoring API call ({id:#010x})");
        return 0;
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
            OpenSocket::TODO as u32
        }
        Args::RegisterHandler { event_id, handler } => match STORE.next_free() {
            Some(entry) => {
                log::debug!("RegisterHandler({event_id:#06x}, {handler:p}) @ {entry:p}");
                entry.set(event_id, handler);
                RegisterHandler::Registered as u32
            }
            None => {
                log::warn!("failed to register handler: no space");
                RegisterHandler::NoSpace as u32
            }
        },
        Args::PrintString { str } => match unsafe { ffi::CStr::from_ptr(str) }.to_str() {
            Ok(str) => {
                log::info!("{str}");
                PrintString::Printed as u32
            }
            Err(err) => {
                log::warn!("PrintString failed: {err:?}");
                PrintString::Failed as u32
            }
        },
        Args::ReturnValue { value } => value,
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
        Procedure::ReturnValue => Some(Args::ReturnValue { value: arg0 }),
    }
}

/// The first half of TriggerEvent, which manipulates the stack and returns from the exception
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn handle_trigger_event(_procedure: u32, event_id: u32) {
    core::arch::naked_asm!(
        "mov  r0, r1", // event id in r0
        "push {{ r0, lr }}",
        "bl   print_trigger",
        "pop  {{ r0, lr }}",

        // Manipulate the stack to match (addresses are high to low):
        //         TODO: is FP required here?
        //   +4                 LR
        //   +4                 null
        //   +(4*MAX_HANDLERS)  handlers
        //   (+4)               (optional padding)
        //   +32                exception frame
        //     +4                 xPSR
        //     +4                 PC
        //     +4                 LR
        //     +4                 R12
        //     +4                 R3
        //     +4                 R2
        //     +4                 R1
        //     +4                 R0

        "mrs r2, msp",
        "mov r12, r2", // reserve space for handlers, preserve original exception frame in r2
        "sub r12, r12, #(4*({0} + 1))", // a word for each handler, plus null
        "msr msp, r12",

        // r2  - original exception frame / handlers
        // r12 - current exception frame

        "ldr r1, [r2,  #(4*6)]", // move PC into R0, caller for call_event_handlers
        "orr r1, r1, #1",
        "str r1, [r12, #0]",

        "str r2, [r12, #4]", // move handlers into R1

        "push {{ r4-r8, lr }}",

        // TODO: add support for floating point
        "add   r2,  r2,  #8", // skip R0, R1; set above
        "add   r12, r12, #8",
        "ldmia r2,  {{ r3-r8 }}", // copy R2, R3, R12, LR, PC, and xPSR to new frame
        "stmia r12, {{ r3-r8 }}",
        "sub   r12, r12, #8",
        "sub   r2,  r2,  #8",

        "movw r3, #:lower16:call_event_handlers", // put call_event_handlers into PC
        "movt r3, #:upper16:call_event_handlers",
        "str  r3, [r12, #24]",

        // Stack up the exception handlers
        "mov r4, r2", // point r4 to the array of exception handlers
        "mov r5, #0", // last HandlerStoreEntry found
        "mov r6, #0", // found count
        "mov r7, r0", // event id into r7

        "1:",
        "mov r0, r7", // event id
        "mov r1, r5", // last handler entry
        "bl  find_next_handler_entry_by_id",
        "mov r5, r0", // last handler entry into r5
        "cmp r0, #0", // check null
        "beq  2f",

        "bl  handler_from_handler_entry", // save handler into array of handlers
        "str r0, [r4]",
        "add r4, r4, #4",

        "add r6, r6, #1", // check that MAX_HANDLERS isn't exceeded
        "cmp r6, #{0}",
        "blt 1b",
        "2:",

        // Null-terminate the array of exception handlers
        "mov r0, #0",
        "str r0, [r4]",

        // Return to call_event_handlers
        "pop {{ r4-r8, lr }}",
        "bx  lr",

        const MAX_HANDLERS,
    )
}

/// The second half of TriggerEvent, which calls all of the handlers
///
/// The caller must push the handlers, plus a null terminator, to the stack before calling. These
/// will be popped before returning to the given caller.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn call_event_handlers(caller: *const (), handlers: *const Handler) -> u32 {
    arch::naked_asm!(
        "push {{ r4-r7 }}", // push r7 to maintain alignment
        "mov  r4, r1", // handlers in r4
        "mov  r6, r0", // caller in r6

        "1:",
        "ldr r5, [r4]", // current handler in r5
        "cmp r5, #0", // check for null
        "beq 2f",

        "mov r0, r5", // handler in r0
        "bl  print_handler",

        "blx  r5",

        "mov r0, r5", // handler in r0
        "bl  print_handler_done",

        "add r4, r4, #4", // move to next handler
        "b   1b",

        "2:",
        "mov r1, r6", // caller in r1
        "pop {{ r4-r7 }}",

        "mrs r0,  msp", // pop the handlers and null terminator
        "add r0,  r0, #(4*({0} + 1))",
        "msr msp, r0",

        "mov r0, #0", // success
        "bx  r1", // return to SVCall caller

        const MAX_HANDLERS,
    )
}
