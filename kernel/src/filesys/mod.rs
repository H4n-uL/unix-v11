mod dev; mod parts; mod gpt; pub mod vfn;

use crate::{
    device::block::BLOCK_DEVICES,
    filesys::{
        dev::DevFile,
        gpt::UEFIPartition,
        parts::{Partition, fat::FileAllocTable, vpart::VirtPart},
        vfn::{FMeta, FType, VirtFNode}
    },
    printlnk,
    ram::dump_bytes
};

use alloc::{
    collections::btree_map::BTreeMap,
    format, string::String, sync::Arc, vec::Vec
};
use spin::{Mutex, RwLock};

struct VirtFile {
    vfd: Mutex<VFileData>
}

struct VFileData {
    meta: FMeta,
    data: Vec<u8>
}

impl VirtFile {
    pub fn new() -> Self {
        return Self {
            vfd: Mutex::new(VFileData {
                meta: FMeta::vfs_only(FType::Regular),
                data: Vec::new()
            })
        };
    }
}

impl VirtFNode for VirtFile {
    fn meta(&self) -> FMeta {
        return self.vfd.lock().meta.clone();
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), String> {
        let data = &self.vfd.lock().data;
        let offset = offset as usize;
        if offset >= data.len() {
            return Err("Offset out of bounds".into());
        }

        let read_len = buf.len().min(data.len() - offset);
        buf[..read_len].clone_from_slice(&data[offset..offset + read_len]);

        return Ok(());
    }

    fn write(&self, buf: &[u8], offset: u64) -> Result<(), String> {
        let mut vfd = self.vfd.lock();

        let offset = offset as usize;
        let write_end = buf.len() + offset;
        let new_size = write_end.max(vfd.data.len());

        vfd.data.resize(new_size, 0);
        vfd.data[offset..write_end].clone_from_slice(buf);
        vfd.meta.size = new_size as u64;
        return Ok(());
    }

    fn truncate(&self, size: u64) -> Result<(), String> {
        let mut vfd = self.vfd.lock();
        vfd.data.resize(size as usize, 0);
        vfd.meta.size = size;
        return Ok(());
    }
}

struct VirtDir {
    meta: FMeta,
    files: Mutex<BTreeMap<String, Arc<dyn VirtFNode>>>
}

impl VirtDir {
    pub fn new() -> Self {
        return Self {
            meta: FMeta::vfs_only(FType::Directory),
            files: Mutex::new(BTreeMap::new())
        };
    }
}

impl VirtFNode for VirtDir {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn list(&self) -> Result<Vec<String>, String> {
        return Ok(self.files.lock().keys().cloned().collect());
    }

    fn walk(&self, name: &str) -> Result<Arc<dyn VirtFNode>, String> {
        return self.files.lock().get(name).cloned().ok_or("No such file".into());
    }

    fn create(&self, name: &str, ftype: FType) -> Result<(), String> {
        let node: Arc<dyn VirtFNode> = match ftype {
            FType::Regular => Arc::new(VirtFile::new()),
            FType::Directory => Arc::new(VirtDir::new()),
            _ => return Err("Unsupported file type for creation".into())
        };
        return self.link(name, node);
    }

    fn link(&self, name: &str, node: Arc<dyn VirtFNode>) -> Result<(), String> {
        let mut files = self.files.lock();
        if files.contains_key(name) { return Err("File already exists".into()); }
        files.insert(String::from(name), node);
        return Ok(());
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        return self.files.lock().remove(name).map(|_| ()).ok_or("No such file".into());
    }
}

pub struct VirtualFileSystem {
    parts: RwLock<BTreeMap<String, Arc<dyn Partition>>>
}

impl VirtualFileSystem { // Constructors
    const fn empty() -> Self {
        return Self { parts: RwLock::new(BTreeMap::new()) };
    }

    pub fn init(&self) {
        self.parts.write().insert("/".into(), Arc::new(VirtPart::new()));
    }
}

impl VirtualFileSystem { // File operations
    pub fn read(&self, path: &str, buf: &mut [u8], offset: u64) -> Result<(), String> {
        return self.walk(path).and_then(|file|
            file.read(buf, offset)
        );
    }

    pub fn write(&self, path: &str, buf: &[u8], offset: u64) -> Result<(), String> {
        return self.walk(path).and_then(|file|
            file.write(buf, offset)
        );
    }

    pub fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        return self.walk(path).and_then(|file|
            file.truncate(size)
        );
    }

    pub fn list(&self, path: &str) -> Result<Vec<String>, String> {
        return self.walk(path).and_then(|node| node.list());
    }
}

impl VirtualFileSystem { // Directory operations
    fn walk_inner(&self, path: &str, isparent: bool) -> Result<Arc<dyn VirtFNode>, String> {
        let parts = self.parts.read();
        let root = parts.get("/").ok_or("VFS not initialised")?.clone().root();
        let partlen = path.split('/').count();
        let mut stack = Vec::<Arc<dyn VirtFNode>>::new();
        let mut path_now = String::new();

        for (i, part) in path.split('/').enumerate() {
            let last = stack.last().unwrap_or(&root);
            if last.meta().ftype != FType::Directory {
                return Err("Directory walk error".into());
            }

            if !["", ".", ".."].contains(&part) {
                if isparent && i >= partlen - 1 { break; }
                if !path_now.ends_with('/') { path_now.push('/') }
                path_now.push_str(part);

                if let Some(mounted) = parts.get(&path_now) {
                    stack.push(mounted.clone().root());
                } else {
                    stack.push(last.walk(part)?);
                }
            } else if part == ".." && !stack.is_empty() {
                stack.pop();
                if let Some(pos) = path_now.rfind('/') {
                    path_now.truncate(pos.max(1));
                }
            }
        }
        return Ok(stack.last().unwrap_or(&root).clone());
    }

    pub fn walk(&self, path: &str) -> Result<Arc<dyn VirtFNode>, String> {
        return self.walk_inner(path, false);
    }

    pub fn walk_parent(&self, path: &str) -> Result<Arc<dyn VirtFNode>, String> {
        return self.walk_inner(path, true);
    }

    pub fn create(&self, path: &str, ftype: FType) -> Result<(), String> {
        let dir = self.walk_parent(path)?;
        let filename = get_file_name(path).ok_or("Invalid path")?;
        return dir.create(filename, ftype);
    }

    pub fn link(&self, path: &str, node: Arc<dyn VirtFNode>) -> Result<(), String> {
        let dir = self.walk_parent(path)?;
        let filename = get_file_name(path).ok_or("Invalid path")?;
        return dir.link(filename, node);
    }

    pub fn unlink(&self, path: &str) -> Result<(), String> {
        let dir = self.walk_parent(path)?;
        let filename = get_file_name(path).ok_or("Invalid path")?;
        return dir.remove(filename);
    }
}

impl VirtualFileSystem { // Mount operations
    pub fn mount(&self, path: &str, part: Arc<dyn Partition>) -> Result<(), String> {
        let mut parts = self.parts.write();
        if parts.contains_key(path) { return Err("Mount point already exists".into()); }
        let dir = self.walk(path).map_err(|_| "Mount point does not exist")?;
        if dir.meta().ftype != FType::Directory { return Err("Mount point is not a directory".into()); }
        parts.insert(path.into(), part);
        return Ok(());
    }

    pub fn unmount(&mut self, path: &str) -> Result<(), String> {
        let mut parts = self.parts.write();
        if path == "/" { return Err("Cannot unmount root".into()); }
        parts.remove(path).map(|_| ()).ok_or("No such mount point".into())
    }
}

fn get_file_name(path: &str) -> Option<&str> {
    let name = path.split('/').last()?;
    if ["", ".", ".."].contains(&name) { return None; }
    return Some(name);
}

pub static VFS: VirtualFileSystem = VirtualFileSystem::empty();

pub fn init_filesys() -> Result<(), String> {
    VFS.init();

    // mkdir /dev
    VFS.create("/dev", FType::Directory)?;
    VFS.create("/mnt", FType::Directory)?;

    let devdir = VFS.walk("/dev")?;

    for (idx, dev) in BLOCK_DEVICES.read().iter().enumerate() {
        let devname = format!("block{}", idx);

        let block = Arc::new(DevFile::new(dev.clone()));
        devdir.link(&devname, block)?;
        let uefi_partable = UEFIPartition::new(dev.clone())?;
        for (i, part) in uefi_partable.get_parts().into_iter().enumerate() {
            let partdev = Arc::new(part);

            if let Some(fat) = FileAllocTable::new(partdev.clone()) {
                let name = format!("/mnt/{}p{}", devname, i);
                VFS.create(&name, FType::Directory)?;
                VFS.mount(&name, fat)?;
            }
            devdir.link(&format!("{}p{}", devname, i), partdev)?;
        }
    }

    // echo buf > /main.rs
    let mut buf = "fn main() {\n    println!(\"Hello, world!\");\n}".as_bytes().to_vec();
    VFS.link("/main.rs", Arc::new(VirtFile::new()))?;
    VFS.write("/main.rs", &buf, 0)?;

    // mv
    VFS.walk("/main.rs").and_then(|file| {
        VFS.link("/src", Arc::new(VirtDir::new()))?;
        VFS.link("/src/main.rs", file)?;
        VFS.unlink("/main.rs")?;
        return Ok(());
    })?;

    // xd /src/main.rs
    buf.iter_mut().for_each(|b| *b = 0);
    buf.resize(13, 0);
    // // walk in to read
    // let file = VFS.walk("/src/main.rs")?;
    // file.read(&mut buf, 0);
    // or direct read from VFS
    VFS.read("/src/main.rs", &mut buf, 26)?;
    dump_bytes(&buf);

    // ls /dev
    let dir = "/dev";
    let vdirn = VFS.walk(dir)?;
    vdirn.list().iter().for_each(|entries| {
        printlnk!("in {}:", dir);
        for entry in entries {
            let vfn = vdirn.walk(&entry).unwrap();
            let meta = vfn.meta();
            let ty = match meta.ftype {
                FType::Regular =>   "Regular:  ",
                FType::Directory => "Directory:",
                FType::Device =>    "Device:   ",
                FType::Partition => "Partition:"
            };
            printlnk!("    {}  {}", ty, entry);
            printlnk!("    File ID     {}", meta.fid);
            printlnk!("    Host Device {}", meta.hostdev);
            if let Some(vdevn) = vfn.as_blkdev() {
                printlnk!("    Device ID   {}", vdevn.devid());
            }
        }
    });

    // xd -n 64 /mnt/block0p0/unix
    match VFS.walk("/mnt/block0p0/unix") {
        Ok(fnode) => {
            printlnk!("Found /mnt/block0p0/unix");
            printlnk!("    size: {} bytes", fnode.meta().size);

            let mut buf = alloc::vec![0u8; 64];
            fnode.read(&mut buf, 0).unwrap();
            printlnk!("First 64 bytes of /mnt/block0p0/unix:");
            dump_bytes(&buf);
        },
        Err(e) => {
            printlnk!("Error finding /mnt/block0p0/unix: {}", e);
        }
    }

    return Ok(());
}
