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

use crate::mac::Mac;
use crate::phy::{LinkState, Oui, Phy, Register};

pub struct KSZ8091 {
    address: u8,
}

impl KSZ8091 {
    pub fn new(address: u8) -> KSZ8091 {
        KSZ8091 { address }
    }
}

impl Phy for KSZ8091 {
    fn oui(&self, mac: &dyn Mac) -> Oui {
        // Bits [2:17] of the Oui are in bits [15:0] of PHY ID 1.
        // Bits [18:23] of the Oui are in bits [15:10] of PHY ID 2.
        // Concatenating these two gives the Oui in bit-reverse order
        // (e.g. 0b00 [2:17] [18:23] 0000 0000).
        let id1 = u32::from(mac.mdio_read(self.address, Register::PhyId1));
        let id2 = u32::from(mac.mdio_read(self.address, Register::PhyId2));

        let oui = u32::reverse_bits(id1 << 14 | id2 >> 2);
        Oui([(oui as u8), ((oui >> 8) as u8), ((oui >> 16) as u8)])
    }

    fn link_state(&self, _mac: &dyn Mac) -> LinkState {
        unimplemented!()
    }

    fn set_link_state(&mut self, _mac: &dyn Mac, _state: LinkState) {
        unimplemented!()
    }

    /// Enable interrupts for link-up and link-down
    fn enable_interrupts(&mut self, mac: &mut dyn Mac) {
        mac.mdio_write(self.address, Register::Vendor(0x1B), 0x0500);
    }

    fn irq(&mut self, mac: &mut dyn Mac) {
        let status = mac.mdio_read(self.address, Register::Vendor(0x1B)) as u8;

        macro_rules! bit_str {
            ($pos:literal, $str:expr) => {
                match status & (1 << $pos) {
                    0 => "",
                    _ => $str,
                }
            };
        }

        log::trace!(
            "PHY IRQ:{}{}{}{}{}{}{}{}",
            bit_str!(7, " jabber"),
            bit_str!(6, " receive-error"),
            bit_str!(5, " page-received"),
            bit_str!(4, " parallel-detect-fault"),
            bit_str!(3, " link-partner-ack"),
            bit_str!(2, " link-down"),
            bit_str!(1, " remote-fault"),
            bit_str!(0, " link-up"),
        );
    }
}
