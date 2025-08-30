use crate::{
    ram::glacier::{GlacierData, MMUCfg, PageSize},
    sysinfo::ramtype
};

#[allow(dead_code)]
pub mod flags {
    // Descriptor type bits [1:0]
    pub const VALID: usize       = 1 << 0;  // Present bit
    const AF: usize              = 1 << 10; // Access Flag
    const NEXT_DESC: usize       = 1 << 1;  // Next descriptor bit

    // Attributes
    const ATTR_IDX_NORMAL: usize = 0 << 2;
    const ATTR_IDX_DEVICE: usize = 1 << 2;

    // Access permissions
    const AP_EL1: usize          = 0 << 6; // EL1 only
    const AP_EL0: usize          = 1 << 6; // Both EL1 and EL0
    const READ_ONLY: usize       = 1 << 7;

    // Shareability
    const SH_NONE: usize         = 0b00 << 8;
    const SH_OUTER: usize        = 0b10 << 8;
    const SH_INNER: usize        = 0b11 << 8;

    // Other flags
    const NG: usize              = 1 << 11; // Not global
    const UXN: usize             = 1 << 54; // Unprivileged execute never
    const PXN: usize             = 1 << 53; // Privileged execute never

    pub const NEXT_TABLE: usize    = VALID | NEXT_DESC;
    pub const PAGE_DEFAULT: usize  = VALID | NEXT_DESC | AF | ATTR_IDX_NORMAL | SH_INNER | AP_EL1;
    pub const PAGE_NOEXEC: usize   = VALID | NEXT_DESC | AF | ATTR_IDX_NORMAL | SH_INNER | AP_EL1 | UXN | PXN;
    pub const PAGE_DEVICE: usize   = VALID | NEXT_DESC | AF | ATTR_IDX_DEVICE | SH_NONE | AP_EL1 | UXN | PXN;
    pub const LARGE_DEFAULT: usize = VALID | AF | ATTR_IDX_NORMAL | SH_INNER | AP_EL1;
    pub const LARGE_NOEXEC: usize  = VALID | AF | ATTR_IDX_NORMAL | SH_INNER | AP_EL1 | UXN | PXN;
    pub const LARGE_DEVICE: usize  = VALID | AF | ATTR_IDX_DEVICE | SH_NONE | AP_EL1;
}

pub fn flags_for_type(ty: u32) -> usize {
    use flags::*;
    match ty {
        ramtype::CONVENTIONAL => PAGE_DEFAULT,
        ramtype::BOOT_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::RUNTIME_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::KERNEL => PAGE_DEFAULT,
        ramtype::KERNEL_DATA => PAGE_NOEXEC,
        ramtype::KERNEL_PAGE_TABLE => PAGE_NOEXEC,
        ramtype::MMIO => PAGE_DEVICE,
        _ => PAGE_NOEXEC
    }
}

impl MMUCfg {
    pub fn detect() -> Self {
        let page_size = PageSize::Size4kiB;
        let va_bits = 48;
        let mut mmfr0: usize;
        unsafe { core::arch::asm!("mrs {}, ID_AA64MMFR0_EL1", out(reg) mmfr0); }
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

pub fn identity_map(glacier: &GlacierData) {
    // Attr0 = Normal memory, Inner/Outer Write-Back Non-transient
    // Attr1 = Device memory nGnRnE
    let mair_el1: u64 = 0xff | (0x00 << 8);

    unsafe {
        core::arch::asm!(
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
            tcr = in(reg) glacier.cfg().tcr_el1(),
            ttbr0 = in(reg) glacier.root_table()
        );
    }
}