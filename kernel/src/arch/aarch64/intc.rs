use crate::device::cpu::{GICD_BASE, GICC_BASE, GICR_BASE, ic_va};

use core::{
    arch::asm, hint::spin_loop, num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering as AtomOrd}
};

static GIC_VERSION: AtomicUsize = AtomicUsize::new(0);

// GICv2 GICC reg offsets
const GICC_CTRLR: usize = 0x000;
const GICC_PMR: usize   = 0x004;
const GICC_BPR: usize   = 0x008;
const GICC_IAR: usize   = 0x00c;
const GICC_EOIR: usize  = 0x010;

// GICD common reg offsets
const GICD_CTRLR: usize = 0x000;
const GICD_PIDR2: usize = 0xffe8;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_IPRIORITYR: usize = 0x400;

fn gic_ver() -> usize {
    if let Some(v) = NonZeroUsize::new(
        GIC_VERSION.load(AtomOrd::Relaxed)
    ) {
        return v.get();
    }

    let gicd = GICD_BASE.load(AtomOrd::Relaxed);
    let gicr = GICR_BASE.load(AtomOrd::Relaxed);
    let gicc = GICC_BASE.load(AtomOrd::Relaxed);

    if gicd != 0 {
        let pidr2 = unsafe { ((gicd + GICD_PIDR2) as *const u32).read_volatile() };
        let hw_ver = ((pidr2 >> 4) & 0xf) as usize;
        if hw_ver != 0 {
            GIC_VERSION.store(hw_ver, AtomOrd::Relaxed);
            return hw_ver;
        }
    }

    let v = if gicr != 0 { 3 } else if gicc != 0 { 2 } else { 0 };
    GIC_VERSION.store(v, AtomOrd::Relaxed);
    return v;
}

pub fn init(is_bsp: bool) {
    let v = gic_ver();

    match v {
        2 => init_v2(),
        3 => init_v3(is_bsp),
        _ => crate::printlnk!("Unknown GIC version: {}", v)
    }
}

fn init_v2() {
    let gicd = GICD_BASE.load(AtomOrd::Relaxed);
    let gicc = ic_va();

    unsafe {
        ((gicd + GICD_CTRLR) as *mut u32).write_volatile(1);
        ((gicc + GICC_PMR) as *mut u32).write_volatile(0xff);
        ((gicc + GICC_BPR) as *mut u32).write_volatile(0);
        ((gicc + GICC_CTRLR) as *mut u32).write_volatile(1);
    }
}

fn init_v3(is_bsp: bool) {
    let gicd = GICD_BASE.load(AtomOrd::Relaxed);
    let gicr = GICR_BASE.load(AtomOrd::Relaxed);

    unsafe {
        if is_bsp {
            // enable GICD (ARE_NS 0x10 | EnableGrp1NS 0x2 | EnableGrp0 0x1)
            ((gicd + GICD_CTRLR) as *mut u32).write_volatile(0x13);
        }

        // wkup redistrib
        let gicr_waker = (gicr + 0x14) as *mut u32;
        let mut waker = gicr_waker.read_volatile();
        waker &= !(1 << 1); // clr sleep bit
        gicr_waker.write_volatile(waker);
        while (gicr_waker.read_volatile() & (1 << 2)) != 0 {
            spin_loop();
        }

        asm!(
            "msr ICC_PMR_EL1, {pmr}",
            "msr ICC_BPR1_EL1, {bpr}",
            "msr ICC_IGRPEN1_EL1, {igren}",
            "isb",
            pmr = in(reg) 0xffu64,
            bpr = in(reg) 0u64,
            igren = in(reg) 1u64
        );
    }
}

#[inline(always)]
pub fn ack() -> u32 {
    return match gic_ver() {
        2 => unsafe {
            ((ic_va() + GICC_IAR) as *const u32).read_volatile()
        }
        3 => {
            let intid: u64;
            unsafe { asm!("mrs {}, ICC_IAR1_EL1", out(reg) intid); }
            intid as u32
        }
        _ => 1023
    };
}

#[inline(always)]
pub fn eoi(intid: u32) {
    match gic_ver() {
        2 => unsafe {
            ((ic_va() + GICC_EOIR) as *mut u32).write_volatile(intid);
        }
        3 => unsafe {
            asm!("msr ICC_EOIR1_EL1, {}", in(reg) intid as u64);
        }
        _ => {}
    }
}

pub fn enable(_cpu_idx: usize, intid: u32) {
    let gicd = GICD_BASE.load(AtomOrd::Relaxed);
    let reg_idx = (intid / u32::BITS) as usize;
    let bit = 1u32 << (intid % u32::BITS);
    unsafe {
        ((gicd + GICD_ISENABLER + reg_idx * size_of::<u32>()) as *mut u32).write_volatile(bit);
    }
}

pub fn disable(_cpu_idx: usize, intid: u32) {
    let gicd = GICD_BASE.load(AtomOrd::Relaxed);
    let reg_idx = (intid / u32::BITS) as usize;
    let bit = 1u32 << (intid % u32::BITS);
    unsafe {
        ((gicd + GICD_ICENABLER + reg_idx * size_of::<u32>()) as *mut u32).write_volatile(bit);
    }
}

pub fn set_priority(_cpu_idx: usize, intid: u32, priority: u8) {
    let gicd = GICD_BASE.load(AtomOrd::Relaxed);
    unsafe {
        ((gicd + GICD_IPRIORITYR + intid as usize) as *mut u8).write_volatile(priority);
    }
}

pub fn send_ipi_others(intid: u32) {
    match gic_ver() {
        2 => unsafe {
            // GICD_SGIR: TargetListFilter=01 (wildcard except self)
            let gicd = GICD_BASE.load(AtomOrd::Relaxed);
            ((gicd + 0xf00) as *mut u32).write_volatile((1 << 24) | intid);
        },
        3 => unsafe {
            // ICC_SGI1R_EL1: IRM=1 (wildcard except self)
            let val: u64 = (1 << 40) | ((intid as u64) << 24);
            core::arch::asm!("msr ICC_SGI1R_EL1, {}", in(reg) val);
        },
        _ => {}
    }
}

pub fn send_ipi(intid: u32, target: u32) {
    match gic_ver() {
        2 => unsafe {
            let gicd = GICD_BASE.load(AtomOrd::Relaxed);
            ((gicd + 0xf00) as *mut u32).write_volatile(((target & 0xff) << 16) | intid);
        },
        3 => unsafe {
            let aff = target.to_le_bytes();
            let val = {
                  ((aff[3] as u64) << 48)
                | ((aff[2] as u64) << 32)
                | ((aff[1] as u64) << 16)
                | ((intid as u64)  << 24)
                | (1 << aff[0])
            };
            asm!("msr ICC_SGI1R_EL1, {}", in(reg) val);
        },
        _ => {}
    }
}

#[inline(always)]
pub fn timer_freq() -> u64 {
    let freq: u64;
    unsafe { asm!("mrs {}, CNTFRQ_EL0", out(reg) freq); }
    return freq;
}

#[inline(always)]
pub fn timer_enable() {
    unsafe {
        asm!("msr CNTP_CTL_EL0, {}", in(reg) 1u64);
    }
}

#[inline(always)]
pub fn timer_disable() {
    unsafe {
        asm!("msr CNTP_CTL_EL0, {}", in(reg) 0u64);
    }
}

#[inline(always)]
pub fn timer_set(ticks: u64) {
    unsafe {
        asm!("msr CNTP_TVAL_EL0, {}", in(reg) ticks);
    }
}

#[inline(always)]
pub fn timer_set_us(us: u64) {
    let freq = timer_freq();
    let ticks = us * freq / 1000000;
    timer_set(ticks);
}

#[inline(always)]
pub fn timer_set_ms(ms: u64) {
    let freq = timer_freq();
    let ticks = ms * freq / 1000;
    timer_set(ticks);
}
