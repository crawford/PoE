// Copyright 2023 Alex Crawford
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

pub mod itm;
pub mod rtt;

static mut LOGGER: Logger = Logger {
    #[cfg(feature = "itm")]
    itm: None,

    #[cfg(feature = "rtt")]
    rtt: None,
};

pub fn init() -> &'static Logger {
    let logger = unsafe { &LOGGER };
    log::set_logger(logger).expect("set_logger");
    logger
}

pub struct Logger {
    #[cfg(feature = "itm")]
    itm: Option<itm::Logger>,

    #[cfg(feature = "rtt")]
    rtt: Option<rtt::Logger>,
}

impl Logger {
    #[cfg(feature = "itm")]
    pub fn add_itm(&self, logger: itm::Logger) -> &Self {
        log::set_max_level(log::max_level().max(logger.level));
        unsafe { LOGGER.itm = Some(logger) };
        log::info!("ITM logging online!");
        self
    }

    #[cfg(feature = "rtt")]
    pub fn add_rtt(&self, logger: rtt::Logger) -> &Self {
        log::set_max_level(log::max_level().max(logger.level));
        unsafe { LOGGER.rtt = Some(logger) };
        log::info!("RTT logging online!");
        self
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        #[cfg(feature = "itm")]
        match &self.itm {
            Some(itm) if itm.enabled(metadata) => return true,
            _ => {}
        }

        #[cfg(feature = "rtt")]
        match &self.rtt {
            Some(rtt) if rtt.enabled(metadata) => return true,
            _ => {}
        }

        false
    }

    fn log(&self, record: &log::Record) {
        #[cfg(feature = "itm")]
        if let Some(itm) = &self.itm {
            itm.log(record);
        }

        #[cfg(feature = "rtt")]
        if let Some(rtt) = &self.rtt {
            rtt.log(record);
        }
    }

    fn flush(&self) {
        #[cfg(feature = "itm")]
        if let Some(itm) = &self.itm {
            itm.flush();
        }

        #[cfg(feature = "rtt")]
        if let Some(rtt) = &self.rtt {
            rtt.flush();
        }
    }
}
