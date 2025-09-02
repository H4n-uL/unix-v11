use crate::filesys::{
    FsError, Result, devfs::Device, vfs::{FileSystem, NodeType, VNode, DirEntry, Metadata}
};
use alloc::{sync::{Arc, Weak}, vec::Vec, vec, string::{String, ToString}, collections::BTreeMap, format};
use core::mem::size_of;
use spin::RwLock;

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Fat32BPB {
    jmp_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entry_count: u16,
    total_sectors_16: u16,
    media: u8,
    fat_size_16: u16,
    sectors_per_track: u16,
    num_heads: u16,
    hidden_sectors: u32,
    total_sectors_32: u32,
    fat_size_32: u32,
    ext_flags: u16,
    fs_version: u16,
    root_cluster: u32,
    fs_info: u16,
    backup_boot: u16,
    reserved: [u8; 12],
    drive_number: u8,
    reserved1: u8,
    boot_sig: u8,
    volume_id: u32,
    volume_label: [u8; 11],
    fs_type: [u8; 8]
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct Fat32FSInfo {
    lead_sig: u32,
    reserved1: [u8; 480],
    struc_sig: u32,
    free_count: u32,
    next_free: u32,
    reserved2: [u8; 12],
    trail_sig: u32
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct FatDirEntry {
    name: [u8; 11],
    attr: u8,
    nt_res: u8,
    crt_time_tenth: u8,
    crt_time: u16,
    crt_date: u16,
    lst_acc_date: u16,
    fst_clus_hi: u16,
    wrt_time: u16,
    wrt_date: u16,
    fst_clus_lo: u16,
    file_size: u32
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct LfnEntry {
    ord: u8,
    name1: [u16; 5],
    attr: u8,
    entry_type: u8,
    checksum: u8,
    name2: [u16; 6],
    fst_clus_lo: u16,
    name3: [u16; 2]
}

#[derive(Debug)]
struct DirectoryEntry {
    name: String,
    short_name: [u8; 11],
    attributes: u8,
    cluster: u32,
    file_size: u32,
    created: (u16, u16),
    modified: (u16, u16),
    accessed: u16
}

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;
const ATTR_LONG_NAME_MASK: u8 = ATTR_LONG_NAME | ATTR_DIRECTORY | ATTR_ARCHIVE;

const LAST_LONG_ENTRY: u8 = 0x40;
const FAT_CLUSTER_END: u32 = 0x0ffffff8;
#[allow(dead_code)]
const FAT_CLUSTER_BAD: u32 = 0x0ffffff7;
#[allow(dead_code)]
const FAT_CLUSTER_FREE: u32 = 0x00000000;

struct CachedCluster {
    cluster_num: u32,
    data: Vec<u8>
}

pub struct Fat32 {
    device: Arc<dyn Device>,
    bpb: Fat32BPB,
    fs_info: Option<Fat32FSInfo>,
    fat_start_sector: u32,
    data_start_sector: u32,
    total_clusters: u32,
    cluster_size: u32,
    fat_cache: RwLock<BTreeMap<u32, u32>>,
    cluster_cache: RwLock<BTreeMap<u32, CachedCluster>>,
    self_ref: RwLock<Option<Weak<Fat32>>>
}

struct Fat32VNode {
    fs: Weak<Fat32>,
    cluster: u32,
    node_type: NodeType,
    name: String,
    file_size: u32,
    attributes: u8
}

impl Fat32 {
    pub fn new(device: Arc<dyn Device>) -> Result<Arc<Self>> {
        let mut boot_sector = [0u8; 512];
        device.read(0, &mut boot_sector)?;

        if boot_sector[510] != 0x55 || boot_sector[511] != 0xaa {
            return Err(FsError::IoError("Invalid boot signature".into()));
        }

        let bpb = unsafe {
            core::ptr::read_unaligned(boot_sector.as_ptr() as *const Fat32BPB)
        };

        if bpb.bytes_per_sector == 0 || bpb.sectors_per_cluster == 0 {
            return Err(FsError::IoError("Invalid BPB".into()));
        }

        let fat_start_sector = bpb.reserved_sectors as u32;
        let fat_sectors = bpb.num_fats as u32 * bpb.fat_size_32;
        let data_start_sector = fat_start_sector + fat_sectors;

        let total_sectors = if bpb.total_sectors_32 != 0 {
            bpb.total_sectors_32
        } else {
            bpb.total_sectors_16 as u32
        };

        let data_sectors = total_sectors - data_start_sector;
        let total_clusters = data_sectors / bpb.sectors_per_cluster as u32;
        let cluster_size = bpb.sectors_per_cluster as u32 * bpb.bytes_per_sector as u32;

        let fs_info = if bpb.fs_info != 0 && bpb.fs_info != 0xFFFF {
            let mut fs_info_buf = [0u8; 512];
            device.read((bpb.fs_info as usize) * 512, &mut fs_info_buf).ok();

            let fs_info = unsafe {
                core::ptr::read_unaligned(fs_info_buf.as_ptr() as *const Fat32FSInfo)
            };

            if fs_info.lead_sig == 0x41615252 && fs_info.struc_sig == 0x61417272 {
                Some(fs_info)
            } else {
                None
            }
        } else {
            None
        };

        let fs = Arc::new(Fat32 {
            device,
            bpb,
            fs_info,
            fat_start_sector,
            data_start_sector,
            total_clusters,
            cluster_size,
            fat_cache: RwLock::new(BTreeMap::new()),
            cluster_cache: RwLock::new(BTreeMap::new()),
            self_ref: RwLock::new(None)
        });

        *fs.self_ref.write() = Some(Arc::downgrade(&fs));
        return Ok(fs);
    }

    fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.data_start_sector + ((cluster - 2) * self.bpb.sectors_per_cluster as u32)
    }

    fn read_fat_entry(&self, cluster: u32) -> Result<u32> {
        {
            let cache = self.fat_cache.read();
            if let Some(&value) = cache.get(&cluster) {
                return Ok(value);
            }
        }

        let fat_offset = cluster * 4;
        let fat_sector = self.fat_start_sector + (fat_offset / self.bpb.bytes_per_sector as u32);
        let entry_offset = (fat_offset % self.bpb.bytes_per_sector as u32) as usize;

        let mut sector_buf = vec![0u8; self.bpb.bytes_per_sector as usize];
        self.device.read(
            fat_sector as usize * self.bpb.bytes_per_sector as usize,
            &mut sector_buf
        )?;

        let value = u32::from_le_bytes([
            sector_buf[entry_offset],
            sector_buf[entry_offset + 1],
            sector_buf[entry_offset + 2],
            sector_buf[entry_offset + 3]
        ]) & 0x0FFFFFFF;

        {
            let mut cache = self.fat_cache.write();
            cache.insert(cluster, value);
        }

        Ok(value)
    }

    fn read_cluster(&self, cluster: u32) -> Result<Vec<u8>> {
        {
            let cache = self.cluster_cache.read();
            if let Some(cached) = cache.get(&cluster) {
                if cached.cluster_num == cluster {
                    return Ok(cached.data.clone());
                }
            }
        }

        if cluster < 2 || cluster >= self.total_clusters + 2 {
            return Err(FsError::IoError("Invalid cluster number".into()));
        }

        let sector = self.cluster_to_sector(cluster);
        let mut data = vec![0u8; self.cluster_size as usize];

        for i in 0..self.bpb.sectors_per_cluster {
            let offset = i as usize * self.bpb.bytes_per_sector as usize;
            let sector_offset = (sector + i as u32) as usize * self.bpb.bytes_per_sector as usize;
            self.device.read(
                sector_offset,
                &mut data[offset..offset + self.bpb.bytes_per_sector as usize]
            )?;
        }

        {
            let mut cache = self.cluster_cache.write();
            if cache.len() > 16 {
                if let Some(first_key) = cache.keys().next().cloned() {
                    cache.remove(&first_key);
                }
            }
            cache.insert(cluster, CachedCluster {
                cluster_num: cluster,
                data: data.clone()
            });
        }

        return Ok(data);
    }

    fn get_cluster_chain(&self, start_cluster: u32) -> Result<Vec<u32>> {
        let mut chain = Vec::new();
        let mut current = start_cluster;

        while current >= 2 && current < FAT_CLUSTER_END {
            if chain.len() > self.total_clusters as usize {
                return Err(FsError::IoError("Circular cluster chain detected".into()));
            }

            chain.push(current);
            current = self.read_fat_entry(current)?;
        }

        return Ok(chain);
    }

    fn parse_short_name(name: &[u8; 11]) -> String {
        let base_name = &name[0..8];
        let extension = &name[8..11];

        let base_str = core::str::from_utf8(base_name)
            .unwrap_or("")
            .trim_end();
        let ext_str = core::str::from_utf8(extension)
            .unwrap_or("")
            .trim_end();

        if ext_str.is_empty() {
            base_str.to_string()
        } else {
            format!("{}.{}", base_str, ext_str)
        }
    }

    fn calculate_checksum(name: &[u8; 11]) -> u8 {
        let mut sum = 0u8;
        for &b in name {
            sum = ((sum >> 1) + ((sum & 1) << 7)).wrapping_add(b);
        }
        return sum;
    }

    fn parse_lfn_entry(lfn: &LfnEntry) -> String {
        let mut chars = Vec::new();

        let name1 = lfn.name1;
        for ch in name1 {
            if ch == 0 || ch == 0xffff { break; }
            chars.push(ch);
        }
        let name2 = lfn.name2;
        for ch in name2 {
            if ch == 0 || ch == 0xffff { break; }
            chars.push(ch);
        }
        let name3 = lfn.name3;
        for ch in name3 {
            if ch == 0 || ch == 0xffff { break; }
            chars.push(ch);
        }

        return String::from_utf16_lossy(&chars);
    }

    fn read_directory(&self, dir_cluster: u32) -> Result<Vec<DirectoryEntry>> {
        let mut entries = Vec::new();
        let cluster_chain = self.get_cluster_chain(dir_cluster)?;
        let mut lfn_buffer = Vec::new();
        let mut lfn_checksum = 0u8;

        for cluster in cluster_chain {
            let data = self.read_cluster(cluster)?;
            let entries_per_cluster = self.cluster_size as usize / size_of::<FatDirEntry>();

            for i in 0..entries_per_cluster {
                let offset = i * size_of::<FatDirEntry>();
                if offset + size_of::<FatDirEntry>() > data.len() {
                    break;
                }

                let entry_bytes = &data[offset..offset + size_of::<FatDirEntry>()];

                if entry_bytes[0] == 0x00 {
                    return Ok(entries);
                }

                if entry_bytes[0] == 0xe5 {
                    lfn_buffer.clear();
                    continue;
                }

                let attr = entry_bytes[11];

                if (attr & ATTR_LONG_NAME_MASK) == ATTR_LONG_NAME {
                    let lfn = unsafe {
                        core::ptr::read_unaligned(entry_bytes.as_ptr() as *const LfnEntry)
                    };

                    if lfn.ord & LAST_LONG_ENTRY != 0 {
                        lfn_buffer.clear();
                        lfn_checksum = lfn.checksum;
                    }

                    lfn_buffer.push(Self::parse_lfn_entry(&lfn));
                } else {
                    let entry = unsafe {
                        core::ptr::read_unaligned(entry_bytes.as_ptr() as *const FatDirEntry)
                    };

                    if entry.name[0] == b'.' && (entry.name[1] == b' ' || entry.name[1] == b'.') {
                        lfn_buffer.clear();
                        continue;
                    }

                    let name = if !lfn_buffer.is_empty() &&
                        Self::calculate_checksum(&entry.name) == lfn_checksum {
                        lfn_buffer.reverse();
                        lfn_buffer.join("")
                    } else {
                        Self::parse_short_name(&entry.name)
                    };

                    entries.push(DirectoryEntry {
                        name,
                        short_name: entry.name,
                        attributes: entry.attr,
                        cluster: (entry.fst_clus_hi as u32) << 16 | entry.fst_clus_lo as u32,
                        file_size: entry.file_size,
                        created: (entry.crt_time, entry.crt_date),
                        modified: (entry.wrt_time, entry.wrt_date),
                        accessed: entry.lst_acc_date
                    });

                    lfn_buffer.clear();
                }
            }
        }

        return Ok(entries);
    }

    fn find_entry_in_directory(&self, dir_cluster: u32, name: &str) -> Result<DirectoryEntry> {
        let entries = self.read_directory(dir_cluster)?;

        for entry in entries {
            if entry.name.eq_ignore_ascii_case(name) {
                return Ok(entry);
            }
        }

        return Err(FsError::NotFound);
    }
}

impl FileSystem for Fat32 {
    fn root(&self) -> Arc<dyn VNode> {
        let weak_self = self.self_ref.read().as_ref()
            .expect("self_ref not initialized")
            .clone();

        Arc::new(Fat32VNode {
            fs: weak_self,
            cluster: self.bpb.root_cluster,
            node_type: NodeType::Directory,
            name: "/".to_string(),
            file_size: 0,
            attributes: ATTR_DIRECTORY
        })
    }

    fn sync(&self) -> Result<()> {
        Ok(())
    }
}

impl VNode for Fat32VNode {
    fn metadata(&self) -> Result<Metadata> {
        return Ok(Metadata {
            size: self.file_size as usize,
            node_type: self.node_type,
            permissions: if self.attributes & ATTR_READ_ONLY != 0 { 0o444 } else { 0o644 },
            uid: 0,
            gid: 0,
            atime: 0,
            mtime: 0,
            ctime: 0
        });
    }

    fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if self.node_type != NodeType::File {
            return Err(FsError::NotFile);
        }

        if offset >= self.file_size as usize {
            return Ok(0);
        }

        let to_read = core::cmp::min(buf.len(), self.file_size as usize - offset);

        if self.cluster == 0 || to_read == 0 {
            return Ok(0);
        }

        let fs = self.fs.upgrade().ok_or(FsError::IoError("Filesystem dropped".into()))?;
        let chain = fs.get_cluster_chain(self.cluster)?;
        let mut bytes_read = 0;
        let mut current_offset = offset;

        let first_cluster = chain[0];
        for cluster in &chain {
            let cluster_start = (*cluster - first_cluster) as usize * fs.cluster_size as usize;
            let cluster_end = cluster_start + fs.cluster_size as usize;

            if current_offset >= cluster_end {
                continue;
            }

            if current_offset < cluster_start {
                break;
            }

            let cluster_data = fs.read_cluster(*cluster)?;
            let cluster_offset = current_offset - cluster_start;
            let available = core::cmp::min(
                fs.cluster_size as usize - cluster_offset,
                to_read - bytes_read
            );

            buf[bytes_read..bytes_read + available]
                .copy_from_slice(&cluster_data[cluster_offset..cluster_offset + available]);

            bytes_read += available;
            current_offset += available;

            if bytes_read >= to_read {
                break;
            }
        }

        return Ok(bytes_read);
    }

    fn write(&self, _offset: usize, _buf: &[u8]) -> Result<usize> {
        Err(FsError::NotSupported)
    }

    fn readdir(&self) -> Result<Vec<DirEntry>> {
        if self.node_type != NodeType::Directory {
            return Err(FsError::NotDirectory);
        }

        let fs = self.fs.upgrade().ok_or(FsError::IoError("Filesystem dropped".into()))?;
        let entries = fs.read_directory(self.cluster)?;
        let mut result = Vec::new();

        for entry in entries {
            if entry.attributes & ATTR_VOLUME_ID != 0 {
                continue;
            }

            let node_type = if entry.attributes & ATTR_DIRECTORY != 0 {
                NodeType::Directory
            } else {
                NodeType::File
            };

            result.push(DirEntry {
                name: entry.name.clone(),
                node_type
            });
        }

        return Ok(result);
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn VNode>> {
        if self.node_type != NodeType::Directory {
            return Err(FsError::NotDirectory);
        }

        let fs = self.fs.upgrade().ok_or(FsError::IoError("Filesystem dropped".into()))?;
        let entry = fs.find_entry_in_directory(self.cluster, name)?;

        let node_type = if entry.attributes & ATTR_DIRECTORY != 0 {
            NodeType::Directory
        } else {
            NodeType::File
        };

        return Ok(Arc::new(Self {
            fs: Arc::downgrade(&fs),
            cluster: entry.cluster,
            node_type,
            name: entry.name.clone(),
            file_size: entry.file_size,
            attributes: entry.attributes
        }));
    }

    fn create(&self, _name: &str, _node_type: NodeType) -> Result<Arc<dyn VNode>> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn truncate(&self, _size: usize) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn ioctl(&self, _cmd: u32, _arg: usize) -> Result<usize> {
        Err(FsError::NotSupported)
    }
}