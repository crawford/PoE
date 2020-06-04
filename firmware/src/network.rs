// Copyright 2020 Alex Crawford
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

use crate::efm32gg::{self, dma};
use crate::ksz8091::KSZ8091;
use smoltcp::iface::{EthernetInterface, EthernetInterfaceBuilder, NeighborCache};
use smoltcp::socket::{SocketHandle, SocketSet};

pub struct ResourceBuilder {
    rx_buffer: dma::RxBuffer,
    tx_buffer: dma::TxBuffer,
    tcp_rx_payload: [u8; 128],
    tcp_tx_payload: [u8; 128],
}

impl ResourceBuilder {
    pub fn new(rx_buffer: dma::RxBuffer, tx_buffer: dma::TxBuffer) -> ResourceBuilder {
        ResourceBuilder {
            rx_buffer,
            tx_buffer,
            tcp_rx_payload: [0; 128],
            tcp_tx_payload: [0; 128],
        }
    }

    pub fn add_iface(
        self,
        interface: EthernetInterface<'static, 'static, 'static, efm32gg::EFM32GG<'static, KSZ8091>>,
    ) -> ResourceWithIfaceBuilder {
        ResourceWithIfaceBuilder {
            inner: self,
            iface: interface,
        }
    }
}

pub struct ResourceWithIfaceBuilder {
    inner: ResourceBuilder,
    iface: EthernetInterface<'static, 'static, 'static, efm32gg::EFM32GG<'static, KSZ8091>>,
}

impl<'a, 'b, 'c> ResourceWithIfaceBuilder {
    pub fn add_sockets(self, sockets: SocketSet<'a, 'b, 'c>) -> ResourceWithIfaceAndSocketsBuilder<'a, 'b, 'c> {
        ResourceWithIfaceAndSocketsBuilder {
            inner: self,
            sockets: sockets,
        }
    }
}

struct ResourceWithIfaceAndSocketsBuilder<'a, 'b, 'c> {
    inner: ResourceWithIfaceBuilder,
    sockets: SocketSet<'a, 'b, 'c>,
}

impl<'a, 'b, 'c> ResourceWithIfaceAndSocketsBuilder<'a, 'b, 'c> {
    pub fn add_tcp_handle(self, handle: smoltcp::socket::SocketHandle) -> Resources<'a, 'b, 'c> {
        Resources {
            iface: self.inner.iface,
            rx_buffer: self.inner.inner.rx_buffer,
            tx_buffer: self.inner.inner.tx_buffer,
            sockets: self.sockets,
            tcp_handle: handle,
        }
    }
}

pub struct Resources {
    iface: EthernetInterface<'static, 'static, 'static, efm32gg::EFM32GG<'static, KSZ8091>>,
    rx_buffer: dma::RxBuffer,
    tx_buffer: dma::TxBuffer,
    sockets: SocketSet,
    tcp_handle: SocketHandle,
}
