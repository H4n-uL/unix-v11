pub mod ctrlblk;

use crate::{
    arch,
    filesys::{VFS, vfn::VirtFNode},
    printlnk,
    proc::ctrlblk::{ProcCtrlBlk, ProcState},
    ram::{glacier::GLACIER, stack_top}
};

use alloc::{
    collections::btree_map::BTreeMap,
    string::String
};
use spin::{Mutex, RwLock};

pub struct ProcTables(pub BTreeMap<usize, ProcCtrlBlk>);

impl ProcTables {
    const fn new() -> Self {
        return Self(BTreeMap::new());
    }

    pub fn exec(&mut self, node: &dyn VirtFNode, args: &[&str]) -> Result<usize, String> {
        let proc = ProcCtrlBlk::new(node, args)?;
        let mut pid_rr = PID_RR.lock();
        let pid = loop {
            let pid = *pid_rr;
            if !self.0.contains_key(&pid) && pid != 0 {
                break pid;
            }
            *pid_rr = pid_rr.wrapping_add(1);
        };
        self.0.insert(pid, proc);
        return Ok(pid);
    }
}

pub static PID_RR: Mutex<usize> = Mutex::new(1);
pub static PROCS: RwLock<ProcTables> = RwLock::new(ProcTables::new());

pub fn exec_aleph() {
    let path = "/mnt/block0p0/sbin/aleph";

    VFS.walk(path).and_then(|node| {
        let pid = PROCS.write().exec(&*node, &[path])?;
        return Err(exec_proc(pid));
    }).unwrap_or_else(|err| {
        printlnk!("Failed to exec {}: {:?}", path, err);
    });
}

fn exec_proc(pid: usize) -> String {
    let ctxt;

    {
        let mut procs = PROCS.write();

        let Some(proc) = procs.0.get_mut(&pid) else {
            return "No such process".into();
        };

        if proc.state != ProcState::Ready {
            return "Process not in ready state".into();
        }

        proc.state = ProcState::Running(arch::phys_id());
        proc.glacier.activate();
        ctxt = *proc.ctxt;
    }

    unsafe {
        arch::proc::rstr_ctxt(&ctxt);
    }
}

pub fn exit_proc(code: i32) -> ! {
    arch::exc::set(false);
    GLACIER.read().activate();

    {
        let mut procs = PROCS.write();
        let pid = procs.0.iter().find(|(_, proc)| {
            if let ProcState::Running(vid) = proc.state {
                return vid == arch::phys_id();
            }
            return false;
        }).map(|(pid, _)| *pid).unwrap_or(0);

        procs.0.remove(&pid);

        printlnk!("proc {} exited: {}", pid, code);
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
