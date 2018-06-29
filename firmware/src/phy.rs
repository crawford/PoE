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
// TODO: Assign names to well-known register addresses

use core::fmt;
use mac::MAC;

pub trait PHY {
    fn oui(&self, mac: &MAC) -> OUI;
    fn link_state(&self, mac: &MAC) -> LinkState;
    fn set_link_state(&mut self, mac: &MAC, state: LinkState);
}

pub struct LinkState {
    pub speed: LinkSpeed,
    pub duplex: LinkDuplex,
}

pub enum LinkSpeed {
    TenMbps,
    HundredMbps,
}

pub enum LinkDuplex {
    HalfDuplex,
    FullDuplex,
}

pub struct OUI(pub [u8; 3]);

impl fmt::Display for OUI {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02X}-{:02X}-{:02X}", self.0[0], self.0[1], self.0[2])
    }
}

pub fn probe_for_phy<M: MAC>(mac: &M) -> Option<u8> {
    (0..32).find(|addr| {
        let id1 = mac.mdio_read(*addr, 0x02);
        let id2 = mac.mdio_read(*addr, 0x03);

        ((id1 != 0x0000 || id2 != 0x0000)
            && (id1 != 0x0000 || id2 != 0x3FFF)
            && (id1 != 0x0000 || id2 != 0xFFFF)
            && (id1 != 0x3FFF || id2 != 0x0000)
            && (id1 != 0x3FFF || id2 != 0x3FFF)
            && (id1 != 0x3FFF || id2 != 0xFFFF)
            && (id1 != 0xFFFF || id2 != 0x0000)
            && (id1 != 0xFFFF || id2 != 0xFFFF))
    })
}