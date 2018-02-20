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

use efm32gg11b820::{Interrupt, CMU, ETH, GPIO, NVIC};

pub struct MAC {}

impl MAC {
    // TODO: This should probably accept a PHY so that this doesn't need to understand how to
    // configure the PHY.
    pub fn new() -> MAC {
        MAC {}
    }

    /// This assumes that the PHY will be interfaced via RMII with the EFM providing the ethernet
    /// clock.
    pub fn configure(&self, eth: &ETH, cmu: &CMU, gpio: &GPIO, nvic: &mut NVIC) {
        // Enable the HFPER clock and source CLKOUT2 from HFXO
        cmu.ctrl.modify(|_, reg| {
            reg.hfperclken().set_bit();
            reg.clkoutsel2().hfxo();
            reg
        });

        // Enable the clock to the ethernet controller
        cmu.hfbusclken0.modify(|_, reg| reg.eth().set_bit());

        // Scale the MDC down to 1.5625MHz (below the 2.5MHz limit)
        // Enable 1536-byte frames, which are needed to support 802.11Q VLAN tagging
        eth.networkcfg.write(|reg| {
            reg.mdcclkdiv().divby32();
            reg.rx1536byteframes().set_bit();
            reg
        });

        // TODO: Use BITSET/BITCLEAR instead
        // Hold the PHY module in reset
        gpio.ph_model.modify(|_, reg| reg.mode7().pushpull());
        gpio.ph_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() & !(1 << 7)) });

        // Power up the PHY module
        gpio.pi_modeh.modify(|_, reg| reg.mode10().pushpull());
        gpio.pi_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() | (1 << 10)) });

        // Configure the RMII GPIOs
        gpio.pf_model.write(|reg| {
            reg.mode6().pushpull(); // TXD1
            reg.mode7().pushpull(); // TXD0
            reg
        });
        gpio.pf_modeh.write(|reg| {
            reg.mode8().pushpull(); // TX_EN
            reg.mode9().input(); // RXD1
            reg
        });
        gpio.pd_modeh.write(|reg| {
            reg.mode9().input(); // RXD0
            reg.mode10().pushpull(); // REFCLK
            reg.mode11().input(); // CRS_DV
            reg.mode12().input(); // RX_ER
            reg.mode13().pushpull(); // MDIO
            reg.mode14().pushpull(); // MDC
            reg
        });

        // Enable the PHY's reference clock
        cmu.routeloc0.modify(|_, reg| reg.clkout2loc().loc5());
        cmu.routepen.modify(|_, reg| reg.clkout2pen().set_bit());

        // Enable the RMII and MDIO
        eth.routeloc1.write(|reg| {
            reg.rmiiloc().loc1();
            reg.mdioloc().loc1();
            reg
        });
        eth.routepen.write(|reg| {
            reg.rmiipen().set_bit();
            reg.mdiopen().set_bit();
            reg
        });

        // Enable the management interface
        eth.networkctrl.write(|reg| {
            reg.manporten().set_bit();
            reg
        });

        // Clear pending interrupts
        nvic.clear_pending(Interrupt::ETH);
        eth.ifcr.write(|reg| {
            reg.mngmntdone().set_bit();
            reg.rxcmplt().set_bit();
            reg.rxusedbitread().set_bit();
            reg.txusedbitread().set_bit();
            reg.txunderrun().set_bit();
            reg.rtrylmtorlatecol().set_bit();
            reg.ambaerr().set_bit();
            reg.txcmplt().set_bit();
            reg.rxoverrun().set_bit();
            reg.respnotok().set_bit();
            reg.nonzeropfrmquant().set_bit();
            reg.pausetimezero().set_bit();
            reg.pfrmtx().set_bit();
            reg.ptpdlyreqfrmrx().set_bit();
            reg.ptpsyncfrmrx().set_bit();
            reg.ptpdlyreqfrmtx().set_bit();
            reg.ptpsyncfrmtx().set_bit();
            reg.ptppdlyreqfrmrx().set_bit();
            reg.ptppdlyrespfrmrx().set_bit();
            reg.ptppdlyreqfrmtx().set_bit();
            reg.ptppdlyrespfrmtx().set_bit();
            reg.tsusecregincr().set_bit();
            reg.rxlpiindc().set_bit();
            reg.wolevntrx().set_bit();
            reg.tsutimercomp().set_bit();
            reg
        });

        // Enable interrupts
        eth.iens.write(|reg| {
            reg.mngmntdone().set_bit();
            reg.rxcmplt().set_bit();
            reg.rxusedbitread().set_bit();
            reg.txusedbitread().set_bit();
            reg.txunderrun().set_bit();
            reg.rtrylmtorlatecol().set_bit();
            reg.ambaerr().set_bit();
            reg.txcmplt().set_bit();
            reg.rxoverrun().set_bit();
            reg.respnotok().set_bit();
            reg.nonzeropfrmquant().set_bit();
            reg.pausetimezero().set_bit();
            reg.pfrmtx().set_bit();
            reg.ptpdlyreqfrmrx().set_bit();
            reg.ptpsyncfrmrx().set_bit();
            reg.ptpdlyreqfrmtx().set_bit();
            reg.ptpsyncfrmtx().set_bit();
            reg.ptppdlyreqfrmrx().set_bit();
            reg.ptppdlyrespfrmrx().set_bit();
            reg.ptppdlyreqfrmtx().set_bit();
            reg.ptppdlyrespfrmtx().set_bit();
            reg.tsusecregincr().set_bit();
            reg.rxlpiindc().set_bit();
            reg.wolevntrx().set_bit();
            reg.tsutimercomp().set_bit();
            reg
        });
        nvic.enable(Interrupt::ETH);

        // Release the PHY reset
        gpio.ph_dout
            .modify(|read, write| unsafe { write.dout().bits(read.dout().bits() | (1 << 7)) });

        // Enable the global clock
        eth.ctrl.write(|reg| reg.gblclken().set_bit());
    }

    pub fn isr(&mut self) {}
}
