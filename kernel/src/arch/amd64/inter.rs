use core::arch::asm;

const GDT: [u8; 48] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0x00, 0x00, 0x00, 0x9a, 0xcf, 0x00,
    0xff, 0xff, 0x00, 0x00, 0x00, 0x92, 0xcf, 0x00,
    0xff, 0xff, 0x00, 0x00, 0x00, 0xfa, 0xcf, 0x00,
    0xff, 0xff, 0x00, 0x00, 0x00, 0xf2, 0xcf, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
];

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TaskStatSeg {
    _0: u32, rsp0: u64, rsp1: u64, rsp2: u64,
    _1: u64, ist1: u64, ist2: u64, ist3: u64, ist4: u64, ist5: u64, ist6: u64, ist7: u64,
    _2: u64, _3: u16, iomap_base: u16
}

impl TaskStatSeg {
    pub const fn new() -> Self {
        return Self {
            _0: 0, rsp0: 0, rsp1: 0, rsp2: 0,
            _1: 0, ist1: 0, ist2: 0, ist3: 0, ist4: 0, ist5: 0, ist6: 0, ist7: 0,
            _2: 0, _3: 0, iomap_base: size_of::<Self>() as u16
        }
    }
}

pub fn get() -> bool {
    let rflags: u64;
    unsafe {
        asm!("pushfq; pop {}", out(reg) rflags, options(nomem, nostack, preserves_flags));
    }
    return (rflags & (1 << 9)) == 0;
}

pub fn set(enabled: bool) {
    unsafe {
        if enabled {
            asm!("cli", options(nomem, nostack, preserves_flags));
        } else {
            asm!("sti", options(nomem, nostack, preserves_flags));
        }
    }
}
