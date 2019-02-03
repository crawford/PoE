use vcell::VolatileCell;

/// The DI page contains production calibration data as well as device identification information.
pub struct PageEntryMap {
    /// CRC of DI-page and calibration temperature
    pub cal: CAL,

    /// Module trace information
    pub moduleinfo: Unimplemented,

    /// Module Crystal Oscillator Calibration
    pub modxocal: Unimplemented,

    // Reserved
    _reserved0: [u32; 5],

    /// External Component description
    pub extinfo: Unimplemented,

    // Reserved
    _reserved1: [u32; 1],

    /// EUI48 OUI and Unique identifier
    pub eui48l: EUI48L,

    /// OUI
    pub eui48h: EUI48H,

    /// Custom information
    pub custominfo: Unimplemented,

    /// Flash page size and misc. chip information
    pub meminfo: Unimplemented,

    // Reserved
    _reserved2: [u32; 2],

    /// Low 32 bits of device unique number
    pub uniquel: UNIQUEL,

    /// High 32 bits of device unique number
    pub uniqueh: UNIQUEH,

    /// Flash and SRAM Memory size in kB
    pub msize: Unimplemented,

    /// Part description
    pub part: Unimplemented,

    /// Device information page revision
    pub devinforev: Unimplemented,

    /// EMU Temperature Calibration Information
    pub emutemp: EMUTEMP,

    // Reserved
    _reserved3: [u32; 2],

    /// ADC0 calibration register 0
    pub adc0cal0: Unimplemented,

    /// ADC0 calibration register 1
    pub adc0cal1: Unimplemented,

    /// ADC0 calibration register 2
    pub adc0cal2: Unimplemented,

    /// ADC0 calibration register 3
    pub adc0cal3: Unimplemented,

    /// ADC1 calibration register 0
    pub adc1cal0: Unimplemented,

    /// ADC1 calibration register 1
    pub adc1cal1: Unimplemented,

    /// ADC1 calibration register 2
    pub adc1cal2: Unimplemented,

    /// ADC1 calibration register 3
    pub adc1cal3: Unimplemented,

    /// HFRCO Calibration Register (4 MHz)
    pub hfrcocal0: Unimplemented,

    // Reserved
    _reserved4: [u32; 2],

    /// HFRCO Calibration Register (7 MHz)
    pub hfrcocal3: Unimplemented,

    // Reserved
    _reserved5: [u32; 2],

    /// HFRCO Calibration Register (13 MHz)
    pub hfrcocal6: Unimplemented,

    /// HFRCO Calibration Register (16 MHz)
    pub hfrcocal7: Unimplemented,

    /// HFRCO Calibration Register (19 MHz)
    pub hfrcocal8: Unimplemented,

    // Reserved
    _reserved6: [u32; 1],

    /// HFRCO Calibration Register (26 MHz)
    pub hfrcocal10: Unimplemented,

    /// HFRCO Calibration Register (32 MHz)
    pub hfrcocal11: Unimplemented,

    /// HFRCO Calibration Register (38 MHz)
    pub hfrcocal12: Unimplemented,

    /// HFRCO Calibration Register (48 MHz)
    pub hfrcocal13: Unimplemented,

    /// HFRCO Calibration Register (56 MHz)
    pub hfrcocal14: Unimplemented,

    /// HFRCO Calibration Register (64 MHz)
    pub hfrcocal15: Unimplemented,

    // Reserved
    _reserved7: [u32; 8],

    /// AUXHFRCO Calibration Register (4 MHz)
    pub auxhfrcocal0: Unimplemented,

    // Reserved
    _reserved8: [u32; 2],

    /// AUXHFRCO Calibration Register (7 MHz)
    pub auxhfrcocal3: Unimplemented,

    // Reserved
    _reserved9: [u32; 2],

    /// AUXHFRCO Calibration Register (13 MHz)
    pub auxhfrcocal6: Unimplemented,

    /// AUXHFRCO Calibration Register (16 MHz)
    pub auxhfrcocal7: Unimplemented,

    /// AUXHFRCO Calibration Register (19 MHz)
    pub auxhfrcocal8: Unimplemented,

    // Reserved
    _reserved10: [u32; 1],

    /// AUXHFRCO Calibration Register (26 MHz)
    pub auxhfrcocal10: Unimplemented,

    /// AUXHFRCO Calibration Register (32 MHz)
    pub auxhfrcocal11: Unimplemented,

    /// AUXHFRCO Calibration Register (38 MHz)
    pub auxhfrcocal12: Unimplemented,

    /// AUXHFRCO Calibration Register (48 MHz)
    pub auxhfrcocal13: Unimplemented,

    /// AUXHFRCO Calibration Register (50 MHz)
    pub auxhfrcocal14: Unimplemented,

    // Reserved
    _reserved11: [u32; 9],

    /// VMON Calibration Register 0
    pub vmoncal0: Unimplemented,

    /// VMON Calibration Register 1
    pub vmoncal1: Unimplemented,

    /// VMON Calibration Register 2
    pub vmoncal2: Unimplemented,

    // Reserved
    _reserved12: [u32; 3],

    /// IDAC0 Calibration Register 0
    pub idac0cal0: Unimplemented,

    /// IDAC0 Calibration Register 1
    pub idac0cal1: Unimplemented,

    // Reserved
    _reserved13: [u32; 2],

    /// DCDC Low-noise VREF Trim Register 0
    pub dcdclnvctrl0: Unimplemented,

    /// DCDC Low-power VREF Trim Register 0
    pub dcdclpvctrl0: Unimplemented,

    /// DCDC Low-power VREF Trim Register 1
    pub dcdclpvctrl1: Unimplemented,

    /// DCDC Low-power VREF Trim Register 2
    pub dcdclpvctrl2: Unimplemented,

    /// DCDC Low-power VREF Trim Register 3
    pub dcdclpvctrl3: Unimplemented,

    /// DCDC LPCMPHYSSEL Trim Register 0
    pub dcdclpcmphyssel0: Unimplemented,

    /// DCDC LPCMPHYSSEL Trim Register 1
    pub dcdclpcmphyssel1: Unimplemented,

    /// VDAC0 Cals for Main Path
    pub vdac0maincal: Unimplemented,

    /// VDAC0 Cals for Alternate Path
    pub vdac0altcal: Unimplemented,

    /// VDAC0 CH1 Error Cal
    pub vdac0ch1cal: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 0, INCBW=1
    pub opa0cal0: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 1, INCBW=1
    pub opa0cal1: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 2, INCBW=1
    pub opa0cal2: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 3, INCBW=1
    pub opa0cal3: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 0, INCBW=0
    pub opa0cal4: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 1, INCBW=0
    pub opa0cal5: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 2, INCBW=0
    pub opa0cal6: Unimplemented,

    /// OPA0 Calibration Register for DRIVESTRENGTH 3, INCBW=0
    pub opa0cal7: Unimplemented,

    /// OPA1 Calibration Register for DRIVESTRENGTH 0, INCBW=1
    pub opa1cal0: Unimplemented,

    /// OPA1 Calibration Register for DRIVESTRENGTH 1, INCBW=1
    pub opa1cal1: Unimplemented,

    /// OPA1 Calibration Register for DRIVESTRENGTH 2, INCBW=1
    pub opa1cal2: Unimplemented,

    // Reserved
    _reserved14: [u32; 1],

    /// OPA1 Calibration Register for DRIVESTRENGTH 0, INCBW=0
    pub opa1cal4: Unimplemented,

    /// OPA1 Calibration Register for DRIVESTRENGTH 1, INCBW=0
    pub opa1cal5: Unimplemented,

    /// OPA1 Calibration Register for DRIVESTRENGTH 2, INCBW=0
    pub opa1cal6: Unimplemented,

    /// OPA1 Calibration Register for DRIVESTRENGTH 3, INCBW=0
    pub opa1cal7: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 0, INCBW=1
    pub opa2cal0: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 1, INCBW=1
    pub opa2cal1: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 2, INCBW=1
    pub opa2cal2: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 3, INCBW=1
    pub opa2cal3: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 0, INCBW=0
    pub opa2cal4: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 1, INCBW=0
    pub opa2cal5: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 2, INCBW=0
    pub opa2cal6: Unimplemented,

    /// OPA2 Calibration Register for DRIVESTRENGTH 3, INCBW=0
    pub opa2cal7: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 0, INCBW=1
    pub opa3cal0: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 1, INCBW=1
    pub opa3cal1: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 2, INCBW=1
    pub opa3cal2: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 3, INCBW=1
    pub opa3cal3: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 0, INCBW=0
    pub opa3cal4: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 1, INCBW=0
    pub opa3cal5: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 2, INCBW=0
    pub opa3cal6: Unimplemented,

    /// OPA3 Calibration Register for DRIVESTRENGTH 3, INCBW=0
    pub opa3cal7: Unimplemented,

    /// Cap Sense Gain Adjustment
    pub csengaincal: Unimplemented,

    // Reserved
    _reserved15: [u32; 22],

    /// USHFRCO Calibration Register (16 MHz)
    pub ushfrcocal7: Unimplemented,

    // Reserved
    _reserved16: [u32; 3],

    /// USHFRCO Calibration Register (32 MHz)
    pub ushfrcocal11: Unimplemented,

    // Reserved
    _reserved17: [u32; 1],

    /// USHFRCO Calibration Register (48 MHz)
    pub ushfrcocal13: Unimplemented,

    /// USHFRCO Calibration Register (50 MHz)
    pub ushfrcocal14: Unimplemented,
}

impl PageEntryMap {
    pub fn get() -> &'static PageEntryMap {
        unsafe { &*(0x0FE0_81B0 as *const PageEntryMap) }
    }
}

pub struct Unimplemented {
    _entry: u32,
}

pub struct CAL {
    entry: VolatileCell<u32>,
}

impl CAL {
    pub fn temp(&self) -> u8 {
        (self.entry.get() >> 16) as u8
    }

    pub fn crc(&self) -> u16 {
        self.entry.get() as u16
    }
}

pub struct EUI48L {
    entry: VolatileCell<u32>,
}

impl EUI48L {
    pub fn oui48l(&self) -> u8 {
        (self.entry.get() >> 24) as u8
    }

    pub fn uniqueid(&self) -> u32 {
        self.entry.get() & 0x00FF_FFFF
    }
}

pub struct EUI48H {
    entry: VolatileCell<u32>,
}

impl EUI48H {
    pub fn oui48h(&self) -> u16 {
        self.entry.get() as u16
    }
}

pub struct UNIQUEL {
    entry: VolatileCell<u32>,
}

impl UNIQUEL {
    pub fn uniquel(&self) -> u32 {
        self.entry.get()
    }
}

pub struct UNIQUEH {
    entry: VolatileCell<u32>,
}

impl UNIQUEH {
    pub fn uniqueh(&self) -> u32 {
        self.entry.get()
    }
}

pub struct EMUTEMP {
    entry: VolatileCell<u32>,
}

impl EMUTEMP {
    pub fn emuroomtemp(&self) -> u8 {
        self.entry.get() as u8
    }
}
