use core::arch::{asm, global_asm};
use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use seq_macro::seq;
use spin::RwLock;

unsafe extern "C" {
    unsafe fn syscall();
}

seq!(N in 0..256 {
    unsafe extern "C" {
        #(fn isr_stub_~N();)*
    }

    static ISR_STUBS: [unsafe extern "C" fn(); 256] = [
        #(isr_stub_~N,)*
    ];
});

macro_rules! isr_nonerr {
    ($n:tt) => {
        concat!(
            ".global isr_stub_", stringify!($n), "\n",
            "isr_stub_", stringify!($n), ":\n",
            "push 0\n",
            "push ", stringify!($n), "\n",
            "jmp isr_cmm\n"
        )
    };
}

macro_rules! isr_err {
    ($n:tt) => {
        concat!(
            ".global isr_stub_", stringify!($n), "\n",
            "isr_stub_", stringify!($n), ":\n",
            "push ", stringify!($n), "\n",
            "jmp isr_cmm\n"
        )
    };
}

// 0-31: CPU exceptions
global_asm!(
    isr_nonerr!(0),  isr_nonerr!(1),  isr_nonerr!(2),  isr_nonerr!(3),
    isr_nonerr!(4),  isr_nonerr!(5),  isr_nonerr!(6),  isr_nonerr!(7),
    isr_err!(8),     isr_nonerr!(9),  isr_err!(10),    isr_err!(11),
    isr_err!(12),    isr_err!(13),    isr_err!(14),    isr_nonerr!(15),
    isr_nonerr!(16), isr_err!(17),    isr_nonerr!(18), isr_nonerr!(19),
    isr_nonerr!(20), isr_err!(21),    isr_nonerr!(22), isr_nonerr!(23),
    isr_nonerr!(24), isr_nonerr!(25), isr_nonerr!(26), isr_nonerr!(27),
    isr_nonerr!(28), isr_err!(29),    isr_err!(30),    isr_nonerr!(31)
);

// 32-255: IRQs / S/W inter
seq!(N in 32..256 {
    global_asm!(#(isr_nonerr!(N),)*);
});

macro_rules! call_handler {
    () => { concat!(
        "push rax\n", "push rbx\n", "push rcx\n", "push rdx\n",
        "push rsi\n", "push rdi\n", "push rbp\n",
        "push r8\n",  "push r9\n",  "push r10\n", "push r11\n",
        "push r12\n", "push r13\n", "push r14\n", "push r15\n",
        "sub rsp, 272\n",
        "movaps [rsp + 0x00], xmm0\n",  "movaps [rsp + 0x10], xmm1\n",
        "movaps [rsp + 0x20], xmm2\n",  "movaps [rsp + 0x30], xmm3\n",
        "movaps [rsp + 0x40], xmm4\n",  "movaps [rsp + 0x50], xmm5\n",
        "movaps [rsp + 0x60], xmm6\n",  "movaps [rsp + 0x70], xmm7\n",
        "movaps [rsp + 0x80], xmm8\n",  "movaps [rsp + 0x90], xmm9\n",
        "movaps [rsp + 0xa0], xmm10\n", "movaps [rsp + 0xb0], xmm11\n",
        "movaps [rsp + 0xc0], xmm12\n", "movaps [rsp + 0xd0], xmm13\n",
        "movaps [rsp + 0xe0], xmm14\n", "movaps [rsp + 0xf0], xmm15\n",
        "stmxcsr [rsp + 0x100]\n",

        "mov rdi, [rsp + 392]\n",
        "lea rsi, [rsp]\n",
        "call exc_handler\n",

        "ldmxcsr [rsp + 0x100]\n",
        "movaps xmm0, [rsp + 0x00]\n",  "movaps xmm1, [rsp + 0x10]\n",
        "movaps xmm2, [rsp + 0x20]\n",  "movaps xmm3, [rsp + 0x30]\n",
        "movaps xmm4, [rsp + 0x40]\n",  "movaps xmm5, [rsp + 0x50]\n",
        "movaps xmm6, [rsp + 0x60]\n",  "movaps xmm7, [rsp + 0x70]\n",
        "movaps xmm8, [rsp + 0x80]\n",  "movaps xmm9, [rsp + 0x90]\n",
        "movaps xmm10, [rsp + 0xa0]\n", "movaps xmm11, [rsp + 0xb0]\n",
        "movaps xmm12, [rsp + 0xc0]\n", "movaps xmm13, [rsp + 0xd0]\n",
        "movaps xmm14, [rsp + 0xe0]\n", "movaps xmm15, [rsp + 0xf0]\n",
        "add rsp, 272\n",
        "pop r15\n", "pop r14\n", "pop r13\n", "pop r12\n",
        "pop r11\n", "pop r10\n", "pop r9\n",  "pop r8\n",
        "pop rbp\n", "pop rdi\n", "pop rsi\n", "pop rdx\n",
        "pop rcx\n", "pop rbx\n", "pop rax\n"
    )}
}

global_asm!(
    ".global syscall",
    "syscall:", // syscall entry
        "swapgs",
        "mov gs:[0], rsp",
        "mov rsp, gs:[8]",
        // additional pushes to match interframe layout
        "push 0x1b",
        "push qword ptr gs:[0]",
        "push r11",
        "push 0x23",
        "push rcx",
        "push 0",
        "push 0x80",
        call_handler!(),
        "add rsp, 16",
        "pop rcx",
        "add rsp, 8",
        "pop r11",
        "pop rsp",
        "swapgs",
        "sysretq",

    "isr_cmm:", // interrupt entry
        call_handler!(),
        "add rsp, 16",
        "iretq"
);

const GDT: [[u8; 8]; 5] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    [0xff, 0xff, 0x00, 0x00, 0x00, 0x9a, 0xaf, 0x00],
    [0xff, 0xff, 0x00, 0x00, 0x00, 0x92, 0xaf, 0x00],
    [0xff, 0xff, 0x00, 0x00, 0x00, 0xf2, 0xaf, 0x00],
    [0xff, 0xff, 0x00, 0x00, 0x00, 0xfa, 0xaf, 0x00]
];

#[repr(C, packed)]
struct GdtPtr {
    limit: u16,
    base: u64
}

#[repr(C)]
struct GlobDescTbl {
    null: [u8; 8],
    code: [u8; 8],
    data: [u8; 8],
    code64: [u8; 8],
    data64: [u8; 8],
    tss: [u8; 16]
}

impl GlobDescTbl {
    const fn new() -> Self {
        return Self {
            null: GDT[0],
            code: GDT[1],
            data: GDT[2],
            code64: GDT[3],
            data64: GDT[4],
            tss: [0u8; 16]
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct TaskStatSeg {
    _0: u32, rsp0: u64, rsp1: u64, rsp2: u64,
    _1: u64, ist1: u64, ist2: u64, ist3: u64,
    ist4: u64, ist5: u64, ist6: u64, ist7: u64,
    _2: u64, _3: u16, iomap_base: u16
}

impl TaskStatSeg {
    const fn new() -> Self {
        return Self {
            _0: 0, rsp0: 0, rsp1: 0, rsp2: 0,
            _1: 0, ist1: 0, ist2: 0, ist3: 0,
            ist4: 0, ist5: 0, ist6: 0, ist7: 0,
            _2: 0, _3: 0, iomap_base: size_of::<Self>() as u16
        }
    }
}

// Per-CPU data for swapgs
#[repr(C)]
pub struct PerCpuData {
    pub user_rsp: u64,
    pub kernel_rsp: u64
}

struct CPUDesc {
    gdt: GlobDescTbl,
    tss: TaskStatSeg,
    percpu: PerCpuData
}

impl CPUDesc {
    const fn new() -> Self {
        return Self {
            gdt: GlobDescTbl::new(),
            tss: TaskStatSeg::new(),
            percpu: PerCpuData { user_rsp: 0, kernel_rsp: 0 }
        };
    }

    fn load_tss(&mut self) {
        let tss_addr = &raw const self.tss as u64;
        let limit = (core::mem::size_of::<TaskStatSeg>() - 1) as u32;

        let tss_addr_bytes = tss_addr.to_ne_bytes();
        let limit_bytes = limit.to_ne_bytes();

        self.gdt.tss[0..2].copy_from_slice(&limit_bytes[0..2]);
        self.gdt.tss[2..5].copy_from_slice(&tss_addr_bytes[0..3]);
        self.gdt.tss[5] = 0x89; // present, type=32
        self.gdt.tss[6] = limit_bytes[2] & 0x0f;
        self.gdt.tss[7..12].copy_from_slice(&tss_addr_bytes[3..8]);
    }

    fn load(&mut self, stack_top: usize) {
        self.tss.rsp0 = stack_top as u64;
        self.percpu.kernel_rsp = stack_top as u64;
        self.percpu.user_rsp = 0;
        self.load_tss();

        let gdtr = GdtPtr {
            limit: (core::mem::size_of::<GlobDescTbl>() - 1) as u16,
            base: &raw const self.gdt as u64
        };
        let percpu_addr = &raw const self.percpu as u64;

        unsafe {
            asm!(
                "lgdt [{gdtr}]",
                "push 0x08",
                "lea rax, [rip + 2f]",
                "push rax",
                "retfq",
                "2:",
                "mov ax, 0x10",
                "mov ds, ax",
                "mov es, ax",
                "mov fs, ax",
                "mov gs, ax",
                "mov ss, ax",
                "ltr {tss:x}",
                gdtr = in(reg) &gdtr,
                tss = in(reg) 0x28u16,
                options(nostack)
            );

            asm!(
                "wrmsr",
                in("ecx") 0xc0000102u32,
                in("eax") percpu_addr as u32,
                in("edx") (percpu_addr >> 32) as u32,
                options(nostack)
            );
        }
    }
}

static CPU_DESCS: RwLock<BTreeMap<usize, Box<CPUDesc>>> = RwLock::new(BTreeMap::new());

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IdtEnt {
    off_lo: u16,
    sel: u16,
    ist: u8,
    attr: u8,
    off_mid: u16,
    off_hi: u32,
    _0: u32
}

impl IdtEnt {
    const fn new() -> Self {
        return Self { off_lo: 0, sel: 0, ist: 0, attr: 0, off_mid: 0, off_hi: 0, _0: 0 };
    }

    fn set(&mut self, handler: u64, sel: u16, ist: u8, attr: u8) {
        self.off_lo = handler as u16;
        self.off_mid = (handler >> 16) as u16;
        self.off_hi = (handler >> 32) as u32;
        self.sel = sel;
        self.ist = ist;
        self.attr = attr;
    }
}

static IDT: RwLock<[IdtEnt; 256]> = RwLock::new([IdtEnt::new(); 256]);

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InterFrame {
    pub xmm: [u128; 16],
    pub mxcsr: u64,
    pub _0: u64,
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rbp: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
    pub rcx: u64, pub rbx: u64, pub rax: u64,
    pub vec: u64, pub err: u64,
    pub rip: u64, pub cs: u64, pub rflags: u64, pub rsp: u64, pub ss: u64
}

#[unsafe(no_mangle)]
extern "C" fn exc_handler(exc_type: u64, frame: &mut InterFrame) {
    match exc_type { // exc_type == frame.vec
        // // CPU EXCEPTIONS
        // 0  => { /* #DE divide error             */ }
        // 1  => { /* #DB debug                    */ }
        // 2  => { /* #NMI NON-MASKABLE INTERRUPT  */ }
        // 3  => { /* #BP breakpoint               */ }
        // 4  => { /* #OF overflow                 */ }
        // 5  => { /* #BR bound range              */ }
        // 6  => { /* #UD invalid opcode           */ }
        // 7  => { /* #NM device not available     */ }
        // 8  => { /* #DF double fault             */ }
        // 10 => { /* #TS invalid TSS              */ }
        // 11 => { /* #NP segment not present      */ }
        // 12 => { /* #SS stack segment fault      */ }
        // 13 => { /* #GP general protection fault */ }
        // 14 => { /* #PF page fault               */ }
        // 16 => { /* #MF FPU error                */ }
        // 17 => { /* #AC alignment check          */ }
        // 18 => { /* #MC machine check            */ }
        // 19 => { /* #XM SIMD exception           */ }
        // 20 => { /* #VE virtualisation           */ }
        // 21 => { /* #CP control protection       */ }

        // // AMD SPECIFIC
        // 29 => { /* #VC VMM communication exception */ }
        // 30 => { /* #SX security exception          */ }
        // // END AMD SPECIFIC

        // ..32 => { /* reserved by Intel */ }
        // // END OF CPU EXCEPTIONS

        128 => { /* syscall */
            frame.rax = crate::kreq::kernel_requestee(
                frame.rax as *const u8,
                frame.rdi as usize, frame.rsi as usize, frame.rdx as usize,
                frame.r10 as usize, frame.r8 as usize, frame.r9 as usize
            ) as u64;
        }
        ..256 => { /* reserved or IRQ */
            crate::printlnk!("Exception type: {}", exc_type);
            crate::printlnk!("Exception frame: {:#x?}", frame);

            panic!("Unhandled exception");
        }
        _  => unreachable!()
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
            asm!("sti", options(nomem, nostack, preserves_flags));
        } else {
            asm!("cli", options(nomem, nostack, preserves_flags));
        }
    }
}

pub fn init() {
    let mut desc = Box::new(CPUDesc::new());
    desc.load(crate::ram::stack_top());
    CPU_DESCS.write().insert(crate::kargs::ap_vid(), desc);

    unsafe {
        // IDT
        let mut idt = IDT.write();
        for i in 0..256 {
            let handler = ISR_STUBS[i] as u64;
            idt[i].set(handler, 0x08, 0, 0x8e);
        }

        let idtr = IdtPtr {
            limit: (size_of::<[IdtEnt; 256]>() - 1) as u16,
            base: idt.as_ptr() as u64
        };

        asm!("lidt [{}]", in(reg) &idtr, options(nostack, preserves_flags));

        // syscall

        let efer: u64;
        asm!(
            "rdmsr",
            in("ecx") 0xc0000080u32,
            out("eax") efer,
            out("edx") _,
            options(nomem, nostack, preserves_flags)
        );
        asm!(
            "wrmsr",
            in("ecx") 0xc0000080u32,
            in("eax") efer | 1,
            in("edx") 0u32,
            options(nomem, nostack, preserves_flags)
        );

        let entry = syscall as *const () as usize;
        asm!(
            "wrmsr",
            in("ecx") 0xc0000082u32,
            in("eax") entry as u32,
            in("edx") (entry >> 32) as u32
        );

        let star: u64 = (0x10 << 48) | (0x08 << 32);
        asm!(
            "wrmsr",
            in("ecx") 0xc0000081u32,
            in("eax") star as u32,
            in("edx") (star >> 32) as u32
        );
        asm!(
            "wrmsr",
            in("ecx") 0xc0000084u32,
            in("eax") 0x200,
            in("edx") 0
        );
    }
}
