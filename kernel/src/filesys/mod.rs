use crate::{printlnk, ram::dump_bytes};
use alloc::{collections::btree_map::BTreeMap, string::String, vec::Vec};

struct Ramfs {
    files: BTreeMap<String, Vec<u8>>
}

impl Ramfs {
    pub fn new() -> Self {
        return Self { files: BTreeMap::new() };
    }

    pub fn create(&mut self, name: &str) -> bool {
        if self.files.contains_key(name) { return false; }
        self.files.insert(String::from(name), Vec::new());
        return true;
    }

    pub fn read(&self, name: &str, buf: &mut [u8], offset: usize) -> bool {
        let Some(f) = self.files.get(name) else { return false; };
        if offset >= f.len() { return false; }
        let read_len = buf.len().min(f.len() - offset);
        buf[..read_len].clone_from_slice(&f[offset..offset + read_len]);
        return true;
    }

    pub fn write(&mut self, name: &str, buf: &[u8], offset: usize) -> bool {
        let Some(f) = self.files.get_mut(name) else { return false; };
        let write_end = buf.len() + offset;
        f.resize(write_end.max(f.len()), 0);
        f[offset..write_end].clone_from_slice(buf);
        return true;
    }

    pub fn truncate(&mut self, name: &str, size: usize) -> bool {
        let Some(f) = self.files.get_mut(name) else { return false; };
        f.resize(size, 0);
        return true;
    }

    pub fn delete(&mut self, name: &str) -> bool {
        return self.files.remove(name).is_some();
    }
}

fn get_path_parts(path: &str) -> Result<Vec<&str>, ()> {
    if !path.starts_with('/') {
        return Err(());
    }
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

pub fn init_filesys() {
    let mut filesys = Ramfs::new();
    let mut buf = "Hello, world!".as_bytes().to_vec();
    filesys.create("log.txt");
    filesys.write("log.txt", &buf, 0);
    filesys.truncate("log.txt", 5);
    buf.iter_mut().for_each(|x| *x = 0);
    filesys.write("log.txt", b"!", 5);
    filesys.read("log.txt", &mut buf, 0);
    dump_bytes(&buf);
    filesys.delete("log.txt");
    printlnk!("/{}", get_path_parts("/a/../../b/c/./d//e/../").unwrap().join("/"));
}