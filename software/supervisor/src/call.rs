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
use core::{arch, ffi, fmt, mem};

use api::*;


/// Maximum number of handlers which can be triggered by a single event
///
/// Note that this value is chosen so that it, plus the null-terminator and the stacked LR, take up
/// 8 words on the stack. This maintains the 8-byte stack alignment of the stacked frame.
pub const MAX_HANDLERS: usize = 6;

#[macro_export]
macro_rules! svcall {
    () => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn SVCall(id: u32, arg0: u32, arg1: u32, arg2: u32, arg3: u32) {
            core::arch::naked_asm!(
                // Pass-through non TriggerEvent calls
                "movw r12, #:lower16:{0}",
                "movt r12, #:upper16:{0}",
                "cmp  r12, r0",
                "bne  handle",

                "mov  r0, r1", // event id in r0
                "push {{ r0, lr }}",
                "bl   print_trigger",
                "pop  {{ r0, lr }}",

                // Manipulate the stack
                //   +1                 LR
                //   +1                 null
                //   +(4*MAX_HANDLERS)  handlers
                //   (+4)               (optional padding)
                //   +32                exception frame
                //     +4                 xPSR
                //     +4                 pc
                //     +4                 lr
                //     +4                 r12
                //     +4                 r3
                //     +4                 r2
                //     +4                 r1
                //     +4                 r0
                "mrs r12, msp", // TODO move the stacked exception frame down MAX_HANDLERS + 2 (null terminator, saved LR)

                // TODO: implement support for FP extension
                "ldr   r1,  [r12, #28]", // check for alignment padding, assign frame size to r3
                "tst   r1,  #(1 << 9)", // check xPSR.STKALIGN, 0x0200 means the padding was applied
                "ite   ne",
                "movne r3, #(4*8 + 4)",
                "moveq r3, #(4*8)",

                "mov r2, r12", // save address of original exception frame to r2
                "sub r12, r12, #(4*({1} + 2))", // reserve space for the handlers
                "msr msp, r12",

                "add r1, r3, r12", // move handlers into R0
                "str r1, [r12, #0]",

                // r0  - event id
                // r1  - beginning of handlers
                // r2  - original exception frame
                // r12 - current exception frame
                // r1 and r2 may differ by one word, depending on exception frame stack alignment

                "push {{ r4-r9, lr }}",

                "add   r2,  r2,  #4", // skip R0, was set above
                "add   r12, r12, #4",
                "ldmia r2,  {{ r3-r9 }}", // copy R1, R2, R3, R12, LR, PC, and xPSR to new frame
                "stmia r12, {{ r3-r9 }}",
                "sub   r12, r12, #4",

                "ldr r3, [r12, #20]", // move LR above exception frame and handlers
                "str r3, [r1,  #(4*({1} + 1))]",

                "ldr r3, [r12, #24]", // put PC into LR
                "str r3, [r12, #20]",

                "movw r3, #:lower16:call_event_handlers", // put call_event_handlers into PC
                "movt r3, #:upper16:call_event_handlers",
                "str  r3, [r12, #24]",

                // Stack up the exception handlers
                "mov r4, r1", // point r4 to the array of exception handlers
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
                "cmp r6, #{1}",
                "blt 1b",
                "2:",

                // Null-terminate the array of exception handlers
                "mov r0, #0",
                "str r0, [r4]",

                // Return to call_event_handlers
                "pop {{ r4-r9, lr }}",
                "bx lr",

                const api::TRIGGER_EVENT,
                const poe::call::MAX_HANDLERS,
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

/// The second half of SVCall for TriggerEvent, which calls all of the handlers
///
/// Note that the *caller* must push LR onto the stack before calling this function. This is needed
/// to allow a simple branch to be used (as opposed to branch-and-link), which is effectively what
/// happens when `SVC` is used. The array of handlers must be null-terminated.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn call_event_handlers(handlers: *const Handler) {
    arch::naked_asm!(
        "1:",
        "ldr r1, [r0]", // check for null
        "cmp r1, #0",
        "beq 2f",

        "push {{ r0 }}", // save handlers pointer
        "mov  r0, r1", // handler in r0

        "push {{ r0 }}",
        "bl   print_handler",
        "pop  {{ r0 }}",

        "push {{ r0 }}",
        "blx  r0",
        "pop  {{ r0 }}",

        "push {{ r0 }}",
        "bl   print_handler_done",
        "pop  {{ r0 }}",

        "pop {{ r0 }}",
        "add r0, r0, #4",
        "b   1b",

        "2:",
        "mrs r0,  msp", // pop the handlers and null terminator
        "add r0,  r0, #(4*({0} + 1))",
        "msr msp, r0",

        "pop {{ lr }}", // pop the specially-saved LR
        "mov r0, #0",
        "bx  lr",
        const MAX_HANDLERS,
    )
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
            .skip(
                if last == core::ptr::null() {
                    0
                } else {
                    usize::try_from(unsafe { last.offset_from(self.handlers.as_ptr()) }).unwrap()
                        + 1
                }, // last.and_then(|last| {
                   //     usize::try_from(unsafe { last.offset_from(self.handlers.as_ptr()) }).ok()
                   // })
                   // .unwrap_or(0),
            )
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
