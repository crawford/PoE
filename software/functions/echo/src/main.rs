#![no_std]
#![no_main]

use core::{panic, ptr};

use api::{Bytes, OpenTcpSocketArgs, TcpSocketEvent};

const BUFFER_SIZE: usize = 32;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text._start")]
pub extern "C" fn _start() {
    static mut RX_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
    static mut TX_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

    let rx = ptr::addr_of_mut!(RX_BUFFER) as *mut u8;
    let tx = ptr::addr_of_mut!(TX_BUFFER) as *mut u8;

    api::open_tcp_socket(&OpenTcpSocketArgs {
        control_callback: control_handler,
        data_callback: data_handler,
        port: 3000,
        rx_bytes: Bytes::new(rx, BUFFER_SIZE),
        tx_bytes: Bytes::new(tx, BUFFER_SIZE),
    });
}

extern "C" fn control_handler(state: TcpSocketEvent) {
    match state {
        TcpSocketEvent::Opened => api::print_string(c"socket opened".as_ptr()),
        TcpSocketEvent::Closed => api::print_string(c"socket closed".as_ptr()),
        _ => api::print_string(c"unknown".as_ptr()),
    };
}

extern "C" fn data_handler(data: Bytes) -> Bytes {
    data
}

#[panic_handler]
fn panic(_: &panic::PanicInfo) -> ! {
    loop {}
}
