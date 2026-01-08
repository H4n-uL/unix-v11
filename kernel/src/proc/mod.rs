pub mod ctrlblk;

use crate::{
    filesys::VFS,
    kargs::ap_vid,
    printlnk,
    proc::ctrlblk::ProcCtrlBlk
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

    VFS.lock().walk(path).and_then(|node| {
        let proc = ProcCtrlBlk::new(&*node, &[path])?;
        exec_proc(proc)?;
        Ok(())
    }).unwrap_or_else(|err| {
        printlnk!("Failed to exec {}: {:?}", path, err);
    });
}

fn exec_proc(proc: ProcCtrlBlk) -> Result<(), String> {
    let ctxt = &raw const *proc.ctxt;

    {
        proc.glacier.activate();
        let mut procs = PROCS.write();
        if let Some(old_proc) = procs.running.remove(&ap_vid()) {
            procs.ready.push_back(old_proc);
        }
        procs.running.insert(ap_vid(), proc);
    }

    unsafe {
        crate::arch::proc::rstr_ctxt(&*ctxt);
    }
}
