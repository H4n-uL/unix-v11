use core::fmt;
use alloc::{collections::BTreeMap, string::{String, ToString}, sync::Arc, vec::Vec};
use spin::{Mutex, RwLock};

pub type Result<T> = core::result::Result<T, FsError>;

#[derive(Debug, Clone)]
pub enum FsError {
    NotFound,
    PermissionDenied,
    NotDirectory,
    NotFile,
    AlreadyExists,
    DirectoryNotEmpty,
    InvalidPath,
    IoError(String),
    NotSupported,
    DeviceError(String)
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFound => write!(f, "File not found"),
            FsError::PermissionDenied => write!(f, "Permission denied"),
            FsError::NotDirectory => write!(f, "Not a directory"),
            FsError::NotFile => write!(f, "Not a file"),
            FsError::AlreadyExists => write!(f, "Already exists"),
            FsError::DirectoryNotEmpty => write!(f, "Directory not empty"),
            FsError::InvalidPath => write!(f, "Invalid path"),
            FsError::IoError(s) => write!(f, "I/O error: {}", s),
            FsError::NotSupported => write!(f, "Operation not supported"),
            FsError::DeviceError(s) => write!(f, "Device error: {}", s)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    File,
    Directory,
    CharDevice,
    BlockDevice,
    Fifo,
    Socket,
    Symlink
}

#[derive(Debug, Clone)]
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

#[derive(Clone)]
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
    fs: Arc<dyn FileSystem>
}

pub struct VirtualFileSystem {
    root: Option<Arc<VfsDir>>,
    mounts: RwLock<Vec<MountPoint>>
}

impl VirtualFileSystem {
    const fn empty() -> Self {
        return Self {
            root: None,
            mounts: RwLock::new(Vec::new())
        };
    }

    pub fn init(&mut self) {
        if self.root.is_some() { return; }

        let root = VfsDir::new("/".to_string());
        let dirs = ["dev", "bin", "etc", "tmp", "usr", "var"];
        for dir in dirs.iter() {
            let new_dir = VfsDir::new(dir.to_string());
            root.add_child(dir.to_string(), new_dir as Arc<dyn VNode>).unwrap();
        }

        self.root = Some(root);
    }

    pub fn mount(&self, path: &str, fs: Arc<dyn FileSystem>) -> Result<()> {
        self.lookup(path)?;
        let mut mounts = self.mounts.write();
        if mounts.iter().any(|m| m.path == path) {
            return Err(FsError::AlreadyExists);
        }

        mounts.push(MountPoint { path: path.to_string(), fs });
        return Ok(());
    }

    pub fn umount(&self, path: &str) -> Result<()> {
        let mut mounts = self.mounts.write();
        let pos = mounts.iter().position(|m| m.path == path)
            .ok_or(FsError::NotFound)?;

        mounts.remove(pos);
        return Ok(());
    }

    pub fn lookup(&self, path: &str) -> Result<Arc<dyn VNode>> {
        if self.root.is_none() {
            return Err(FsError::NotFound);
        }
        let root = self.root.clone();
        if path == "/" {
            return Ok(root.unwrap() as Arc<dyn VNode>);
        }

        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut current = root.unwrap() as Arc<dyn VNode>;
        let mut current_path = String::new();

        for component in components {
            current_path.push('/');
            current_path.push_str(component);

            if let Some(mount) = self.mounts.read().iter().find(|m| m.path == current_path) {
                current = mount.fs.root();
                continue;
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
        let vnode = self.lookup(path)?;
        return vnode.readdir();
    }
}

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

pub static VFS: Mutex<VirtualFileSystem> = Mutex::new(VirtualFileSystem::empty());

pub fn with_vfs<F, R>(f: F) -> Result<R>
where F: FnOnce(&VirtualFileSystem) -> Result<R> {
    let vfs = VFS.lock();
    return f(&vfs);
}