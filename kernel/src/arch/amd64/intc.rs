use crate::{device::cpu::ic_va, kargs::AP_LIST};

use core::{
    arch::asm,
    sync::atomic::{AtomicU64, Ordering as AtomOrd}
};

const LAPIC_TPR: usize       = 0x080;
const LAPIC_EOI: usize       = 0x0b0;
const LAPIC_SVR: usize       = 0x0f0;
const LAPIC_ICR_LO: usize    = 0x300;
const LAPIC_ICR_HI: usize    = 0x310;
const LAPIC_LVT_TIMER: usize = 0x320;
const LAPIC_LVT_ERROR: usize = 0x370;
const LAPIC_TIMER_ICR: usize = 0x380;
const LAPIC_TIMER_CCR: usize = 0x390;
const LAPIC_TIMER_DCR: usize = 0x3e0;

static TIMER_FREQ: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
fn lapic_read(off: usize) -> u32 {
    unsafe { return ((ic_va() + off) as *const u32).read_volatile(); }
}

#[inline(always)]
fn lapic_write(off: usize, val: u32) {
    unsafe { ((ic_va() + off) as *mut u32).write_volatile(val); }
}

pub fn init() {
    lapic_write(LAPIC_SVR, 0x1ff);
    lapic_write(LAPIC_TPR, 0);
    lapic_write(LAPIC_LVT_TIMER, 32 | (1 << 17));
    lapic_write(LAPIC_LVT_ERROR, 33);

    if AP_LIST.virtid_self() == 0 {
        calibrate_timer();
    }
}

fn calibrate_timer() {
    const PIT_FREQ: u64 = 1_193_182; // twelveth of 14,318,180 Hz crystal oscillator
    const CALIB_MS: u64 = 10;
    let pit_ticks = (PIT_FREQ * CALIB_MS / 1000) as u16;

    unsafe {
        asm!(
            "out 0x61, al",
            "mov al, 0xb0",
            "out 0x43, al",
            "mov al, {lo}",
            "out 0x42, al",
            "mov al, {hi}",
            "out 0x42, al",
            in("al") 0u8,
            lo = in(reg_byte) (pit_ticks & 0xff) as u8,
            hi = in(reg_byte) (pit_ticks >> 8) as u8
        );

        lapic_write(LAPIC_TIMER_DCR, 0x0b);
        lapic_write(LAPIC_TIMER_ICR, 0xffffffff);

        asm!("out 0x61, al", in("al") 1u8);

        loop {
            let status: u8;
            asm!("in al, 0x61", out("al") status);
            if status & 0x20 != 0 { break; }
        }

        let elapsed = 0xffffffffu32 - lapic_read(LAPIC_TIMER_CCR);
        let freq = (elapsed as u64) * 1000 / CALIB_MS;
        TIMER_FREQ.store(freq, AtomOrd::Relaxed);
    }
}

#[inline(always)] // Ack is no-op for AMD64 LAPIC
pub fn ack() -> u32 { return 0; }

#[inline(always)]
pub fn eoi(_intid: u32) {
    lapic_write(LAPIC_EOI, 0);
}

pub fn enable(_intid: u32) {}
pub fn disable(_intid: u32) {}

pub fn send_ipi_others(vector: u32) {
    lapic_write(LAPIC_ICR_HI, 0);
    lapic_write(LAPIC_ICR_LO, (3 << 18) | (vector & 0xff));
}

pub fn send_ipi(vector: u32, target: u32) {
    lapic_write(LAPIC_ICR_HI, target << 24);
    lapic_write(LAPIC_ICR_LO, vector & 0xff);
}

#[inline(always)]
pub fn timer_freq() -> u64 {
    return TIMER_FREQ.load(AtomOrd::Relaxed);
}

#[inline(always)]
pub fn timer_enable() {
    let lvt = lapic_read(LAPIC_LVT_TIMER);
    lapic_write(LAPIC_LVT_TIMER, lvt & !(1 << 16));
}

#[inline(always)]
pub fn timer_disable() {
    let lvt = lapic_read(LAPIC_LVT_TIMER);
    lapic_write(LAPIC_LVT_TIMER, lvt | (1 << 16));
}

#[inline(always)]
pub fn timer_set(ticks: u64) {
    lapic_write(LAPIC_TIMER_ICR, ticks as u32);
}

#[inline(always)]
pub fn timer_set_us(us: u64) {
    let freq = timer_freq();
    if freq > 0 {
        let ticks = us * freq / 1_000_000;
        timer_set(ticks);
    }
}

#[inline(always)]
pub fn timer_set_ms(ms: u64) {
    let freq = timer_freq();
    if freq > 0 {
        let ticks = ms * freq / 1000;
        timer_set(ticks);
    }
}