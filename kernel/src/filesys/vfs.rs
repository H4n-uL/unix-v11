use crate::filesys::{FsError, Result};
use alloc::{collections::{BTreeMap, BTreeSet}, string::{String, ToString}, sync::Arc, vec::Vec};
use spin::{Mutex, RwLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeType {
    File,
    Directory,
    CharDevice,
    BlockDevice,
    Fifo,
    Socket,
    Symlink
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub size: usize,
    pub node_type: NodeType,
    pub permissions: u16,
    pub uid: u32,
    pub gid: u32,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64
}

impl Default for Metadata {
    fn default() -> Self {
        return Self {
            size: 0,
            node_type: NodeType::File,
            permissions: 0o644,
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0
        };
    }
}

pub trait VNode: Send + Sync {
    fn metadata(&self) -> Result<Metadata>;
    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn readdir(&self) -> Result<Vec<DirEntry>>;
    fn lookup(&self, name: &str) -> Result<Arc<dyn VNode>>;
    fn create(&self, name: &str, node_type: NodeType) -> Result<Arc<dyn VNode>>;
    fn unlink(&self, name: &str) -> Result<()>;
    fn truncate(&self, size: usize) -> Result<()>;
    fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize>;
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub node_type: NodeType
}

pub trait FileSystem: Send + Sync {
    fn root(&self) -> Arc<dyn VNode>;
    fn sync(&self) -> Result<()>;
}

struct VfsDir {
    name: String,
    children: RwLock<BTreeMap<String, Arc<dyn VNode>>>,
    metadata: Metadata
}

impl VfsDir {
    fn new(name: String) -> Arc<Self> {
        let mut metadata = Metadata::default();
        metadata.node_type = NodeType::Directory;
        metadata.permissions = 0o755;

        return Arc::new(Self {
            name,
            children: RwLock::new(BTreeMap::new()),
            metadata
        });
    }

    fn add_child(&self, name: String, node: Arc<dyn VNode>) -> Result<()> {
        let mut children = self.children.write();
        if children.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }
        children.insert(name, node);
        return Ok(());
    }

    fn remove_child(&self, name: &str) -> Result<()> {
        let mut children = self.children.write();
        children.remove(name).ok_or(FsError::NotFound)?;
        return Ok(());
    }
}

impl VNode for VfsDir {
    fn metadata(&self) -> Result<Metadata> {
        let mut metadata = self.metadata.clone();
        metadata.size = self.children.read().len();
        return Ok(metadata);
    }

    fn read(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> {
        return Err(FsError::NotFile);
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize> {
        return Err(FsError::NotFile);
    }

    fn readdir(&self) -> Result<Vec<DirEntry>> {
        let children = self.children.read();
        let mut entries = Vec::new();

        entries.push(DirEntry {
            name: ".".to_string(),
            node_type: NodeType::Directory
        });
        entries.push(DirEntry {
            name: "..".to_string(),
            node_type: NodeType::Directory
        });

        for (name, node) in children.iter() {
            let metadata = node.metadata()?;
            entries.push(DirEntry {
                name: name.clone(),
                node_type: metadata.node_type
            });
        }

        return Ok(entries);
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn VNode>> {
        if name == "." || name == ".." {
            return Err(FsError::NotSupported);
        }

        let children = self.children.read();
        return children.get(name).cloned().ok_or(FsError::NotFound);
    }

    fn create(&self, name: &str, node_type: NodeType) -> Result<Arc<dyn VNode>> {
        match node_type {
            NodeType::Directory => {
                let new_dir = VfsDir::new(name.to_string());
                self.add_child(name.to_string(), new_dir.clone() as Arc<dyn VNode>)?;
                return Ok(new_dir as Arc<dyn VNode>);
            }
            _ => return Err(FsError::NotSupported)
        }
    }

    fn unlink(&self, name: &str) -> Result<()> {
        return self.remove_child(name);
    }

    fn truncate(&self, _size: usize) -> Result<()> {
        return Err(FsError::NotDirectory);
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Result<usize> {
        return Err(FsError::NotSupported);
    }
}

struct MountPoint {
    path: String,
    fsys: Vec<Arc<dyn FileSystem>>
}

pub struct VirtualFileSystem {
    root: Option<Arc<VfsDir>>,
    mounts: RwLock<BTreeMap<String, MountPoint>>
}

impl VirtualFileSystem {
    const fn empty() -> Self {
        return Self {
            root: None,
            mounts: RwLock::new(BTreeMap::new())
        };
    }

    pub fn init(&mut self) {
        if self.root.is_some() { return; }
        self.root = Some(VfsDir::new("/".to_string()));
    }

    pub fn mount(&self, path: &str, fs: Arc<dyn FileSystem>) -> Result<()> {
        if !path.is_empty() && !path.starts_with('/') {
            return Err(FsError::InvalidPath);
        }
        if !path.is_empty() && path != "/" {
            let mount_point = self.lookup(path)?;
            if mount_point.metadata()?.node_type != NodeType::Directory {
                return Err(FsError::NotDirectory);
            }
        }
        let mut mounts = self.mounts.write();
        match mounts.get_mut(path) {
            Some(mount_point) => mount_point.fsys.push(fs),
            None => { mounts.insert(path.to_string(), MountPoint {
                path: path.to_string(),
                fsys: alloc::vec![fs]
            }); }
        }
        return Ok(());
    }

    pub fn umount(&self, path: &str) -> Result<()> {
        let mut mounts = self.mounts.write();
        match mounts.get_mut(path) {
            Some(mount_point) => {
                if mount_point.fsys.is_empty() {
                    return Err(FsError::NotFound);
                }
                mount_point.fsys.pop();
                if mount_point.fsys.is_empty() {
                    mounts.remove(path);
                }
                Ok(())
            }
            None => Err(FsError::NotFound)
        }
    }

    pub fn lookup(&self, path: &str) -> Result<Arc<dyn VNode>> {
        if self.root.is_none() {
            return Err(FsError::NotFound);
        }
        let mounts = self.mounts.read();
        if let Some(mount_point) = mounts.get(path) {
            if let Some(top_fs) = mount_point.fsys.last() {
                return Ok(top_fs.root());
            }
        }
        if path.is_empty() || path == "/" {
            return Ok(self.root.clone().unwrap() as Arc<dyn VNode>);
        }
        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if let Some(root_mount) = mounts.get("/") {
            for fs in root_mount.fsys.iter().rev() {
                if let Ok(node) = self.traverse_path(&fs.root(), &components, &mounts) {
                    return Ok(node);
                }
            }
        }
        return self.traverse_path(&(self.root.clone().unwrap() as Arc<dyn VNode>), &components, &mounts);
    }

    fn traverse_path(&self, start: &Arc<dyn VNode>, components: &[&str], mounts: &BTreeMap<String, MountPoint>) -> Result<Arc<dyn VNode>> {
        let mut current = start.clone();
        let mut current_path = String::new();
        for component in components {
            current_path.push('/');
            current_path.push_str(component);
            if let Some(mount) = mounts.get(&current_path) {
                if let Some(top_fs) = mount.fsys.last() {
                    current = top_fs.root();
                    continue;
                }
            }
            current = current.lookup(component)?;
        }
        return Ok(current);
    }

    pub fn open(&self, path: &str) -> Result<File> {
        let vnode = self.lookup(path)?;
        return Ok(File::new(vnode));
    }

    pub fn create(&self, path: &str, node_type: NodeType) -> Result<Arc<dyn VNode>> {
        let (parent_path, name) = split_path(path)?;
        let parent = self.lookup(parent_path)?;
        return parent.create(name, node_type);
    }

    pub fn unlink(&self, path: &str) -> Result<()> {
        let (parent_path, name) = split_path(path)?;
        let parent = self.lookup(parent_path)?;
        return parent.unlink(name);
    }

    pub fn readdir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let mounts = self.mounts.read();
        if let Some(mount_point) = mounts.get(path) {
            if !mount_point.fsys.is_empty() {
                let mut merged = Vec::new();
                let mut seen_names = BTreeSet::new();
                for fs in mount_point.fsys.iter().rev() {
                    let entries = fs.root().readdir()?;
                    for entry in entries {
                        if !seen_names.contains(&entry.name) {
                            seen_names.insert(entry.name.clone());
                            merged.push(entry);
                        }
                    }
                }
                if let Ok(base_vnode) = self.lookup_base(path) {
                    let base_entries = base_vnode.readdir()?;
                    for entry in base_entries {
                        if !seen_names.contains(&entry.name) {
                            merged.push(entry);
                        }
                    }
                }
                return Ok(merged);
            }
        }
        let vnode = self.lookup(path)?;
        return vnode.readdir();
    }

    fn lookup_base(&self, path: &str) -> Result<Arc<dyn VNode>> {
        if self.root.is_none() {
            return Err(FsError::NotFound);
        }
        if path.is_empty() || path == "/" {
            return Ok(self.root.clone().unwrap() as Arc<dyn VNode>);
        }
        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mounts = self.mounts.read();
        return self.traverse_path(&(self.root.clone().unwrap() as Arc<dyn VNode>), &components, &mounts);
    }
}

pub static VFS: Mutex<VirtualFileSystem> = Mutex::new(VirtualFileSystem::empty());

pub struct File {
    vnode: Arc<dyn VNode>,
    offset: Mutex<usize>
}

impl File {
    pub fn new(vnode: Arc<dyn VNode>) -> Self {
        return Self { vnode, offset: Mutex::new(0) };
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = self.offset.lock();
        let n = self.vnode.read(*offset, buf)?;
        *offset += n;
        return Ok(n);
    }

    pub fn write(&self, buf: &[u8]) -> Result<usize> {
        let mut offset = self.offset.lock();
        let n = self.vnode.write(*offset, buf)?;
        *offset += n;
        return Ok(n);
    }

    pub fn seek(&self, pos: usize) -> Result<()> {
        *self.offset.lock() = pos;
        return Ok(());
    }

    pub fn metadata(&self) -> Result<Metadata> {
        return self.vnode.metadata();
    }

    pub fn ioctl(&self, cmd: u32, arg: usize) -> Result<usize> {
        return self.vnode.ioctl(cmd, arg);
    }
}

fn split_path(path: &str) -> Result<(&str, &str)> {
    let path = path.trim_end_matches('/');

    return match path.rfind('/') {
        Some(pos) => {
            let parent = if pos == 0 { "/" } else { &path[..pos] };
            let name = &path[pos + 1..];

            if name.is_empty() {
                Err(FsError::InvalidPath)
            } else {
                Ok((parent, name))
            }
        }
        None => Err(FsError::InvalidPath)
    }
}