// Copyright 2021 Alex Crawford
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

use crate::efm32gg::EFM32GG;
use crate::ksz8091::KSZ8091;

use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::{Dhcpv4Event, Dhcpv4Socket, TcpSocket};
use smoltcp::wire::{IpCidr, Ipv4Address, Ipv4Cidr};

const CONTROL_PORT: u16 = 51900;

pub struct Resources {
    pub interface: Interface<'static, EFM32GG<'static, KSZ8091>>,
    pub dhcp_handle: SocketHandle,
    pub tcp_handle: SocketHandle,
}

#[derive(Clone, Copy, Debug)]
pub enum State {
    Uninit,
    NoLink,
    NoDhcp,
    NoGateway,
    Operational,
}

impl Resources {
    pub fn handle_sockets<D, I>(&mut self, dhcp: D, identify: I)
    where
        D: FnOnce(State),
        I: FnOnce(bool),
    {
        self.handle_dhcp(dhcp);
        self.handle_tcp(identify);
    }

    pub fn reset_dhcp(&mut self) {
        self.interface
            .get_socket::<Dhcpv4Socket>(self.dhcp_handle)
            .reset();
    }

    fn handle_dhcp<F: FnOnce(State)>(&mut self, dhcp: F) {
        let iface = &mut self.interface;
        match iface.get_socket::<Dhcpv4Socket>(self.dhcp_handle).poll() {
            None => {}
            Some(Dhcpv4Event::Configured(config)) => {
                log::debug!("DHCP config acquired");
                dhcp(State::Operational);

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
                dhcp(State::NoDhcp);

                iface.update_ip_addrs(|addrs| {
                    addrs[0] = IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::UNSPECIFIED, 0))
                });
                iface.routes_mut().remove_default_ipv4_route();
                self.reset_dhcp();
            }
        }
    }

    fn handle_tcp<F: FnOnce(bool)>(&mut self, identify: F) {
        let socket = self.interface.get_socket::<TcpSocket>(self.tcp_handle);
        if !socket.is_open() {
            socket.listen(CONTROL_PORT).unwrap();
        }

        if socket.may_recv() {
            socket
                .recv(|b| {
                    let len = b.len();
                    match b.iter().next() {
                        Some(b'0') => identify(false),
                        Some(b'1') => identify(true),
                        _ => {}
                    }
                    (len, ())
                })
                .unwrap();

            socket.close();
        }
    }
}
