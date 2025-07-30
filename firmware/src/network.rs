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
use crate::command::{Interpreter, InterpreterMode};
use crate::efm32gg::EFM32GG;
use crate::ksz8091::KSZ8091;

#[cfg(feature = "telnet")]
use ignore_result::Ignore;
use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::{Dhcpv4Event, Dhcpv4Socket, TcpSocket};
use smoltcp::wire::{IpCidr, Ipv4Address, Ipv4Cidr};

const CONTROL_PORT: u16 = 51900;

#[cfg(feature = "telnet")]
const TELNET_PORT: u16 = 23;

pub struct Resources {
    pub interface: Interface<'static, EFM32GG<'static, KSZ8091>>,
    pub dhcp_handle: SocketHandle,
    pub tcp_handle: SocketHandle,

    #[cfg(feature = "telnet")]
    pub telnet_handle: SocketHandle,

    #[cfg(feature = "telnet")]
    pub interpreter: Interpreter,
    #[cfg(feature = "telnet")]
    pub prev_mode: InterpreterMode,
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

    #[cfg(feature = "telnet")]
    fn handle_telnet(&mut self) {
        use InterpreterMode::*;
        #[allow(unused)]
        use TelnetCommands::*;
        #[allow(unused)]
        use TelnetOptions::*;

        const EOF: u8 = 236;
        const IP: u8 = 244;
        const WILL: u8 = 251;
        const WONT: u8 = 252;
        const DO: u8 = 253;
        const DONT: u8 = 254;
        const IAC: u8 = 255;

        #[allow(unused)]
        enum TelnetCommands {
            EOF = 236,
            IP = 244,
            WILL = 251,
            WONT = 252,
            DO = 253,
            DONT = 254,
            IAC = 255,
        }

        #[allow(unused)]
        enum TelnetOptions {
            BinaryTransmission = 0,
            Echo = 1,
            SuppressGoAhead = 3,
            TimingMark = 6,
            LineMode = 34,
            SuppressLocalEcho = 45,
        }
        const BINARY_TRANSMISSION: u8 = 0;
        const ECHO: u8 = 1;
        const SUPPRESS_GO_AHEAD: u8 = 3;
        const TIMING_MARK: u8 = 6;
        const LINEMODE: u8 = 34;
        const SUPPRESS_LOCAL_ECHO: u8 = 45;

        let socket = self.interface.get_socket::<TcpSocket>(self.telnet_handle);

        #[allow(unused)]
        macro_rules! do_option {
            ($option:expr) => {
                socket.send_slice(&[IAC, DO, $option]).ignore()
            };
        }

        #[allow(unused)]
        macro_rules! dont_option {
            ($option:expr) => {
                socket.send_slice(&[IAC, DONT, $option]).ignore()
            };
        }

        #[allow(unused)]
        macro_rules! will_option {
            ($option:expr) => {
                socket.send_slice(&[IAC, WILL, $option]).ignore()
            };
        }

        #[allow(unused)]
        macro_rules! wont_option {
            ($option:expr) => {
                socket.send_slice(&[IAC, WONT, $option]).ignore()
            };
        }

        if !socket.is_open() {
            socket.listen(TELNET_PORT).unwrap();
        }

        if socket.can_recv() && socket.can_send() {
            let mut data = [0; 512];
            let request = socket
                .recv(|b| {
                    data[..b.len()].copy_from_slice(b);
                    (b.len(), &data[..b.len()])
                })
                .expect("receiving from telnet");

            let mut bytes = request.iter();
            let mut abort = false;
            while bytes.as_ref().first() == Some(&IAC) && bytes.as_ref().get(1) != Some(&IAC) {
                bytes.next();
                match bytes.next() {
                    Some(&DO) => match bytes.next() {
                        Some(&SUPPRESS_GO_AHEAD) => will_option!(SUPPRESS_GO_AHEAD),
                        Some(&TIMING_MARK) => will_option!(TIMING_MARK),
                        Some(option) => log::debug!("ignoring telnet DO: option {option}"),
                        None => log::debug!("ignoring malformed telnet DO command"),
                    },
                    Some(&WILL) => match bytes.next() {
                        Some(&BINARY_TRANSMISSION | &ECHO | &LINEMODE | &SUPPRESS_LOCAL_ECHO) => {}
                        Some(option) => log::debug!("ignoring telnet WILL: option {option}"),
                        None => log::debug!("ignoring malformed telnet WILL command"),
                    },
                    Some(&WONT) => match bytes.next() {
                        Some(&ECHO | &BINARY_TRANSMISSION) => {}
                        Some(&SUPPRESS_LOCAL_ECHO) => {
                            log::debug!("telnet client won't suppress local echo")
                        }
                        Some(option) => log::debug!("ignoring telnet WON'T: option {option}"),
                        None => log::debug!("ignoring malformed telnet WON'T command"),
                    },
                    Some(&DONT) => match bytes.next() {
                        Some(&ECHO) => {}
                        Some(option) => log::debug!("ignoring telnet DON'T: option {option}"),
                        None => log::debug!("ignoring malformed telnet DON'T command"),
                    },
                    Some(&EOF) => socket.close(),
                    Some(&IP) => abort = true,
                    Some(code) => log::debug!("ignoring telnet command: {code}"),
                    None => log::debug!("ignoring malformed telnet command"),
                }
            }
            if abort {
                self.interpreter.abort(socket);
                self.prev_mode = self.interpreter.mode();
                return;
            }

            self.interpreter.exec(bytes.as_slice(), socket);
            let mode = self.interpreter.mode();
            match (self.prev_mode, mode) {
                (Command, Data) => {
                    // do_option!(BINARY_TRANSMISSION);
                    // will_option!(ECHO);
                }
                (Data, Command) => {
                    // dont_option!(BINARY_TRANSMISSION);
                    // wont_option!(ECHO);
                }
                _ => {}
            }
            self.prev_mode = mode;
        } else if !socket.may_send() {
            // TODO: Why is this causing nmap to report that the socket is closed?
            //       Does this only happen with the SLSTK3701A?
            // socket.close();
        }
    }
}
