mod dev; mod gpt; mod vfn;

use crate::{
    device::block::BLOCK_DEVICES,
    filesys::{dev::DevFile, gpt::UEFIPartition, vfn::{FMeta, FType, VirtFNode}},
    printlnk,
    ram::dump_bytes
};
use alloc::{collections::btree_map::BTreeMap, format, string::{String, ToString}, sync::Arc, vec::Vec};
use spin::Mutex;

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
        if offset >= data.len() { return Err("Offset out of bounds".into()); }
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

    fn list(&self) -> Option<Vec<String>> {
        return None;
    }

    fn walk(&self, _: &str) -> Option<Arc<dyn VirtFNode>> {
        return None;
    }

    fn create(&self, _: &str, _: Arc<dyn VirtFNode>) -> Result<(), String> {
        return Err("This is not a directory.".into());
    }

    fn remove(&self, _: &str) -> Result<(), String> {
        return Err("This is not a directory.".into());
    }
}

struct VirtDirectory {
    meta: FMeta,
    files: Mutex<BTreeMap<String, Arc<dyn VirtFNode>>>
}

impl VirtDirectory {
    pub fn new() -> Self {
        return Self {
            meta: FMeta::vfs_only(FType::Directory),
            files: Mutex::new(BTreeMap::new())
        };
    }
}

impl VirtFNode for VirtDirectory {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn read(&self, _: &mut [u8], _: u64) -> Result<(), String> {
        return Err("This is not a file.".into());
    }

    fn write(&self, _: &[u8], _: u64) -> Result<(), String> {
        return Err("This is not a file.".into());
    }

    fn truncate(&self, _: u64) -> Result<(), String> {
        return Err("This is not a file.".into());
    }

    fn list(&self) -> Option<Vec<String>> {
        return Some(self.files.lock().keys().cloned().collect());
    }

    fn walk(&self, name: &str) -> Option<Arc<dyn VirtFNode>> {
        return self.files.lock().get(name).cloned();
    }

    fn create(&self, name: &str, node: Arc<dyn VirtFNode>) -> Result<(), String> {
        let mut files = self.files.lock();
        if files.contains_key(name) { return Err("File already exists.".into()); }
        files.insert(String::from(name), node);
        return Ok(());
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        return self.files.lock().remove(name).map(|_| ()).ok_or("No such file.".into());
    }
}

struct VirtualFileSystem {
    root: Arc<VirtDirectory>
}

impl VirtualFileSystem {
    pub fn new() -> Self {
        return Self { root: Arc::new(VirtDirectory::new()) };
    }

    pub fn read(&self, path: &str, buf: &mut [u8], offset: u64) -> Result<(), String> {
        return self.walk(path, false).ok_or("File not found.".into()).and_then(|file|
            file.read(buf, offset)
        );
    }

    pub fn write(&self, path: &str, buf: &[u8], offset: u64) -> Result<(), String> {
        return self.walk(path, false).ok_or("File not found.".into()).and_then(|file|
            file.write(buf, offset)
        );
    }

    pub fn truncate(&self, path: &str, size: u64) -> Result<(), String> {
        return self.walk(path, false).ok_or("File not found.".into()).and_then(|file|
            file.truncate(size)
        );
    }

    pub fn list(&self, path: &str) -> Option<Vec<String>> {
        return self.walk(path, false).and_then(|node| node.list());
    }

    pub fn walk(&self, path: &str, parent: bool) -> Option<Arc<dyn VirtFNode>> {
        let root = self.root.clone() as Arc<dyn VirtFNode>;
        let partlen = path.split('/').count();
        let mut stack = Vec::<Arc<dyn VirtFNode>>::new();
        for (i, part) in path.split('/').enumerate() {
            let last = stack.last().unwrap_or(&root);
            if last.meta().ftype != FType::Directory { return None; }
            if !["", ".", ".."].contains(&part) {
                if parent && i >= partlen - 1 { break; }
                stack.push(last.walk(part)?);
            } else if part == ".." {
                if !stack.is_empty() { stack.pop(); }
            }
        }
        return Some(stack.last().unwrap_or(&root).clone());
    }

    pub fn link(&self, path: &str, node: Arc<dyn VirtFNode>) -> Result<(), String> {
        let dir = self.walk(path, true).ok_or("No such directory.")?;
        let filename = get_file_name(path).ok_or("No such file.")?;
        return dir.create(filename, node);
    }

    pub fn unlink(&self, path: &str) -> Result<(), String> {
        let dir = self.walk(path, true).ok_or("No such directory.")?;
        let filename = get_file_name(path).ok_or("No such file.")?;
        return dir.remove(filename);
    }
}

fn join_path(paths: &[&str]) -> Result<String, ()> {
    if paths.is_empty() { return Err(()); }
    let mut parts = Vec::new();
    for &part in paths {
        for p in part.split('/').filter(|s| !s.is_empty()) {
            match p {
                "" | "." => continue,
                ".." => { if !parts.is_empty() { parts.pop(); } },
                _ => { parts.push(p); }
            }
        }
    }
    return Ok(format!("/{}", parts.join("/")));
}

fn get_file_name(path: &str) -> Option<&str> {
    let name = path.split('/').last()?;
    if ["", ".", ".."].contains(&name) { return None; }
    return Some(name);
}

pub fn init_filesys() -> Result<(), String> {
    let dev = BLOCK_DEVICES.lock().first().unwrap().clone();
    let vfs = VirtualFileSystem::new();

    // mkdir /dev
    let devdir = Arc::new(VirtDirectory::new());
    let block = Arc::new(DevFile::new(dev.clone()));
    devdir.create("block0", block)?;
    for (i, part) in UEFIPartition::new(dev.clone())?.get_parts().into_iter().enumerate() {
        let partdev = Arc::new(part);
        devdir.create(&format!("block0p{}", i), partdev)?;
    }
    vfs.link("/dev", devdir)?;

    // echo buf > /main.rs
    let mut buf = "fn main() {\n    println!(\"Hello, world!\");\n}".as_bytes().to_vec();
    let file = Arc::new(VirtFile::new());
    // // pre-write
    // file.write(&buf, 0);
    // vfs.link("/main.rs", file);
    // or post-write
    vfs.link("/main.rs", file)?;
    vfs.write("/main.rs", &buf, 0)?;

    // mv
    vfs.walk("/main.rs", false).ok_or("No such file".to_string()).and_then(|file| {
        vfs.link("/src", Arc::new(VirtDirectory::new()))?;
        vfs.link("/src/main.rs", file)?;
        vfs.unlink("/main.rs")?;
        return Ok(());
    })?;

    // xd /src/main.rs
    buf.iter_mut().for_each(|b| *b = 0);
    buf.resize(13, 0);
    // // walk in to read
    // let Some(file) = vfs.walk("/src/main.rs") else { return; };
    // file.read(&mut buf, 0);
    // or direct read from vfs
    vfs.read("/src/main.rs", &mut buf, 26)?;
    dump_bytes(&buf);

    // ls
    let dir = "/dev";
    let vdn = vfs.walk(dir, false).ok_or("No such directory")?;
    vdn.list().iter().for_each(|entries| {  
        printlnk!("in {}:", dir);
        for entry in entries {
            let vfn = vdn.walk(&entry).unwrap();
            printlnk!("    {:?}: {}", vfn.meta().ftype, entry);
            printlnk!("    fid   {}", vfn.meta().fid);
            printlnk!("    devid {}", vfn.meta().devid);
        }
    });
    return Ok(());
}