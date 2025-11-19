use crate::{
    device::block::BlockDevice,
    filesys::{parts::Partition, vfn::{FMeta, FType, VirtFNode}}
};

use core::fmt::{Debug, Formatter, Result as FmtResult};
use alloc::{
    boxed::Box, string::{String, ToString}, sync::Arc, vec::Vec
};
use spin::Mutex;
use zerocopy::{LE, U16, U32};

macro_rules! getname {
    ($x:expr) => { str::from_utf8($x).unwrap_or("").trim() };
}

#[repr(C, packed)]
#[derive(Clone, Debug)]
struct BiosParameterBlock {
    jmp_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_p_sct: U16<LE>,
    sct_p_clust: u8,
    res_sct_cnt: U16<LE>,
    num_fats: u8,
    root_entry_count: U16<LE>,
    total_scts_16: U16<LE>,
    media: u8,
    fat_size_16: U16<LE>,
    scts_per_trk: U16<LE>,
    num_heads: U16<LE>,
    hidden_scts: U32<LE>,
    total_scts_32: U32<LE>,
    fat_size_32: U32<LE>,
    ext_flags: U16<LE>,
    fs_version: U16<LE>,
    root_clust: U32<LE>,
    fs_info: U16<LE>,
    bk_boot_sct: U16<LE>,
    reserved: [u8; 12],
    drive_number: u8,
    reserved1: u8,
    boot_signature: u8,
    volume_id: U32<LE>,
    volume_label: [u8; 11],
    fs_type: [u8; 8]
}

impl BiosParameterBlock {
    fn from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= size_of::<BiosParameterBlock>());
        unsafe { (bytes.as_ptr() as *const BiosParameterBlock).read() }
    }
}

#[repr(C, packed)]
#[derive(Debug)]
struct DirEntry {
    name: [u8; 8],
    ext: [u8; 3],
    attr: u8,
    _r0: u8,
    ctime_10th: u8,
    ctime: U16<LE>,
    cdate: U16<LE>,
    adate: U16<LE>,
    clust_hi: U16<LE>,
    time: U16<LE>,
    date: U16<LE>,
    clust_lo: U16<LE>,
    file_size: U32<LE>
}

impl DirEntry {
    fn from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= size_of::<DirEntry>());
        unsafe { (bytes.as_ptr() as *const DirEntry).read() }
    }

    fn get_clust(&self) -> u32 {
        let hi = self.clust_hi.get() as u32;
        let lo = self.clust_lo.get() as u32;
        return (hi << 16) | lo;
    }

    fn is_directory(&self) -> bool {
        return self.attr & 0x10 != 0;
    }
}

struct Fat32File {
    part: Arc<FileAllocTable32>,
    dentry: Option<Box<DirEntry>> // None if root
}

impl Fat32File {
    pub fn new(part: Arc<FileAllocTable32>, dentry: DirEntry) -> Self {
        return Self { part, dentry: Some(Box::new(dentry)) };
    }

    pub fn new_root(part: Arc<FileAllocTable32>) -> Self {
        return Self { part, dentry: None };
    }

    fn get_clust(&self) -> u32 {
        match &self.dentry {
            Some(d) => d.get_clust(),
            None => self.part.i.lock().bpb.root_clust.get()
        }
    }

    fn get_size(&self) -> u64 {
        match &self.dentry {
            Some(d) => d.file_size.get() as u64,
            None => 0
        }
    }

    fn is_directory(&self) -> bool {
        match &self.dentry {
            Some(d) => d.is_directory(),
            None => true
        }
    }
}

impl VirtFNode for Fat32File {
    fn read(&self, buf: &mut [u8], off: u64) -> Result<(), String> {
        if self.is_directory() {
            return Err("Cannot read directory".into());
        }

        let file_size = self.get_size();
        if off >= file_size {
            return Ok(());
        }

        let clust_sz = self.part.i.lock().clust_sz() as u64;
        let start_clust = self.get_clust();
        let to_read = core::cmp::min(buf.len() as u64, file_size - off) as usize;
        let chain = self.get_clust_chain(start_clust)?;

        let mut bytes_read = 0;
        let mut off_now = off;

        for &clust in &chain {
            if bytes_read >= to_read {
                break;
            }

            let clust_off = off_now % clust_sz;
            if off_now >= clust_sz {
                off_now -= clust_sz;
                continue;
            }

            let mut clust_buf = alloc::vec![0u8; clust_sz as usize];
            self.read_clust(clust, &mut clust_buf)?;

            let available = clust_sz - clust_off;
            let copy_len = core::cmp::min(available as usize, to_read - bytes_read);

            buf[bytes_read..bytes_read + copy_len]
                .copy_from_slice(&clust_buf[clust_off as usize..clust_off as usize + copy_len]);

            bytes_read += copy_len;
            off_now = 0;
        }

        return Ok(());
    }

    fn write(&self, _buf: &[u8], _off: u64) -> Result<(), String> {
        return Err("Write not implemented".into());
    }

    fn truncate(&self, _size: u64) -> Result<(), String> {
        return Err("Truncate not implemented".into());
    }

    fn meta(&self) -> FMeta {
        use crate::filesys::vfn::{FMeta, FType, vfid};

        let ftype = if self.is_directory() { FType::Directory } else { FType::Regular };
        let mut meta = FMeta::default(vfid(), 0, ftype);
        meta.size = self.get_size();
        return meta;
    }

    fn create(&self, _name: &str, _ftype: FType) -> Result<(), String> {
        return Err("Create not implemented".into());
    }

    fn link(&self, _name: &str, _node: Arc<dyn VirtFNode>) -> Result<(), String> {
        return Err("Link not implemented".into());
    }

    fn list(&self) -> Result<Vec<String>, String> {
        if !self.is_directory() {
            return Err("Not a directory".into());
        }

        let start_clust = self.get_clust();
        let chain = self.get_clust_chain(start_clust)?;
        let clust_sz = self.part.i.lock().clust_sz();

        let mut entries = Vec::new();
        let mut clust_buf = alloc::vec![0u8; clust_sz];

        for &clust in &chain {
            self.read_clust(clust, &mut clust_buf)?;

            for chunk in clust_buf.chunks(size_of::<DirEntry>()) {
                let dentry = DirEntry::from_bytes(chunk);

                if dentry.name[0] == 0 {
                    return Ok(entries);
                }
                if dentry.name[0] == 0xE5 {
                    continue;
                }
                if dentry.attr & 0x0F == 0x0F {
                    continue; // LFN
                }
                if dentry.attr & 0x08 != 0 {
                    continue; // Volume label
                }

                let name = getname!(&dentry.name);
                let ext = getname!(&dentry.ext);

                if name == "." || name == ".." {
                    continue;
                }

                let full_name = if ext.is_empty() {
                    name.to_string()
                } else {
                    alloc::format!("{}.{}", name, ext)
                };

                entries.push(full_name);
            }
        }

        return Ok(entries);
    }

    fn remove(&self, _name: &str) -> Result<(), String> {
        return Err("Remove not implemented".into());
    }

    fn walk(&self, name: &str) -> Result<Arc<dyn VirtFNode>, String> {
        if !self.is_directory() {
            return Err("Not a directory".into());
        }

        let start_clust = self.get_clust();
        let chain = self.get_clust_chain(start_clust)?;
        let clust_sz = self.part.i.lock().clust_sz();

        let mut clust_buf = alloc::vec![0u8; clust_sz];
        let (s_name, s_ext) = name.split_once(".")
            .map(|(n, e)| (n.trim(), e.trim()))
            .unwrap_or((name.trim(), ""));

        for &clust in &chain {
            self.read_clust(clust, &mut clust_buf)?;

            for chunk in clust_buf.chunks(size_of::<DirEntry>()) {
                let dentry = DirEntry::from_bytes(chunk);

                if dentry.name[0] == 0 {
                    return Err(alloc::format!("File not found: {}", name));
                }
                if dentry.name[0] == 0xE5 {
                    continue;
                }
                if dentry.attr & 0x0F == 0x0F {
                    continue; // LFN
                }
                if dentry.attr & 0x08 != 0 {
                    continue; // Volume label
                }

                let e_name = getname!(&dentry.name);
                let e_ext = getname!(&dentry.ext);

                if e_name.eq_ignore_ascii_case(s_name) && e_ext.eq_ignore_ascii_case(s_ext) {
                    return Ok(Arc::new(Fat32File::new(self.part.clone(), dentry)));
                }
            }
        }

        return Err(alloc::format!("File not found: {}", name));
    }
}

impl Fat32File {
    fn read_fat_entry(&self, clust: u32) -> Result<u32, String> {
        let inner = self.part.i.lock();
        let fat_off = clust * size_of::<u32>() as u32;
        let fat_sct = inner.bpb.res_sct_cnt.get() as u32 + (fat_off / inner.part.block_size() as u32);
        let ent_off = (fat_off % inner.part.block_size() as u32) as usize;

        let mut buf = alloc::vec![0u8; inner.part.block_size() as usize];
        inner.part
            .read_block(&mut buf, fat_sct as u64)
            .map_err(|e| alloc::format!("FAT read error: {}", e))?;

        let entry = u32::from_le_bytes([
            buf[ent_off],
            buf[ent_off + 1],
            buf[ent_off + 2],
            buf[ent_off + 3]
        ]) & 0x0fffffff;

        return Ok(entry);
    }

    fn get_clust_chain(&self, start_clust: u32) -> Result<Vec<u32>, String> {
        let mut chain = Vec::new();
        let mut current = start_clust;

        while 2 <= current && current < 0x0ffffff8 {
            chain.push(current);
            current = self.read_fat_entry(current)?;
        }

        return Ok(chain);
    }

    fn read_clust(&self, clust: u32, buf: &mut [u8]) -> Result<(), String> {
        let inner = self.part.i.lock();
        let sct = inner.calc_sct(clust);
        let sct_p_clust = inner.bpb.sct_p_clust as u64;
        let bps = inner.part.block_size() as usize;
        let clust_sz = inner.clust_sz();

        if buf.len() < clust_sz {
            return Err("Buffer too small".into());
        }

        for i in 0..sct_p_clust {
            inner.part
                .read_block(&mut buf[(i as usize * bps)..], sct + i)
                .map_err(|e| alloc::format!("Clust read error: {}", e))?;
        }

        return Ok(());
    }
}

pub struct FileAllocTable32 {
    i: Mutex<Fat32Inner>
}

struct Fat32Inner {
    part: Arc<dyn BlockDevice>,
    bpb: Box<BiosParameterBlock>
}

impl Debug for FileAllocTable32 {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let bpb = &self.i.lock().bpb;
        f.debug_struct("FileAllocTable32")
            .field("bpb", bpb)
            .finish()
    }
}

impl FileAllocTable32 {
    pub fn new(part: Arc<dyn BlockDevice>) -> Option<Arc<Self>> {
        return Some(Arc::new(Self {
            i: Mutex::new(Fat32Inner::new(part)?)
        }));
    }
}

impl Fat32Inner {
    pub fn new(part: Arc<dyn BlockDevice>) -> Option<Self> {
        let mut bytes = alloc::vec![0u8; part.block_size() as usize];
        part.read_block(&mut bytes, 0).ok()?;
        let bpb = Box::new(BiosParameterBlock::from_bytes(&bytes));
        return Some(Self { part, bpb });
    }

    fn calc_sct(&self, clust: u32) -> u64 {
        let res_sct_cnt = self.bpb.res_sct_cnt.get() as u32;
        let fat_size = self.bpb.fat_size_32.get();
        let num_fats = self.bpb.num_fats as u32;
        let sct_p_clust = self.bpb.sct_p_clust as u32;

        let first_data_sct = res_sct_cnt + (num_fats * fat_size);
        let sct = first_data_sct + (clust.saturating_sub(2) * sct_p_clust);
        return sct as u64;
    }

    fn clust_sz(&self) -> usize {
        return self.bpb.sct_p_clust as usize * self.part.block_size() as usize;
    }
}

impl Partition for FileAllocTable32 {
    fn root(self: Arc<Self>) -> Arc<dyn VirtFNode> {
        return Arc::new(Fat32File::new_root(self));
    }
}
