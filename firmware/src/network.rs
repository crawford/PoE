// Copyright 2021 Alex Crawford
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

use crate::efm32gg::EFM32GG;
use crate::ksz8091::KSZ8091;

use core::fmt::Write;
use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::TcpSocket;

pub struct Resources {
    pub interface: Interface<'static, EFM32GG<'static, KSZ8091>>,
    pub tcp_handle: SocketHandle,
}

impl Resources {
    pub fn handle_sockets(&mut self) {
        self.handle_tcp();
    }

    fn handle_tcp(&mut self) {
        let socket = self.interface.get_socket::<TcpSocket>(self.tcp_handle);
        if !socket.is_open() {
            socket.listen(6969).unwrap();
        }

        if socket.can_send() {
            log::debug!("tcp:6969 send greeting");
            writeln!(socket, "hello").unwrap();
            log::debug!("tcp:6969 close");
            socket.close();
        }
    }
}
