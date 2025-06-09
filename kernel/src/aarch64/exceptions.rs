#[repr(C, align(2048))]
pub struct ExceptionVector { pub data: [u32; 512] }

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.exceptions")]
pub static mut EXCEPTION_VECTOR: ExceptionVector = ExceptionVector { data: [0; 512] };

pub fn init_exceptions() {
    let xvec = unsafe { (&raw mut EXCEPTION_VECTOR).as_mut().unwrap() };
    const LDR_X16_PC_REL: u32 = 0x58000010; // ldr x16, #offset
    const BR_X16: u32         = 0xd61f0200; // br x16

    let handlers = [
        sync_el1h,  irq_el1h,  fiq_el1h,  serr_el1h,
        sync_el1t,  irq_el1t,  fiq_el1t,  serr_el1t,
        sync_el0,   irq_el0,   fiq_el0,   serr_el0,
        sync_el0_2, irq_el0_2, fiq_el0_2, serr_el0_2
    ];
    // let xvec_addr = xvec.data.as_ptr() as usize;

    for i in 0..handlers.len() {
        let slot_index = i * 2;
        let handler_addr = handlers[i] as usize;
        let slot_addr = &raw const xvec.data[slot_index] as usize;
        let pc = slot_addr + 8;

        let offset = ((handler_addr as isize - pc as isize) / 4) as i32;
        assert!(offset >= -(1 << 18) && offset < (1 << 18), "offset out of LDR range");

        let ldr = LDR_X16_PC_REL | (((offset as u32) & 0x7ffff) << 5);
        xvec.data[slot_index]     = ldr;
        xvec.data[slot_index + 1] = BR_X16;

        let base = 512 - 16 * 2;
        xvec.data[base + i * 2]     = (handler_addr & 0xffff_ffff) as u32;
        xvec.data[base + i * 2 + 1] = (handler_addr >> 32) as u32;
    }

    unsafe {
        core::arch::asm!("msr vbar_el1, {}", in(reg) &raw const EXCEPTION_VECTOR);
        core::arch::asm!("dsb sy");
        core::arch::asm!("isb");
    }
}

macro_rules! handler {
    ($name:ident, $msg:expr) => {
        #[unsafe(no_mangle)]
        #[unsafe(link_section = ".text.exceptions")]
        extern "C" fn $name() {
            super::serial_puts($msg);
            loop { super::halt(); }
        }
    };
}

handler!(sync_el1h,  "[EXC] sync_el1h\n");
handler!(irq_el1h,   "[EXC] irq_el1h\n");
handler!(fiq_el1h,   "[EXC] fiq_el1h\n");
handler!(serr_el1h,  "[EXC] serr_el1h\n");
handler!(sync_el1t,  "[EXC] sync_el1t\n");
handler!(irq_el1t,   "[EXC] irq_el1t\n");
handler!(fiq_el1t,   "[EXC] fiq_el1t\n");
handler!(serr_el1t,  "[EXC] serr_el1t\n");
handler!(sync_el0,   "[EXC] sync_el0\n");
handler!(irq_el0,    "[EXC] irq_el0\n");
handler!(fiq_el0,    "[EXC] fiq_el0\n");
handler!(serr_el0,   "[EXC] serr_el0\n");
handler!(sync_el0_2, "[EXC] sync_el0_2\n");
handler!(irq_el0_2,  "[EXC] irq_el0_2\n");
handler!(fiq_el0_2,  "[EXC] fiq_el0_2\n");
handler!(serr_el0_2, "[EXC] serr_el0_2\n");