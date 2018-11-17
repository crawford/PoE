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

use mac::MAC;
use phy::{LinkState, Register, OUI, PHY};

pub struct KSZ8091 {
    address: u8,
}

impl KSZ8091 {
    pub fn new(address: u8) -> KSZ8091 {
        KSZ8091 { address }
    }
}

impl PHY for KSZ8091 {
    fn oui(&self, mac: &MAC) -> OUI {
        // Bits [2:17] of the OUI are in bits [15:0] of PHY ID 1.
        // Bits [18:23] of the OUI are in bits [15:10] of PHY ID 2.
        // Concatenating these two gives the OUI in bit-reverse order
        // (e.g. 0b00 [2:17] [18:23] 0000 0000).
        let id1 = u32::from(mac.mdio_read(self.address, Register::PhyId1));
        let id2 = u32::from(mac.mdio_read(self.address, Register::PhyId2));

        // TODO: Use u32::reverse_bits once it is stablized.
        let oui = reverse(id1 << 14 | id2 >> 2);
        OUI([(oui as u8), ((oui >> 8) as u8), ((oui >> 16) as u8)])
    }

    fn link_state(&self, _mac: &MAC) -> LinkState {
        unimplemented!()
    }

    fn set_link_state(&mut self, _mac: &MAC, _state: LinkState) {
        unimplemented!()
    }
}

fn reverse(mut x: u32) -> u32 {
    x = ((x >> 1) & 0x5555_5555) | ((x & 0x5555_5555) << 1);
    x = ((x >> 2) & 0x3333_3333) | ((x & 0x3333_3333) << 2);
    x = ((x >> 4) & 0x0f0f_0f0f) | ((x & 0x0f0f_0f0f) << 4);
    x = ((x >> 8) & 0x00ff_00ff) | ((x & 0x00ff_00ff) << 8);
    x = ((x >> 16) & 0x0000_ffff) | ((x & 0x0000_ffff) << 16);
    x
}
