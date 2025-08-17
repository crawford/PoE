// Copyright (C) 2018 Alex Crawford
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// XXX: Figure out error handling

use crate::mac::Mdio;
use core::fmt;

pub trait Phy {
    fn oui(&self, mac: &dyn Mdio) -> Oui;
    fn link_state(&self, mac: &dyn Mdio) -> Option<LinkState>;
    fn set_link_state(&mut self, mac: &dyn Mdio, state: LinkState);
    fn irq(&mut self, mac: &mut dyn Mdio);
}

#[derive(Debug)]
pub struct LinkState {
    pub speed: LinkSpeed,
    pub duplex: LinkDuplex,
}

#[derive(Debug)]
pub enum LinkSpeed {
    TenMbps,
    HundredMbps,
}

#[derive(Debug)]
pub enum LinkDuplex {
    HalfDuplex,
    FullDuplex,
}

pub struct Oui(pub [u8; 3]);

impl fmt::Display for Oui {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02X}-{:02X}-{:02X}", self.0[0], self.0[1], self.0[2])
    }
}

#[derive(Debug)]
pub enum Register {
    BasicControl,
    BasicStatus,
    PhyId1,
    PhyId2,
    AutoAdvertisement,
    AutoPartnerAbility,
    AutoExpansion,
    AutoNextPage,
    AutoPartnerNextPageAbility,
    MmdControl,
    MmdRegisterData,
    Vendor(u8),
}

impl From<Register> for u8 {
    fn from(register: Register) -> u8 {
        match register {
            Register::BasicControl => 0x00,
            Register::BasicStatus => 0x01,
            Register::PhyId1 => 0x02,
            Register::PhyId2 => 0x03,
            Register::AutoAdvertisement => 0x04,
            Register::AutoPartnerAbility => 0x05,
            Register::AutoExpansion => 0x06,
            Register::AutoNextPage => 0x07,
            Register::AutoPartnerNextPageAbility => 0x08,
            Register::MmdControl => 0x0D,
            Register::MmdRegisterData => 0x0E,
            Register::Vendor(addr) => addr,
        }
    }
}

pub fn probe_addr<M: Mdio>(mdio: &M) -> Option<u8> {
    (0..32).find(|addr| {
        let id1 = mdio.read(*addr, Register::PhyId1);
        let id2 = mdio.read(*addr, Register::PhyId2);

        id1 != 0x0000 && id1 != 0x3FFF && id2 != 0x0000 && id2 != 0xFFFF
            || id1 != 0x0000 && id1 != 0x3FFF && id1 != 0xFFFF
            || id2 != 0x0000 && id2 != 0x3FFF && id2 != 0xFFFF
    })
}
