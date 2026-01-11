use crate::ram::glacier::{Glacier, PageSize, RvmCfg};

use core::arch::asm;

#[allow(dead_code)]
pub mod flags {
    pub const VALID: usize = 0b1;
    pub const NEXT: usize  = 0b100_0000_0011;

    pub const K_ROO: usize = 0b111_1000_0011 | 0b11 << 53;
    pub const K_RWO: usize = 0b111_0000_0011 | 0b11 << 53;
    pub const K_ROX: usize = 0b111_1000_0011;
    pub const K_RWX: usize = 0b111_0000_0011;

    pub const D_RO: usize  = 0b100_1000_0111 | 0b11 << 53;
    pub const D_RW: usize  = 0b100_0000_0111 | 0b11 << 53;

    pub const U_ROO: usize = 0b111_1100_0011 | 0b11 << 53;
    pub const U_RWO: usize = 0b111_0100_0011 | 0b11 << 53;
    pub const U_ROX: usize = 0b111_1100_0011;
    pub const U_RWX: usize = 0b111_0100_0011;
}

impl RvmCfg {
    pub fn detect() -> Self {
        let mmfr0: usize;
        unsafe { asm!("mrs {}, ID_AA64MMFR0_EL1", out(reg) mmfr0); }

        let tgran4  = (mmfr0 >> 28) & 0xf;
        let tgran16 = (mmfr0 >> 20) & 0xf;
        let tgran64 = (mmfr0 >> 24) & 0xf;

        let psz = if tgran4 != 0xf {
            PageSize::Size4kiB
        } else if tgran16 != 0 {
            PageSize::Size16kiB
        } else if tgran64 != 0xf {
            PageSize::Size64kiB
        } else {
            panic!("No supported page granule found");
        };

        let mmfr2: usize;
        unsafe { asm!("mrs {}, ID_AA64MMFR2_EL1", out(reg) mmfr2); }
        let va_range = (mmfr2 >> 16) & 0xf;

        let va_bits = if va_range == 0 {
            48
        } else {
            match psz {
                PageSize::Size4kiB  if tgran4  != 0x1 => 48,
                PageSize::Size16kiB if tgran16 != 0x2 => 48,
                _ => 52
            }
        };

        let ips = mmfr0 & 0xf;
        let pa_bits = match ips {
            0 => 32, 1 => 36, 2 => 40, 3 => 42,
            4 => 44, 5 => 48, 6 => 52, _ => 48
        };

        return Self { psz, va_bits, pa_bits };
    }

    fn tcr_el1(&self) -> usize {
        let tnsz = usize::BITS as usize - self.va_bits as usize;

        let (tg0, tg1) = match self.psz {
            PageSize::Size4kiB  => (0b00, 0b10),
            PageSize::Size16kiB => (0b10, 0b01),
            PageSize::Size64kiB => (0b01, 0b11)
        };

        let mut tcr = 0;
        tcr |= tnsz | (tnsz << 16); // T0SZ and T1SZ
        tcr |= (tg0 << 14) | (tg1 << 30); // TG0 and TG1

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
                "tlbi vmalle1",
                "dsb sy",
                "isb",

                "msr mair_el1, {mair}",
                "msr tcr_el1, {tcr}",
                "msr ttbr0_el1, {ttbr0}",
                "msr ttbr1_el1, {ttbr0}",
                "isb",

                "mrs x0, sctlr_el1",      // set ...
                "orr x0, x0, #(1 << 0)",  // M bit (turn on RAM Virtualisation Controller)
                "orr x0, x0, #(1 << 2)",  // C bit (turn on Data cache)
                "orr x0, x0, #(1 << 12)", // I bit (turn on Instruction cache)
                "msr sctlr_el1, x0",      // ... write back
                "isb",

                "tlbi vmalle1",
                "ic iallu",
                "dsb sy",
                "isb",
                mair = in(reg) mair_el1,
                tcr = in(reg) self.cfg().tcr_el1(),
                ttbr0 = in(reg) self.root_table(),
                out("x0") _,
            );
        }
    }

    pub fn flush(&self, va: usize) {
        let tlbi_va = va >> self.cfg().psz.shift();
        unsafe {
            asm!(
                "tlbi vale1, {va}",
                "dsb ish",
                "isb",
                va = in(reg) tlbi_va
            );
        }
    }

    pub fn is_active(&self) -> bool {
        let ptr: usize;
        unsafe {
            asm!(
                "mrs {}, ttbr0_el1",
                out(reg) ptr
            );
        }
        return ptr == self.root_table() as usize;
    }

    pub fn activate(&self) {
        unsafe {
            asm!(
                "msr ttbr0_el1, {ttbr0}",
                "msr ttbr1_el1, {ttbr0}",
                "isb",

                "tlbi vmalle1",
                "ic iallu",
                "dsb sy",
                "isb",
                ttbr0 = in(reg) self.root_table()
            );
        }
    }
}
