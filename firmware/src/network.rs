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

use core::str::FromStr;
use led::rgb::Color;
use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::{Dhcpv4Event, Dhcpv4Socket, TcpSocket};
use smoltcp::wire::{IpCidr, Ipv4Address, Ipv4Cidr};

pub struct Resources {
    pub interface: Interface<'static, EFM32GG<'static, KSZ8091>>,
    pub http_handle: SocketHandle,
    pub websocket_handle: SocketHandle,
    pub dhcp_handle: SocketHandle,
}

enum Method {
    Get,
    Post,
    Unknown,
}

impl Resources {
    pub fn handle_sockets(&mut self, led: &mut dyn led::rgb::RGB) {
        self.handle_www(led);
        self.handle_dhcp();
    }

    fn handle_www(&mut self, led: &mut dyn led::rgb::RGB) {
        let socket = self.interface.get_socket::<TcpSocket>(self.http_handle);
        if !socket.is_open() {
            socket.listen(80).unwrap();
        }

        if socket.can_recv() && socket.can_send() {
            match socket
                .recv(|b| {
                    if b.starts_with(b"GET /") {
                        (b.len(), Method::Get)
                    } else if b.starts_with(b"POST /value") {
                        fn convert(bytes: &[u8]) -> Option<Color> {
                            let hex = unsafe { core::str::from_utf8_unchecked(bytes) };
                            let red = !u8::from_str(&hex[0..2]).ok()?;
                            let green = !u8::from_str(&hex[2..4]).ok()?;
                            let blue = !u8::from_str(&hex[4..6]).ok()?;

                            Some(Color { red, green, blue })
                        }

                        match convert(&b[b.len() - 6..]) {
                            Some(color) => {
                                led.set(color);
                                (b.len(), Method::Post)
                            }
                            None => (b.len(), Method::Unknown),
                        }
                    } else {
                        (b.len(), Method::Unknown)
                    }
                })
                .unwrap()
            {
                Method::Get => {
                    let header = include_bytes!(concat!(env!("OUT_DIR"), "/index-200.txt"));
                    let html = include_bytes!(concat!(env!("OUT_DIR"), "/index.html"));
                    socket
                        .send(|b| {
                            b[0..header.len()].copy_from_slice(header);
                            b[header.len()..][..html.len()].copy_from_slice(html);

                            (header.len() + html.len(), ())
                        })
                        .unwrap();
                    socket.close();
                }
                Method::Post => {
                    let header = include_bytes!(concat!(env!("OUT_DIR"), "/value-200.txt"));
                    socket
                        .send(|b| {
                            b[0..header.len()].copy_from_slice(header);

                            (header.len(), ())
                        })
                        .unwrap();
                    socket.close();
                }
                Method::Unknown => {
                    let header = include_bytes!(concat!(env!("OUT_DIR"), "/400.txt"));
                    socket
                        .send(|b| {
                            b[0..header.len()].copy_from_slice(header);

                            (header.len(), ())
                        })
                        .unwrap();
                    socket.close();
                }
            }
        }
    }

    fn handle_dhcp(&mut self) {
        let iface = &mut self.interface;
        match iface.get_socket::<Dhcpv4Socket>(self.dhcp_handle).poll() {
            None => {}
            Some(Dhcpv4Event::Configured(config)) => {
                log::debug!("DHCP config acquired");

                log::info!("IP address: {}", config.address);
                iface.update_ip_addrs(|addrs| addrs[0] = IpCidr::Ipv4(config.address));

                if let Some(router) = config.router {
                    log::debug!("Default gateway: {}", router);
                    iface.routes_mut().add_default_ipv4_route(router).unwrap();
                } else {
                    log::debug!("Default gateway: None");
                    iface.routes_mut().remove_default_ipv4_route();
                }

                for (i, s) in config.dns_servers.iter().enumerate() {
                    if let Some(s) = s {
                        log::debug!("DNS server {}:    {}", i, s);
                    }
                }
            }
            Some(Dhcpv4Event::Deconfigured) => {
                log::debug!("DHCP config lost");
                iface.update_ip_addrs(|addrs| {
                    addrs[0] = IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0))
                });
                iface.routes_mut().remove_default_ipv4_route();
            }
        }
    }
}
