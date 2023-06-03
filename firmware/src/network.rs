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

#[cfg(feature = "telnet")]
use crate::command;
use crate::efm32gg::EFM32GG;
use crate::ksz8091::KSZ8091;

#[cfg(feature = "telnet")]
use core::str;
use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::{Dhcpv4Event, Dhcpv4Socket, TcpSocket};
use smoltcp::wire::{IpCidr, Ipv4Address, Ipv4Cidr};

const CONTROL_PORT: u16 = 51900;
const HTTP_PORT: u16 = 80;

#[cfg(feature = "telnet")]
const TELNET_PORT: u16 = 23;

pub struct Resources {
    pub interface: Interface<'static, EFM32GG<'static, KSZ8091>>,
    pub dhcp_handle: SocketHandle,
    pub http_handle: SocketHandle,
    pub tcp_handle: SocketHandle,

    #[cfg(feature = "telnet")]
    pub telnet_handle: SocketHandle,

    pub id_active: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum State {
    Uninit,
    NoLink,
    NoDhcp,
    NoGateway,
    Operational,
}

enum Request {
    GetIndex,
    ToggleIdentify,
    Unknown,
}

impl Resources {
    pub fn handle_sockets<D, I>(&mut self, dhcp: D, mut identify: I)
    where
        D: FnOnce(State),
        I: FnMut(bool),
    {
        self.handle_dhcp(dhcp);
        self.handle_http(&mut identify);
        self.handle_tcp(&mut identify);

        #[cfg(feature = "telnet")]
        self.handle_telnet();
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

    fn handle_http<F: FnMut(bool)>(&mut self, mut enable_led: F) {
        use Request::*;

        fn parse(data: &[u8]) -> (usize, Request) {
            if data.starts_with(b"GET /") {
                (data.len(), GetIndex)
            } else if data.starts_with(b"POST /identify") {
                (data.len(), ToggleIdentify)
            } else {
                (data.len(), Unknown)
            }
        }

        let socket = self.interface.get_socket::<TcpSocket>(self.http_handle);

        if !socket.is_open() {
            socket.listen(HTTP_PORT).unwrap();
        }

        macro_rules! reply {
            ($name:literal) => {{
                let resp = include_bytes!(concat!(env!("OUT_DIR"), "/", $name));
                socket
                    .send(|b| {
                        b[0..resp.len()].copy_from_slice(resp);

                        (resp.len(), ())
                    })
                    .expect(concat!("sending ", $name));
            }};
        }

        if socket.can_recv() && socket.can_send() {
            match socket.recv(|b| parse(b)).expect("receiving from http") {
                GetIndex => reply!("index.http"),
                Unknown => reply!("not-found.http"),
                ToggleIdentify => {
                    self.id_active = !self.id_active;
                    enable_led(self.id_active);
                    reply!("identify.http");
                }
            }
            // XXX: This doesn't actually reset the connection for some reason.
            socket.close();
        }
    }

    #[cfg(feature = "telnet")]
    fn handle_telnet(&mut self) {
        const EOF: u8 = 236;
        const WILL: u8 = 251;
        const DO: u8 = 253;
        const IAC: u8 = 255;

        let socket = self.interface.get_socket::<TcpSocket>(self.telnet_handle);

        if !socket.is_open() {
            socket.listen(TELNET_PORT).unwrap();
        }

        if socket.can_recv() && socket.can_send() {
            let mut data = [0; 128];
            let request = socket
                .recv(|b| {
                    data[..b.len()].copy_from_slice(b);
                    (b.len(), &data[..b.len()])
                })
                .expect("receiving from telnet");

            let mut bytes = request.iter();
            while bytes.as_ref().get(0) == Some(&IAC) {
                bytes.next();
                match bytes.next() {
                    Some(&DO | &WILL) => match bytes.next() {
                        Some(option) => log::debug!("ignoring telnet option code: {option}"),
                        None => log::debug!("ignoring malformed telnet DO/WILL command"),
                    },
                    Some(&EOF) => {
                        socket.close();
                        return;
                    }
                    Some(code) => log::debug!("ignoring telnet command: {code}"),
                    None => log::debug!("ignoring malformed telnet command"),
                }
            }

            let input = match str::from_utf8(bytes.as_slice()) {
                Ok(text) => text,
                Err(err) => {
                    log::warn!("failed to parse telnet input ({bytes:?}): {err}");
                    ""
                }
            };

            command::interpret(input, socket);
        }
    }
}
