use crate::{device::block::BlockDevice, filesys::dev::PartitionDev};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use zerocopy::{FromBytes, LE, U16, U32, U64};

pub struct UEFIPartition {
    dev: Arc<dyn BlockDevice>,
    head: UUIDPartitionTable,
    entries: Vec<UUIDPartitionEntry>
}

#[repr(C)]
#[derive(Clone, Copy, FromBytes)]
pub struct UUIDPartitionTable {
    sign: [u8; 8], // == "EFI PART"
    ver: U32<LE>,
    headsize: U32<LE>,
    crc32: U32<LE>,
    _r0: u32, // 0
    lba_here: U64<LE>,
    lba_backup: U64<LE>,
    lba_conv_first: U64<LE>,
    lba_conv_last: U64<LE>,
    disk_uuid: [u8; 16],
    partentry_lba: U64<LE>,
    partentry_num: U32<LE>,
    partentry_len: U32<LE>,
    partentry_crc: U32<LE>
    // zero pad until block size
}

#[repr(C)]
#[derive(Clone, Copy, FromBytes)]
struct UUIDPartitionEntry {
    type_uuid: [u8; 16],
    unique_uuid: [u8; 16],
    first_lba: U64<LE>,
    last_lba: U64<LE>,
    attr: U64<LE>,
    name: [U16<LE>; 36]
}

const PART_EFI: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11,
    0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b
];

impl UEFIPartition {
    pub fn new(dev: Arc<dyn BlockDevice>) -> Result<Self, String> {
        let mut buf = alloc::vec![0u8; dev.block_size() as usize];
        dev.read_block(&mut buf, 1)?;
        let head: UUIDPartitionTable = FromBytes::read_from_bytes(&buf[..size_of::<UUIDPartitionTable>()])
            .map_err(|_| "Failed to parse GPT header")?;

        if &head.sign != b"EFI PART" {
            return Err("Invalid GPT signature".into());
        }

        let ent_size = head.partentry_len.get() as usize;
        let ent_num = head.partentry_num.get() as usize;
        let mut ent_buf = alloc::vec![0u8; ent_size * ent_num];
        dev.read_block(&mut ent_buf, head.partentry_lba.get())?;
        let mut entries = Vec::with_capacity(ent_size * ent_num);

        for p in 0..ent_num {
            let start = p * ent_size;
            let end = start + ent_size;
            let entry: UUIDPartitionEntry = FromBytes::read_from_bytes(&ent_buf[start..end])
                .map_err(|_| format!("Failed to parse GPT entry {}", p))?;
            if entry.type_uuid == [0; 16] { continue; }
            if entry.unique_uuid == [0; 16] { continue; }
            entries.push(entry);
        }

        let uefipart = Self {
            dev: dev.clone(), head,
            entries
        };

        return Ok(uefipart);
    }

    pub fn get_parts(&self) -> Vec<PartitionDev> {
        let mut parts = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let start = entry.first_lba.get();
            let end = entry.last_lba.get();
            let part = PartitionDev::new(
                self.dev.clone(), i as u32,
                start, end - start + 1
            );
            parts.push(part);
        }
        return parts;
    }
}
