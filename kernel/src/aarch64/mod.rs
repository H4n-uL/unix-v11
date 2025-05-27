mod exceptions;

use crate::{ember::ramtype, ram::PAGE_4KIB, ramblock::{RAMBlockManager, RBPtr}, EMBER};
use aarch64_cpu::{asm::wfi, registers::DAIF};
pub use exceptions::init_exceptions;
use spin::MutexGuard;
use tock_registers::interfaces::{Readable, Writeable};

fn set_interrupts(enabled: bool) {
    if enabled { DAIF.set(DAIF.get() & !0b1111); }
    else { DAIF.set(DAIF.get() | 0b1111); }
}

pub fn halt() {
    set_interrupts(false);
    wfi();
}

const UART0_BASE: usize = 0x0900_0000; // QEMU virt PL011 UART

pub fn init_serial() {
    unsafe {
        // Disable UART
        core::ptr::write_volatile((UART0_BASE + 0x30) as *mut u32, 0x0);
        // Clear all pending interrupts
        core::ptr::write_volatile((UART0_BASE + 0x44) as *mut u32, 0x7ff);
        // Enable UART, TX, RX
        core::ptr::write_volatile((UART0_BASE + 0x30) as *mut u32, 0x301); // UARTCR: UARTEN|TXE|RXE
    }
}

pub fn serial_putchar(c: u8) {
    unsafe {
        while core::ptr::read_volatile((UART0_BASE + 0x18) as *const u32) & (1 << 5) != 0 {}
        core::ptr::write_volatile((UART0_BASE + 0x00) as *mut u32, c as u32);
    }
}

pub fn serial_puts(s: &str) {
    for byte in s.bytes() { serial_putchar(byte); }
}

pub fn serial_puthex(n: usize) {
    serial_puts("0x");
    if n == 0 { serial_putchar(b'0'); return; }
    let mut leading = true;
    for i in (0..16).rev() {
        let nibble = (n >> (i << 2)) & 0xf;
        if nibble != 0 { leading = false; }
        if !leading { serial_putchar(b"0123456789abcdef"[nibble]); }
    }
}

pub struct SerialWriter;

impl core::fmt::Write for SerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        serial_puts(s);
        Ok(())
    }
}

const VALID: u64           = 1 << 0;
const PAGE_DESC: u64       = 1 << 1;
const ATTR_IDX_NORMAL: u64 = 0 << 2;
const ATTR_IDX_DEVICE: u64 = 1 << 2;
const AP_RW_EL1: u64       = 0b00 << 6;
const SH_NONE: u64         = 0b00 << 8;
const SH_INNER: u64        = 0b11 << 8;
const AF: u64              = 1 << 10;
const UXN: u64 = 1 << 54;
const PXN: u64 = 1 << 53;

const PAGE_DEFAULT: u64 = AF | ATTR_IDX_NORMAL | SH_INNER | AP_RW_EL1;
const PAGE_NOEXEC: u64  = PAGE_DEFAULT | UXN | PXN;
const PAGE_DEVICE: u64 =  AF | ATTR_IDX_DEVICE | SH_NONE  | AP_RW_EL1 | UXN | PXN;

fn get_page_idx(level: usize, virt: u64) -> usize {
    match level {
        0 => ((virt >> 39) & 0x1ff) as usize,
        1 => ((virt >> 30) & 0x1ff) as usize,
        2 => ((virt >> 21) & 0x1ff) as usize,
        3 => ((virt >> 12) & 0x1ff) as usize,
        _ => unreachable!(),
    }
}

pub unsafe fn map_page(l0: *mut u64, virt: u64, phys: u64, flags: u64, ramblock: &mut RAMBlockManager) {
    let virt = virt & 0x0000_ffff_ffff_f000;
    let phys = phys & 0x0000_ffff_ffff_f000;

    let mut table = l0;
    for level in 0..4 {
        let index = get_page_idx(level, virt);
        let entry = table.add(index);
        if level == 3 { *entry = phys | VALID | PAGE_DESC | flags; }
        else {
            table = if *entry & VALID == 0 {
                let next_phys = ramblock.alloc(PAGE_4KIB, ramtype::PAGE_TABLE)
                    .expect("[ERROR] alloc for page table failed!\n");
                core::ptr::write_bytes(next_phys.ptr::<*mut u8>(), 0, PAGE_4KIB);
                *entry = next_phys.addr() as u64 | VALID;
                next_phys.ptr()
            }
            else { (*entry & 0x0000_ffff_ffff_f000) as *mut u64 };
        }
    }
}

fn flags_for(ty: u32) -> u64 {
    match ty {
        ramtype::CONVENTIONAL => PAGE_DEFAULT,
        ramtype::BOOT_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::RUNTIME_SERVICES_CODE => PAGE_DEFAULT,
        ramtype::KERNEL       => PAGE_DEFAULT,
        ramtype::KERNEL_DATA  => PAGE_NOEXEC,
        ramtype::PAGE_TABLE   => PAGE_NOEXEC,
        ramtype::MMIO         => PAGE_DEVICE,
        _                     => PAGE_NOEXEC
    }
}

const ENTRIES_PER_TABLE: usize = 0x200;

// Not working yet, I rly hate AArch64 MMU
pub unsafe fn identity_map(ramblock: &mut MutexGuard<'_, RAMBlockManager>) {
    let ember = EMBER.lock();
    let ram_size = ember.layout_total() as u64;

    let num_4kib_pages = (ram_size as usize + PAGE_4KIB - 1) / PAGE_4KIB;
    let num_l3 = (num_4kib_pages + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_l2 = (num_l3 + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;
    let num_l1 = (num_l2 + ENTRIES_PER_TABLE - 1) / ENTRIES_PER_TABLE;

    let total_tables = 1 + num_l1 + num_l2 + num_l3;
    let table_size = (total_tables * 3) * PAGE_4KIB;

    let l0 = ramblock.reserve_as(
        table_size, ramtype::CONVENTIONAL, ramtype::PAGE_TABLE, false
    ).unwrap();
    core::ptr::write_bytes(l0.ptr::<*mut u8>(), 0, table_size);
    let _ = ramblock.alloc(PAGE_4KIB, ramtype::PAGE_TABLE);

    for desc in ember.ram_layout() {
        let block_ty = desc.ty;
        let block_start = desc.phys_start;
        let block_end = block_start + desc.page_count * PAGE_4KIB as u64;

        for phys in (block_start..block_end).step_by(PAGE_4KIB) {
            map_page(l0.ptr(), phys, phys, flags_for(block_ty), ramblock);
        }
    }

    let mut mmfr0: u64;
    core::arch::asm!("mrs {}, ID_AA64MMFR0_EL1", out(reg) mmfr0);
    let parange = mmfr0 & 0xf;
    // 0 = 32 bits, 1 = 36 bits, 2 = 40 bits
    // 3 = 42 bits, 4 = 44 bits, 5 = 48 bits

    // MAIR_EL1 = Attr0 (normal WB/WA), Attr1 (device nGnRE)
    let mair_el1: u64 = (0b1111_1111 << 0) | (0b0000_0100 << 8);

    // TCR_EL1
    let tcr_el1: u64 =
          (16 << 0)     // T0SZ: 48-bit VA
        | (0b01 << 8)   // ORGN0 = WB/WA
        | (0b01 << 10)  // IRGN0 = WB/WA
        | (0b11 << 12)  // SH0 = Inner Shareable
        | (0b00 << 14)  // TG0 = 4 KiB granule
        | (0b10 << 30)  // TG1 = 4 KiB granule
        | (parange << 32) // IPS = PARange
    ;

    core::arch::asm!("
        // Set up registers for MMU
        mov x1, {0} // MAIR_EL1
        mov x2, {1} // TCR_EL1
        mov x3, {2} // TTBR0_EL1

        // Disable MMU
        mrs x0, sctlr_el1
        bic x0, x0, #1
        msr sctlr_el1, x0
        isb

        // Invalidate TLB
        tlbi vmalle1
        dsb sy
        isb

        // Set up MMU
        msr mair_el1, x1
        msr tcr_el1, x2
        msr ttbr0_el1, x3
        isb

        // Enable MMU
        mrs x0, sctlr_el1
        orr x0, x0, #1         // M = 1: MMU enable
        orr x0, x0, #(1 << 2)  // C = 1: Data cache
        orr x0, x0, #(1 << 12) // I = 1: Instruction cache
        msr sctlr_el1, x0
        isb
    ",
        in(reg) mair_el1,
        in(reg) tcr_el1,
        in(reg) l0.addr()
    );
}

#[inline(always)]
pub fn stack_ptr() -> *const u8 {
    let sp: usize;
    unsafe { core::arch::asm!("mov {}, sp", out(reg) sp); }
    return sp as *const u8;
}

pub unsafe fn move_stack(ptr: &RBPtr, size: usize) {
    let mut ember = EMBER.lock();
    let stack_ptr = stack_ptr();
    let old_stack_base = ember.stack_base;
    let stack_size = old_stack_base - stack_ptr as usize;

    let new_stack_base = ptr.addr() + size;
    let new_stack_bottom = (new_stack_base - stack_size) as *mut u8;

    core::ptr::copy(stack_ptr, new_stack_bottom, stack_size);
    core::arch::asm!("mov sp, {}", in(reg) new_stack_bottom);

    ember.stack_base = new_stack_base;
}