// Copyright 2018 Alex Crawford
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

use crate::mac::Mdio;
use crate::phy::{LinkState, Oui, Phy, Register};

pub struct KSZ8091 {
    address: u8,
}

impl KSZ8091 {
    pub fn new(address: u8, mdio: &mut dyn Mdio) -> KSZ8091 {
        // Enable interrupts for link-up and link-down
        mdio.write(address, Register::Vendor(0x1B), 0x0500);

        KSZ8091 { address }
    }
}

impl Phy for KSZ8091 {
    fn oui(&self, mdio: &dyn Mdio) -> Oui {
        // Bits [2:17] of the Oui are in bits [15:0] of PHY ID 1.
        // Bits [18:23] of the Oui are in bits [15:10] of PHY ID 2.
        // Concatenating these two gives the Oui in bit-reverse order
        // (e.g. 0b00 [2:17] [18:23] 0000 0000).
        let id1 = u32::from(mdio.read(self.address, Register::PhyId1));
        let id2 = u32::from(mdio.read(self.address, Register::PhyId2));

        let oui = u32::reverse_bits(id1 << 14 | id2 >> 2);
        Oui([(oui as u8), ((oui >> 8) as u8), ((oui >> 16) as u8)])
    }

    fn link_state(&self, _mdio: &dyn Mdio) -> LinkState {
        unimplemented!()
    }

    fn set_link_state(&mut self, _mdio: &dyn Mdio, _state: LinkState) {
        unimplemented!()
    }

    fn irq(&mut self, mdio: &mut dyn Mdio) {
        let status = mdio.read(self.address, Register::Vendor(0x1B)) as u8;

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
