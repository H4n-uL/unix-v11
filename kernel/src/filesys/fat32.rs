use crate::{
    device::block::BlockDevice,
    filesys::vfn::{FMeta, VirtFNode}, printlnk
};

use core::fmt::{Debug, Formatter, Result as FmtResult};
use alloc::{boxed::Box, string::{String, ToString}, sync::Arc, vec::Vec};
use spin::Mutex;
use zerocopy::{LE, U16, U32};

#[repr(C, packed)]
#[derive(Debug)]
struct BiosParameterBlock {
    jmp_boot: [u8; 3],
    oem_name: [u8; 8],
    bytes_p_sct: U16<LE>,
    sct_p_clust: u8,
    res_sct_count: U16<LE>,
    num_fats: u8,
    root_entry_count: U16<LE>,
    total_sectors_16: U16<LE>,
    media: u8,
    fat_size_16: U16<LE>,
    sectors_per_track: U16<LE>,
    num_heads: U16<LE>,
    hidden_sectors: U32<LE>,
    total_sectors_32: U32<LE>,
    fat_size_32: U32<LE>,
    ext_flags: U16<LE>,
    fs_version: U16<LE>,
    root_clust: U32<LE>,
    fs_info: U16<LE>,
    bk_boot_sector: U16<LE>,
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
    cluster_hi: U16<LE>,
    time: U16<LE>,
    date: U16<LE>,
    cluster_lo: U16<LE>,
    file_size: U32<LE>
}

impl DirEntry {
    fn from_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() >= size_of::<DirEntry>());
        unsafe { (bytes.as_ptr() as *const DirEntry).read() }
    }
}

pub struct FileAllocTable {
    i: Mutex<Fat32Inner>
}

struct Fat32Inner {
    part: Arc<dyn BlockDevice>,
    bpb: Box<BiosParameterBlock>
}

impl Debug for FileAllocTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let bpb = &self.i.lock().bpb;
        f.debug_struct("FileAllocTable")
            .field("bpb", bpb)
            .finish()
    }
}

impl FileAllocTable {
    pub fn new(part: Arc<dyn BlockDevice>) -> Option<Arc<Self>> {
        return Some(Arc::new(Self { i: Mutex::new(Fat32Inner::new(part)?) }));
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
        let res_sct_count = self.bpb.res_sct_count.get() as u32;
        let fat_size = self.bpb.fat_size_32.get();
        let num_fats = self.bpb.num_fats as u32;
        let sct_p_clust = self.bpb.sct_p_clust as u32;

        let first_data_sct = res_sct_count + (num_fats * fat_size);
        let sct = first_data_sct + (clust.saturating_sub(2) * sct_p_clust);
        return sct as u64;
    }

    fn list(&self) -> Result<Vec<String>, String> {
        let root_dir_sct = self.calc_sct(self.bpb.root_clust.get());

        let mut buf = alloc::vec![0u8; self.part.block_size() as usize];
        self.part.read_block(&mut buf, root_dir_sct as u64).map_err(|e| alloc::format!("Read error: {}", e))?;
        let mut entries = Vec::new();
        for chunk in buf.chunks(size_of::<DirEntry>()) {
            let dentry = DirEntry::from_bytes(chunk);
            if dentry.name[0] == 0 { break; }
            if dentry.name[0] == 0xE5 { continue; }
            if dentry.attr & 0x08 != 0 { continue; } // Volume
            let name = core::str::from_utf8(&dentry.name).unwrap_or("").trim();
            let ext = core::str::from_utf8(&dentry.ext).unwrap_or("").trim();
            let full_name = if ext.is_empty() { name.to_string() } else { alloc::format!("{}.{}", name, ext) };
            entries.push(full_name.clone());
            printlnk!("Entry: {}, size: {}", full_name, dentry.file_size.get());
        }

        return Ok(Vec::new());
    }
}

impl VirtFNode for FileAllocTable {
    fn meta(&self) -> FMeta {
        unimplemented!();
    }

    fn list(&self) -> Result<Vec<String>, String> {
        return self.i.lock().list();
    }
}
