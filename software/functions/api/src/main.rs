#![no_std]
#![no_main]

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text._start")]
pub extern "C" fn _start() -> u32 {
    api::register_handler(0xABCD, handler);
    api::trigger_event(0xABCD)
}

extern "C" fn handler() {
    api::print_string("Hello, from Rust!".as_ptr());
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
