// Copyright 2018 Alex Crawford
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

// XXX: Figure out error handling

use crate::mac::Mac;
use core::fmt;

pub trait Phy {
    fn oui(&self, mac: &dyn Mac) -> Oui;
    fn link_state(&self, mac: &dyn Mac) -> LinkState;
    fn set_link_state(&mut self, mac: &dyn Mac, state: LinkState);
    fn enable_interrupts(&mut self, mac: &mut dyn Mac);
    fn irq(&mut self, mac: &mut dyn Mac);
}

#[allow(unused)]
pub struct LinkState {
    pub speed: LinkSpeed,
    pub duplex: LinkDuplex,
}

#[allow(unused)]
pub enum LinkSpeed {
    TenMbps,
    HundredMbps,
}

#[allow(unused)]
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

pub fn probe_for_phy<M: Mac>(mac: &M) -> Option<u8> {
    (0..32).find(|addr| {
        let id1 = mac.mdio_read(*addr, Register::PhyId1);
        let id2 = mac.mdio_read(*addr, Register::PhyId2);

        id1 != 0x0000 && id1 != 0x3FFF && id2 != 0x0000 && id2 != 0xFFFF
            || id1 != 0x0000 && id1 != 0x3FFF && id1 != 0xFFFF
            || id2 != 0x0000 && id2 != 0x3FFF && id2 != 0xFFFF
    })
}
