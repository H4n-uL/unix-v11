#![allow(non_camel_case_types)]

use crate::{
    device::block::BlockDevice,
    filesys::{
        parts::Partition,
        vfn::{FMeta, FType, VirtFNode}
    }
};

use core::str::Utf8Error;
use alloc::{string::String, sync::Arc, vec::Vec};
use zerocopy::{LE, U16, U32};

type u16le = U16<LE>;
type u32le = U32<LE>;

#[repr(C)]
#[derive(Clone, Copy)]
struct FatDirEnt {
    name: [u8; 8],
    ext: [u8; 3],
    attr: u8,
    ntres: u8,
    crt_time_tenth: u8,
    crt_time: u16le,
    crt_date: u16le,
    lst_acc_date: u16le,
    fst_clus_hi: u16le,
    wrt_time: u16le,
    wrt_date: u16le,
    fst_clus_lo: u16le,
    file_size: u32le
}

impl FatDirEnt {
    pub fn filename(&self) -> Result<String, Utf8Error> {
        let name = core::str::from_utf8(&self.name)?.trim_end();
        let ext = core::str::from_utf8(&self.ext)?.trim_end();

        if ext.is_empty() {
            return Ok(alloc::format!("{}", name));
        } else {
            return Ok(alloc::format!("{}.{}", name, ext));
        }
    }

    fn ftype(&self) -> FType {
        if self.attr & 0x10 != 0 {
            return FType::Directory;
        } else {
            return FType::Regular;
        }
    }
}

struct FatFile {
    dirent: FatDirEnt,
    fs: Arc<FileAllocTable>,
    hostdev: u64,
    fid: u64
}

impl FatFile {
    pub fn new(fs: Arc<FileAllocTable>, dirent: FatDirEnt, fid: u64) -> Self {
        let hostdev = fs.part.devid();
        return Self { dirent, fs, hostdev, fid };
    }

    pub fn for_each_ent<T, F>(&self, mut f: F) -> Result<Option<T>, String>
    where F: FnMut(&FatDirEnt, u64) -> Option<T> {
        if self.dirent.ftype() != FType::Directory {
            return Err("This is not a directory".into());
        }

        let mut clust =
            (self.dirent.fst_clus_hi.get() as u32) << 16
            | (self.dirent.fst_clus_lo.get() as u32);

        let is_chained = clust != 0;

        loop {
            let sct = if is_chained {
                self.fs.clust2sct(clust)
            } else {
                self.fs.bpb.rsvd_sec_cnt.get() as u64
                + (self.fs.bpb.num_fats as u64 * self.fs.fat_sz() as u64)
            };

            let buf_size = if is_chained {
                self.fs.bpb.byts_per_sec.get() as usize * self.fs.bpb.sec_per_clus as usize
            } else {
                self.fs.bpb.root_ent_cnt.get() as usize * size_of::<FatDirEnt>()
            };

            let mut buf = alloc::vec![0u8; buf_size];
            self.fs.part.read_block(&mut buf, sct)
                .map_err(|e| alloc::format!("FAT32 read error: {}", e))?;

            let ent_cnt = buf.len() / size_of::<FatDirEnt>();
            let ent_ptr = buf.as_ptr() as *const FatDirEnt;

            for i in 0..ent_cnt {
                let ent = unsafe { ent_ptr.add(i).read() };

                if ent.name[0] == 0x00 {
                    return Ok(None);
                }
                if ent.name[0] == 0xe5 {
                    continue;
                }
                if ent.attr == 0x0f {
                    continue;
                }
                if ent.attr & 0x08 != 0 {
                    continue;
                }

                let fid = ((clust as u64) << 32) | i as u64;
                if let Some(res) = f(&ent, fid) {
                    return Ok(Some(res));
                }
            }

            if !is_chained {
                break;
            }

            clust = match self.fs.next_clust(clust) {
                Some(nc) => nc,
                None => break
            };
        }

        return Ok(None);
    }
}

impl VirtFNode for FatFile {
    fn meta(&self) -> FMeta {
        return FMeta {
            fid: self.fid,
            size: self.dirent.file_size.get() as u64,
            hostdev: self.hostdev,
            ftype: self.dirent.ftype(),
            perm: 0o777,
            uid: 0xffff,
            gid: 0xffff
        };
    }

    fn read(&self, buf: &mut [u8], offset: u64) -> Result<(), String> {
        if self.dirent.ftype() != FType::Regular {
            return Err("This file is not IOable".into());
        }

        let mut skip_rem = offset as usize;
        let mut bytes_rem = buf.len();

        let mut clust =
            (self.dirent.fst_clus_hi.get() as u32) << 16
            | (self.dirent.fst_clus_lo.get() as u32);

        let clust_size =
            self.fs.bpb.byts_per_sec.get() as usize
            * self.fs.bpb.sec_per_clus as usize;

        while skip_rem >= clust_size {
            skip_rem -= clust_size;
            clust = match self.fs.next_clust(clust) {
                Some(nc) => nc,
                None => return Ok(())
            };
        }

        while bytes_rem > 0 {
            let sct = self.fs.clust2sct(clust);
            let mut clust_buf = alloc::vec![0u8; clust_size];
            self.fs.part.read_block(&mut clust_buf, sct)
                .map_err(|e| alloc::format!("FAT32 read error: {}", e))?;

            let read_size = bytes_rem.min(clust_size - skip_rem);
            let read_start = buf.len() - bytes_rem;

            buf[read_start..read_start + read_size]
                .copy_from_slice(&clust_buf[skip_rem..skip_rem + read_size]);

            bytes_rem -= read_size;
            skip_rem = 0;

            clust = match self.fs.next_clust(clust) {
                Some(nc) => nc,
                None => break
            };
        }

        return Ok(());
    }
    
    fn list(&self) -> Result<Vec<String>, String> {
        let mut entries = Vec::new();
        self.for_each_ent(|ent, _fid| {
            match ent.filename() {
                Ok(name) => {
                    entries.push(name);
                }
                Err(_) => {}
            }
            return None::<()>;
        })?;

        return Ok(entries);
    }

    fn walk(&self, name: &str) -> Result<Arc<dyn VirtFNode>, String> {
        let file = self.for_each_ent(|&ent, fid| {
            match ent.filename() {
                Ok(fname) if fname.eq_ignore_ascii_case(name) => {
                    let file = FatFile::new(self.fs.clone(), ent, fid);
                    return Some(file);
                }
                _ => {}
            }
            return None;
        })?;

        if let Some(file) = file {
            return Ok(Arc::new(file) as Arc<dyn VirtFNode>);
        } else {
            return Err("File not found".into());
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BootParamBlock {
    jmpboot: [u8; 3],
    oem_name: [u8; 8],
    byts_per_sec: u16le,
    sec_per_clus: u8,
    rsvd_sec_cnt: u16le,
    // 0x10
    num_fats: u8,
    root_ent_cnt: u16le,
    tot_sec16: u16le,
    media: u8,
    fat_sz16: u16le,
    sec_per_trk: u16le,
    num_heads: u16le,
    hidd_sec: u32le,
    // 0x20
    tot_sec32: u32le
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fat12BpbExt {
    drv_num: u8,
    _0: u8,
    boot_sig: u8,
    vol_id: u32le,
    vol_lab: [u8; 11],
    fil_sys_type: [u8; 8]
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fat32BpbExt {
    fat_sz32: u32le,
    ext_flags: u16le,
    fs_ver: u16le,
    root_clus: u32le,
    fs_info: u16le,
    bk_boot_sec: u16le,
    _0: [u8; 12]
}

pub struct FileAllocTable {
    part: Arc<dyn BlockDevice>,
    bpb: BootParamBlock,
    ext32: Option<Fat32BpbExt>,
    ext12: Fat12BpbExt
}

pub enum FatType {
    Fat12, Fat16, Fat32(Fat32BpbExt)
}

impl FileAllocTable {
    pub fn new(part: Arc<dyn BlockDevice>) -> Option<Arc<Self>> {
        let mut buf = [0u8; 512];
        part.read_block(&mut buf, 0).ok()?;
        let bptr = buf.as_ptr();

        let bpb = unsafe { (bptr as *const BootParamBlock).read() };
        let is_32bit = bpb.fat_sz16.get() == 0;

        let mut offset = size_of::<BootParamBlock>();
        let ext32 = if is_32bit {
            let ext32 = unsafe { (bptr.add(offset) as *const Fat32BpbExt).read() };
            offset += size_of::<Fat32BpbExt>();
            Some(ext32)
        } else {
            None
        };

        let ext12 = unsafe { (bptr.add(offset) as *const Fat12BpbExt).read() };

        return Some(Arc::new(Self {
            part, bpb, ext32, ext12
        }));
    }

    fn fat_sz(&self) -> u32 {
        if let Some(ext32) = &self.ext32 {
            ext32.fat_sz32.get()
        } else {
            self.bpb.fat_sz16.get() as u32
        }
    }

    pub fn clust_cnt(&self) -> u32 {
        let tot_sct = self.bpb.tot_sec32.get().max(self.bpb.tot_sec16.get() as u32);

        let root_dir_sct = (
            (self.bpb.root_ent_cnt.get() * size_of::<FatDirEnt>() as u16)
            + (self.bpb.byts_per_sec.get() - 1)
        ) / self.bpb.byts_per_sec.get();

        let data_sec = tot_sct - (
            self.bpb.rsvd_sec_cnt.get() as u32
            + (self.bpb.num_fats as u32 * self.fat_sz())
            + root_dir_sct as u32
        );
        data_sec / self.bpb.sec_per_clus as u32
    }

    pub fn fat_type(&self) -> FatType {
        if let Some(ext32) = &self.ext32 {
            FatType::Fat32(*ext32)
        } else {
            match self.clust_cnt() {
                ..=4084 => FatType::Fat12,
                4085.. => FatType::Fat16
            }
        }
    }

    fn clust2sct(&self, clust: u32) -> u64 {
        let root_dir_sct = (
            (self.bpb.root_ent_cnt.get() as usize * size_of::<FatDirEnt>())
            + (self.bpb.byts_per_sec.get() - 1) as usize
        ) / self.bpb.byts_per_sec.get() as usize;

        let data_sct_base = self.bpb.rsvd_sec_cnt.get() as u64
            + (self.bpb.num_fats as u64 * self.fat_sz() as u64)
            + root_dir_sct as u64;

        let sct = data_sct_base + ((clust - 2) as u64 * self.bpb.sec_per_clus as u64);
        return sct;
    }

    fn next_clust(&self, clust: u32) -> Option<u32> {
        let fat_off = match self.fat_type() {
            FatType::Fat12 => clust as u64 + (clust as u64 >> 1),
            FatType::Fat16 => clust as u64 * size_of::<u16>() as u64,
            FatType::Fat32(_) => clust as u64 * size_of::<u32>() as u64
        };

        let fat_sct = self.bpb.rsvd_sec_cnt.get() as u64 + (fat_off / self.bpb.byts_per_sec.get() as u64);
        let ent_off = (fat_off % self.bpb.byts_per_sec.get() as u64) as usize;

        let mut buf = alloc::vec![0u8; self.part.block_size() as usize];
        self.part.read_block(&mut buf, fat_sct).ok()?;

        let entry = match self.fat_type() {
            FatType::Fat12 | FatType::Fat16 => {
                let raw = &buf[ent_off..ent_off + size_of::<u16>()];
                let raw = u16le::from_bytes(raw.try_into().unwrap()).get();

                match self.fat_type() {
                    FatType::Fat12 => {
                        (if clust & 1 == 0 {
                            raw & 0x0fff
                        } else {
                            raw >> 4
                        }) as u32
                    }
                    FatType::Fat16 => raw as u32,
                    _ => unreachable!()
                }
            }
            FatType::Fat32(_) => {
                let raw = &buf[ent_off..ent_off + size_of::<u32>()];
                let raw = u32le::from_bytes(raw.try_into().unwrap()).get();
                raw & 0x0fffffff
            }
        };

        return match self.fat_type() {
            FatType::Fat12 if entry >= 0x0ff8 => None,
            FatType::Fat16 if entry >= 0xfff8 => None,
            FatType::Fat32(_) if entry >= 0x0ffffff8 => None,
            _ => Some(entry)
        };
    }
}

impl Partition for FileAllocTable {
    fn root(self: Arc<Self>) -> Arc<dyn VirtFNode> {
        let clust = match self.fat_type() {
            FatType::Fat32(ext32) => ext32.root_clus.get(),
            _ => 0
        };

        let ent = FatDirEnt {
            name: *b"/       ",
            ext: *b"   ",
            attr: 0x10,
            ntres: 0,
            crt_time_tenth: 0,
            crt_time: u16le::new(0),
            crt_date: u16le::new(0),
            lst_acc_date: u16le::new(0),
            fst_clus_hi: u16le::new((clust >> 16) as u16),
            wrt_time: u16le::new(0),
            wrt_date: u16le::new(0),
            fst_clus_lo: u16le::new((clust & 0xffff) as u16),
            file_size: u32le::new(0)
        };

        return Arc::new(FatFile::new(self, ent, 0)) as Arc<dyn VirtFNode>;
    }
}
