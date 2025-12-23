use crate::ram::glacier::{Glacier, MMUCfg, PageSize};

use core::arch::asm;

#[allow(dead_code)]
pub mod flags {
    pub const VALID: usize = 0b1;
    pub const NEXT: usize  = 0b10000000011;

    pub const K_ROO: usize = 0b11110000011 | 0b11 << 53;
    pub const K_RWO: usize = 0b11100000011 | 0b11 << 53;
    pub const K_ROX: usize = 0b11110000011;
    pub const K_RWX: usize = 0b11100000011;

    pub const D_RO: usize  = 0b10010000111 | 0b11 << 53;
    pub const D_RW: usize  = 0b10000000111 | 0b11 << 53;

    pub const U_ROO: usize = 0b11111000011 | 0b11 << 53;
    pub const U_RWO: usize = 0b11101000011 | 0b11 << 53;
    pub const U_ROX: usize = 0b11111000011;
    pub const U_RWX: usize = 0b11101000011;
}

impl MMUCfg {
    pub fn detect() -> Self {
        let page_size = PageSize::Size4kiB;
        let va_bits = 48;
        let mut mmfr0: usize;
        unsafe { asm!("mrs {}, ID_AA64MMFR0_EL1", out(reg) mmfr0); }
        let ips = mmfr0 & 0xf;
        let pa_bits = match ips {
            0 => 32,
            1 => 36,
            2 => 40,
            3 => 42,
            4 => 44,
            5 => 48,
            6 => 52,
            _ => 48
        };

        return Self { page_size, va_bits, pa_bits };
    }

    fn tcr_el1(&self) -> usize {
        let tnsz = usize::BITS as usize - self.va_bits as usize;

        let tg = match self.page_size {
            PageSize::Size4kiB => 0b00,
            PageSize::Size16kiB => 0b10,
            PageSize::Size64kiB => 0b01
        };

        let mut tcr = 0;
        tcr |= tnsz | (tnsz << 16); // T0SZ and T1SZ
        tcr |= (tg << 14) | (tg << 30); // TG0 and TG1

        tcr |= 0b01 << 8;  // IRGN0 = Normal WB/WA
        tcr |= 0b01 << 10; // ORGN0 = Normal WB/WA
        tcr |= 0b11 << 12; // SH0 = Inner Shareable
        tcr |= 0b01 << 24; // IRGN1 = Normal WB/WA
        tcr |= 0b01 << 26; // ORGN1 = Normal WB/WA
        tcr |= 0b11 << 28; // SH1 = Inner Shareable

        let ips = match self.pa_bits {
            32 => 0b000,
            36 => 0b001,
            40 => 0b010,
            42 => 0b011,
            44 => 0b100,
            48 => 0b101,
            52 => 0b110,
            _ => 0b101
        };
        tcr |= ips << 32;
        return tcr;
    }
}

impl Glacier {
    pub fn identity_map(&self) {
        // Attr0 = Normal RAM, Inner/Outer Write-Back Non-transient
        // Attr1 = Device RAM nGnRnE
        let mair_el1: u64 = 0xff | (0x00 << 8);

        unsafe {
            asm!(
                "msr mair_el1, {mair}",
                "msr tcr_el1, {tcr}",
                "msr ttbr0_el1, {ttbr0}",
                "msr ttbr1_el1, {ttbr0}",
                "isb",

                "mrs x0, sctlr_el1",
                "orr x0, x0, #(1 << 0)",  // M bit: MMU enable
                "orr x0, x0, #(1 << 2)",  // C bit: Data cache enable
                "orr x0, x0, #(1 << 12)", // I bit: Instruction cache enable
                "msr sctlr_el1, x0",
                "isb",

                "ic iallu",
                "dsb sy",
                "isb",
                mair = in(reg) mair_el1,
                tcr = in(reg) self.cfg().tcr_el1(),
                ttbr0 = in(reg) self.root_table()
            );
        }
    }
}
