use crate::{
    arch::{exc::ExcFrame, rvm::flags},
    filesys::vfn::VirtFNode,
    ram::{
        PhysPageBuf,
        glacier::{Glacier, hihalf},
        physalloc::{AllocParams, OwnedPtr, PHYS_ALLOC}
    }
};

use alloc::{
    boxed::Box,
    collections::btree_map::BTreeMap,
    string::String,
    sync::Arc,
    vec::Vec
};
use xmas_elf::{ElfFile, program::Type};

pub struct VRamMap {
    pub va: usize,
    pub pa: usize,
    pub size: usize,
    pub flags: usize
}

#[derive(PartialEq, Eq)]
pub enum ProcState {
    Ready,
    Blocked,
    Sleeping
}

pub struct ProcCtrlBlk {
    pub ppid: usize,

    pub glacier: Glacier,
    pub phys_alloc: Vec<OwnedPtr>,
    pub vram_map: Vec<VRamMap>,
    pub ctxt: Box<ExcFrame>,

    pub state: ProcState,
    pub fds: BTreeMap<usize, Arc<dyn VirtFNode>>
}

fn get_proc_vaset(elf: &ElfFile) -> (usize, usize) {
    let va_base = elf.program_iter()
        .filter(|ph| ph.get_type() == Ok(Type::Load))
        .map(|ph| ph.virtual_addr())
        .min().unwrap() as usize;
    let va_top = elf.program_iter()
        .filter(|ph| ph.get_type() == Ok(Type::Load))
        .map(|ph| ph.virtual_addr() + ph.mem_size())
        .max().unwrap() as usize;

    return (va_base, va_top);
}

impl ProcCtrlBlk {
    pub fn new(node: &dyn VirtFNode, _args: &[&str]) -> Result<Self, String> {
        let read_len = node.meta().size as usize;
        let mut file_bin = PhysPageBuf::new(read_len).ok_or("Failed to allocate buffer")?;
        node.read(&mut file_bin, 0)?;

        let elf = ElfFile::new(&file_bin)?;
        let ep = elf.header.pt2.entry_point() as usize;
        let mut glacier = Glacier::new();

        let (va_base, va_top) = get_proc_vaset(&elf);
        let proc_size = va_top - va_base;

        let mut phys_alloc = Vec::new();

        let proc_ptr = PHYS_ALLOC.alloc(
            AllocParams::new(proc_size)
        ).ok_or("Failed to allocate process memory")?;
        let proc_addr = proc_ptr.addr();
        phys_alloc.push(proc_ptr);

        let mut vram_map = Vec::new();

        for ph in elf.program_iter() {
            if let Ok(Type::Load) = ph.get_type() {
                let offset = ph.offset() as usize;
                let file_size = ph.file_size() as usize;
                let mem_size = ph.mem_size() as usize;
                let virt_addr = ph.virtual_addr() as usize;
                let phys_addr = proc_addr + (virt_addr - va_base);
                let phys_ptr = phys_addr as *mut u8;

                let flags = match ph.flags().0 {
                    0b100 => flags::U_ROO, // read only
                    0b101 => flags::U_ROX, // read & execute
                    0b110 => flags::U_RWO, // read & write
                    0b111 => flags::U_RWX, // read & write & execute
                    _     => flags::U_RWO  // fallback to read & write
                };

                glacier.map_range(
                    virt_addr, phys_addr,
                    mem_size, flags
                ).map_err(|_| "Failed to map process")?;

                vram_map.push(VRamMap {
                    va: virt_addr,
                    pa: phys_addr,
                    size: mem_size,
                    flags
                });

                unsafe {
                    phys_ptr.write_bytes(0, mem_size);
                    file_bin[offset..offset + file_size].as_ptr().copy_to(phys_ptr, file_size);
                }
            }
        }

        let stack_size = 0x100000;
        let stack_ptr = PHYS_ALLOC.alloc(
            AllocParams::new(stack_size)
        ).ok_or("Failed to allocate user stack")?;

        let lohalf_top = 0usize.wrapping_sub(hihalf());
        glacier.map_range(
            lohalf_top - stack_size, stack_ptr.addr(),
            stack_size, flags::U_RWO
        ).map_err(|_| "Failed to map user stack")?;

        vram_map.push(VRamMap {
            va: lohalf_top - stack_size,
            pa: stack_ptr.addr(),
            size: stack_size,
            flags: flags::U_RWO
        });
        phys_alloc.push(stack_ptr);

        let mut ctxt = ExcFrame::new();
        ctxt.set_pc(ep);
        ctxt.set_sp(lohalf_top);

        return Ok(Self {
            ppid: 0,
            glacier,
            phys_alloc,
            vram_map,
            ctxt: Box::new(ctxt),
            state: ProcState::Ready,
            fds: BTreeMap::new()
        });
    }
}

impl Drop for ProcCtrlBlk {
    fn drop(&mut self) {
        for pptr in self.phys_alloc.drain(..) {
            PHYS_ALLOC.free(pptr);
        }
    }
}
