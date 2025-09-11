mod dev; mod vfn;

use crate::{
    device::block::BLOCK_DEVICES,
    filesys::{vfn::{FMeta, FType, VirtFNode}, dev::DevFile},
    printlnk,
    ram::dump_bytes
};
use alloc::{collections::btree_map::BTreeMap, format, string::String, sync::Arc, vec::Vec};
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
                meta: FMeta::default(FType::Regular),
                data: Vec::new()
            })
        };
    }
}

impl VirtFNode for VirtFile {
    fn meta(&self) -> FMeta {
        return self.vfd.lock().meta.clone();
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> bool {
        let data = &self.vfd.lock().data;
        let offset = offset as usize;
        if offset >= data.len() { return false; }
        let read_len = buf.len().min(data.len() - offset);
        buf[..read_len].clone_from_slice(&data[offset..offset + read_len]);
        return true;
    }

    fn write(&self, buf: &[u8], offset: u64) -> bool {
        let mut vfd = self.vfd.lock();
        let offset = offset as usize;
        let write_end = buf.len() + offset;
        let new_size = write_end.max(vfd.data.len());
        vfd.data.resize(new_size, 0);
        vfd.data[offset..write_end].clone_from_slice(buf);
        vfd.meta.size = new_size as u64;
        return true;
    }

    fn truncate(&self, size: u64) -> bool {
        let mut vfd = self.vfd.lock();
        vfd.data.resize(size as usize, 0);
        vfd.meta.size = size;
        return true;
    }

    fn list(&self) -> Option<Vec<String>> {
        return None;
    }

    fn walk(&self, _: &str) -> Option<Arc<dyn VirtFNode>> {
        return None;
    }

    fn create(&self, _: &str, _: Arc<dyn VirtFNode>) -> bool {
        return false;
    }

    fn remove(&self, _: &str) -> bool {
        return false;
    }
}

struct VirtDirectory {
    meta: FMeta,
    files: Mutex<BTreeMap<String, Arc<dyn VirtFNode>>>
}

impl VirtDirectory {
    pub fn new() -> Self {
        return Self {
            meta: FMeta::default(vfn::FType::Directory),
            files: Mutex::new(BTreeMap::new())
        };
    }
}

impl VirtFNode for VirtDirectory {
    fn meta(&self) -> FMeta {
        return self.meta.clone();
    }

    fn read(&self, _: &mut [u8], _: u64) -> bool {
        return false;
    }

    fn write(&self, _: &[u8], _: u64) -> bool {
        return false;
    }

    fn truncate(&self, _: u64) -> bool {
        return false;
    }

    fn list(&self) -> Option<Vec<String>> {
        return Some(self.files.lock().keys().cloned().collect());
    }

    fn walk(&self, name: &str) -> Option<Arc<dyn VirtFNode>> {
        return self.files.lock().get(name).cloned();
    }

    fn create(&self, name: &str, node: Arc<dyn VirtFNode>) -> bool {
        let mut files = self.files.lock();
        if files.contains_key(name) { return false; }
        files.insert(String::from(name), node);
        return true;
    }

    fn remove(&self, name: &str) -> bool {
        return self.files.lock().remove(name).is_some();
    }
}

struct VirtualFileSystem {
    root: Arc<VirtDirectory>
}

impl VirtualFileSystem {
    pub fn new() -> Self {
        return Self { root: Arc::new(VirtDirectory::new()) };
    }

    pub fn read(&self, path: &str, buf: &mut [u8], offset: u64) -> bool {
        return self.walk(path, false).is_some_and(|file| {
            file.read(buf, offset)
        });
    }

    pub fn write(&self, path: &str, buf: &[u8], offset: u64) -> bool {
        return self.walk(path, false).is_some_and(|file| {
            file.write(buf, offset)
        });
    }

    pub fn truncate(&self, path: &str, size: u64) -> bool {
        return self.walk(path, false).is_some_and(|file| {
            file.truncate(size)
        });
    }

    pub fn list(&self, path: &str) -> Option<Vec<String>> {
        return self.walk(path, false).and_then(|node| node.list());
    }

    pub fn walk(&self, path: &str, parent: bool) -> Option<Arc<dyn VirtFNode>> {
        let Ok(mut parts) = get_path_parts(path) else { return None; };
        if parent { parts.pop(); }
        let mut current = self.root.clone() as Arc<dyn VirtFNode>;
        for part in parts {
            let Some(next) = current.walk(part) else { return None; };
            current = next;
        }
        return Some(current);
    }

    pub fn link(&self, path: &str, node: Arc<dyn VirtFNode>) -> bool {
        let Some(dir) = self.walk(path, true) else { return false; };
        let Some(filename) = get_file_name(path) else { return false; };
        return dir.create(filename, node);
    }

    pub fn unlink(&self, path: &str) -> bool {
        let Some(dir) = self.walk(path, true) else { return false; };
        let Some(filename) = get_file_name(path) else { return false; };
        return dir.remove(filename);
    }
}

fn get_path_parts(path: &str) -> Result<Vec<&str>, ()> {
    if !path.starts_with('/') { return Err(()); }
    let mut parts = Vec::new();
    for part in path.split('/').filter(|s| !s.is_empty()) {
        match part {
            "" | "." => continue,
            ".." => { if !parts.is_empty() { parts.pop(); } },
            _ => { parts.push(part); }
        }
    }
    return Ok(parts);
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
    if name.is_empty() { return None; }
    return Some(name);
}

pub fn init_filesys() {
    let dev = BLOCK_DEVICES.lock();
    let vfs = VirtualFileSystem::new();

    // mkdir /dev
    let devdir = Arc::new(VirtDirectory::new()) as Arc<dyn VirtFNode>;
    devdir.create("dev0", Arc::new(DevFile::new(dev.first().unwrap().clone())));
    vfs.link("/dev", devdir);

    // echo buf > /main.rs
    let mut buf = "fn main() {\n    println!(\"Hello, world!\");\n}".as_bytes().to_vec();
    let file = Arc::new(VirtFile::new()) as Arc<dyn VirtFNode>;
    // // pre-write
    // file.write(&buf, 0);
    // vfs.link("/main.rs", file);
    // or post-write
    vfs.link("/main.rs", file);
    vfs.write("/main.rs", &buf, 0);

    // xd /main.rs
    buf.iter_mut().for_each(|b| *b = 0);
    // // walk in to read
    // let Some(file) = vfs.walk("/main.rs", false) else { return; };
    // file.read(&mut buf, 0);
    // or direct read from vfs
    if !vfs.read("/main.rs", &mut buf, 0) { return; }
    dump_bytes(&buf);

    // mv
    vfs.walk("/main.rs", false).and_then(|file| {
        vfs.link("/src", Arc::new(VirtDirectory::new()));
        vfs.link("/src/main.rs", file);
        vfs.unlink("/main.rs");
        return Some(());
    });

    // ls
    let dir = "/src";
    vfs.list(dir).iter().for_each(|entries| {
        printlnk!("in {}:", dir);
        for entry in entries {
            let path = join_path(&[dir, entry]).unwrap();
            let meta = vfs.walk(&path, false).unwrap().meta();
            printlnk!("    {:?}: {}", meta.ftype, entry);
        }
    });
}