pub mod ctrlblk;

use crate::{
    arch, filesys::VFS, kargs::AP_LIST,
    printlnk,
    proc::ctrlblk::ProcCtrlBlk,
    ram::{glacier::GLACIER, stack_top}
};

use alloc::{
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    string::String
};
use spin::RwLock;

pub struct ProcTables {
    pub running: BTreeMap<usize, ProcCtrlBlk>,
    pub ready: VecDeque<ProcCtrlBlk>,
    pub blocked: VecDeque<ProcCtrlBlk>,
    pub sleeping: VecDeque<ProcCtrlBlk>
}

impl ProcTables {
    const fn new() -> Self {
        Self {
            running: BTreeMap::new(),
            ready: VecDeque::new(),
            blocked: VecDeque::new(),
            sleeping: VecDeque::new()
        }
    }
}

pub static PROCS: RwLock<ProcTables> = RwLock::new(ProcTables::new());

pub fn exec_aleph() {
    let path = "/mnt/block0p0/sbin/aleph";

    VFS.walk(path).and_then(|node| {
        let proc = ProcCtrlBlk::new(&*node, &[path])?;
        exec_proc(proc)?;
        Ok(())
    }).unwrap_or_else(|err| {
        printlnk!("Failed to exec {}: {:?}", path, err);
    });
}

fn exec_proc(proc: ProcCtrlBlk) -> Result<(), String> {
    let ctxt = *proc.ctxt;
    let ap_virtid = AP_LIST.virtid_self();

    {
        proc.glacier.activate();
        let mut procs = PROCS.write();
        if let Some(old_proc) = procs.running.remove(&ap_virtid) {
            procs.ready.push_back(old_proc);
        }
        procs.running.insert(ap_virtid, proc);
    }

    unsafe {
        arch::proc::rstr_ctxt(&ctxt);
    }
}

pub fn exit_proc(code: i32) -> ! {
    arch::exc::set(false);
    GLACIER.read().activate();
    let ap_virtid = AP_LIST.virtid_self();

    {
        let mut procs = PROCS.write();
        if let Some(old_proc) = procs.running.remove(&ap_virtid) {
            printlnk!("proc {} exited: {}", old_proc.pid, code);
        }
    }

    unsafe { arch::move_stack(stack_top()); }
    schedule();
}

fn schedule() -> ! {
    printlnk!("scheduling...");

    loop {
        arch::halt();
    }
}
