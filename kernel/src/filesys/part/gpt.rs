use crate::{device::block::BlockDevice, filesys::{FsError, Result}, ram::PageAligned};
use alloc::{string::String, sync::Arc, vec::Vec};

const TABLE_SIZE_CRC32: usize = 16;

const fn gcrc32t() -> [[u32; 256]; TABLE_SIZE_CRC32] {
    let mut tables = [[0u32; 256]; TABLE_SIZE_CRC32];

    let mut i = 0; while i < 256 {
        let (mut crc, mut j) = (i as u32, 0);
        while j < 8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb88320 } else { crc >> 1 };
        j += 1; }
        tables[0][i] = crc;
    i += 1; }

    let mut i = 0; while i < 256 {
        let mut j = 1; while j < TABLE_SIZE_CRC32 {
            tables[j][i] = (tables[j-1][i] >> 8) ^ tables[0][tables[j-1][i] as u8 as usize];
        j += 1; }
    i += 1; }

    return tables;
}

const CRC32_TABLE: [[u32; 256]; TABLE_SIZE_CRC32] = gcrc32t();

fn crc32_slow(mut crc: u32, buf: &[u8]) -> u32 {
    crc = !crc;
    buf.iter().for_each(|&byte| { crc = (crc >> 8) ^ CRC32_TABLE[0][((crc as u8) ^ byte) as usize]; });
    return !crc;
}

pub fn crc32(mut crc: u32, buf: &[u8]) -> u32 {
    if TABLE_SIZE_CRC32 < 4 { return crc32_slow(crc, buf); }
    crc = !crc;

    buf.chunks(TABLE_SIZE_CRC32).for_each(|chunk| {
        if chunk.len() < TABLE_SIZE_CRC32 { crc = !crc32_slow(!crc, chunk); return; }
        let mut crc_temp = 0u32;
        for i in (0..TABLE_SIZE_CRC32).rev() {
            if i < 4 { crc_temp ^= CRC32_TABLE[TABLE_SIZE_CRC32 - i - 1][chunk[i] as usize ^ ((crc >> (i * 8)) & 0xFF) as usize]; }
            else { crc_temp ^= CRC32_TABLE[TABLE_SIZE_CRC32 - i - 1][chunk[i] as usize]; }
        }
        crc = crc_temp;
    });

    return !crc;
}

pub struct GptHeader {
    pub signature: [u8; 8],
    pub revision: u32,
    pub header_size: u32,
    pub header_crc32: u32,
    pub current_lba: u64,
    pub backup_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub disk_uuid: [u8; 16],
    pub partition_entry_lba: u64,
    pub number_of_entries: u32,
    pub entry_size: u32,
    pub entries_crc32: u32
}

impl GptHeader {
    fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 92 {
            return Err(FsError::InvalidPath);
        }

        let header = Self {
            signature: buf[0..8].try_into().unwrap(),
            revision: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            header_size: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            header_crc32: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            // reserved: buf[20..24]
            current_lba: u64::from_le_bytes(buf[24..32].try_into().unwrap()),
            backup_lba: u64::from_le_bytes(buf[32..40].try_into().unwrap()),
            first_usable_lba: u64::from_le_bytes(buf[40..48].try_into().unwrap()),
            last_usable_lba: u64::from_le_bytes(buf[48..56].try_into().unwrap()),
            disk_uuid: buf[56..72].try_into().unwrap(),
            partition_entry_lba: u64::from_le_bytes(buf[72..80].try_into().unwrap()),
            number_of_entries: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            entry_size: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            entries_crc32: u32::from_le_bytes(buf[88..92].try_into().unwrap())
        };

        return Ok(header);
    }

    fn verify_crc(&self, buf: &[u8]) -> bool {
        if buf.len() < self.header_size as usize {
            return false;
        }

        let mut header_copy = Vec::from(&buf[0..self.header_size as usize]);
        header_copy[16..20].copy_from_slice(&[0, 0, 0, 0]);

        let calculated_crc = crc32(0, &header_copy);
        return calculated_crc == self.header_crc32;
    }
}

pub struct PartitionAttributes {
    pub required: bool,
    pub no_block_io: bool,
    pub legacy_boot: bool,
    pub type_specific: u16
}

impl PartitionAttributes {
    fn from_u64(attrs: u64) -> Self {
        return Self {
            required: attrs & 1 != 0,
            no_block_io: attrs & 2 != 0,
            legacy_boot: attrs & 4 != 0,
            type_specific: ((attrs >> 48) & 0xFFFF) as u16
        };
    }
}

pub struct GptPartition {
    pub type_uuid: [u8; 16],
    pub unique_uuid: [u8; 16],
    pub start_lba: u64,
    pub end_lba: u64,
    pub block_size: usize,
    pub attributes: PartitionAttributes,
    pub name: String
}

impl GptPartition {
    fn from_bytes(buf: &[u8], entry_size: usize, block_size: usize) -> Option<Self> {
        if buf.len() < 128 || entry_size < 128 { return None; }
        let type_uuid: [u8; 16] = buf[0..16].try_into().unwrap();
        if type_uuid == [0; 16] { return None; }

        let unique_uuid: [u8; 16] = buf[16..32].try_into().unwrap();
        let start_lba = u64::from_le_bytes(buf[32..40].try_into().unwrap());
        let end_lba = u64::from_le_bytes(buf[40..48].try_into().unwrap());
        let attrs = u64::from_le_bytes(buf[48..56].try_into().unwrap());

        let mut name = String::new();
        for i in 0..36 {
            let offset = 56 + i * 2;
            if offset + 2 > entry_size {
                break;
            }
            let ch = u16::from_le_bytes([buf[offset], buf[offset + 1]]);
            if ch == 0 {
                break;
            }
            if ch < 0xd800 || ch >= 0xe000 {
                if let Some(c) = char::from_u32(ch as u32) {
                    name.push(c);
                }
            }
        }

        return Some(Self {
            type_uuid, unique_uuid,
            start_lba, end_lba, block_size,
            attributes: PartitionAttributes::from_u64(attrs),
            name
        });
    }

    pub fn size_in_sectors(&self) -> u64 {
        self.end_lba - self.start_lba + 1
    }

    pub fn size_in_bytes(&self) -> u64 {
        self.size_in_sectors() * self.block_size as u64
    }

    pub fn is_efi_system(&self) -> bool {
        const ESP_UUID: [u8; 16] = [
            0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11,
            0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b
        ];
        self.type_uuid == ESP_UUID
    }

    pub fn type_name(&self) -> &str {
        if self.is_efi_system() {
            "EFI System Partition"
        } else {
            "Unknown"
        }
    }
}

fn verify_mbr(device: &Arc<dyn BlockDevice>) -> Result<bool> {
    let mut mbr_buf = PageAligned::new(512);
    device.read(0, &mut mbr_buf)
        .map_err(|e| FsError::DeviceError(e))?;

    if mbr_buf[510] != 0x55 || mbr_buf[511] != 0xaa {
        return Ok(false);
    }

    let partition_type = mbr_buf[450];
    return Ok(partition_type == 0xee);
}

pub struct Gpt {
    pub header: GptHeader,
    pub block_size: usize,
    pub partitions: Vec<GptPartition>,
    pub has_protective_mbr: bool,
    pub backup_header_valid: bool
}

impl Gpt {
    pub fn read(device: &Arc<dyn BlockDevice>) -> Result<Self> {
        let block_size = device.block_size();
        let has_protective_mbr = match verify_mbr(device) {
            Ok(result) => result,
            Err(_) => false
        };
        if !has_protective_mbr {
            crate::printlnk!("Warning: No protective MBR found");
        }

        let mut header_buf = PageAligned::new(block_size);
        if let Err(e) = device.read(1, &mut header_buf) {
            return Err(FsError::DeviceError(e));
        }

        let header = GptHeader::from_bytes(&header_buf)?;
        if &header.signature != b"EFI PART" {
            return Err(FsError::InvalidPath);
        }

        if !header.verify_crc(&header_buf) {
            crate::printlnk!("Warning: Primary GPT header CRC mismatch");
        }

        let entries_size = (header.number_of_entries * header.entry_size) as usize;
        let entries_sectors = (entries_size + block_size - 1) / block_size;

        if entries_sectors == 0 || entries_size == 0 {
            return Err(FsError::InvalidPath);
        }

        let mut entries_buf = Vec::with_capacity(entries_sectors * block_size);
        for i in 0..entries_sectors {
            let lba = header.partition_entry_lba + i as u64;
            let mut sector_buf = PageAligned::new(block_size);
            device.read(lba, &mut sector_buf).map_err(|e| FsError::DeviceError(e))?;
            entries_buf.extend_from_slice(&sector_buf);
        }

        if crc32(0, &entries_buf[..entries_size]) != header.entries_crc32 {
            crate::printlnk!("Warning: Partition entries CRC mismatch");
        }

        let mut partitions = Vec::new();
        for i in 0..header.number_of_entries as usize {
            let offset = i * header.entry_size as usize;
            let end = offset + header.entry_size as usize;

            if end > entries_buf.len() { break; }
            if let Some(partition) = GptPartition::from_bytes(
                &entries_buf[offset..end],
                header.entry_size as usize,
                block_size
            ) { partitions.push(partition); }
        }

        let backup_header_valid = Self::verify_backup_gpt(
            device,
            &header,
            block_size
        );

        return Ok(Self {
            header,
            block_size,
            partitions,
            has_protective_mbr,
            backup_header_valid
        });
    }

    fn verify_backup_gpt(
        device: &Arc<dyn BlockDevice>,
        primary: &GptHeader,
        block_size: usize
    ) -> bool {
        let mut backup_buf = PageAligned::new(block_size);

        if device.read(primary.backup_lba, &mut backup_buf).is_err() {
            return false;
        }

        if let Ok(backup) = GptHeader::from_bytes(&backup_buf) {
            if &backup.signature != b"EFI PART" { return false; }
            if !backup.verify_crc(&backup_buf) { return false; }

            let cur_back = backup.current_lba == primary.backup_lba;
            let back_cur = backup.backup_lba == primary.current_lba;
            let uuid_match = backup.disk_uuid == primary.disk_uuid;
            return cur_back && back_cur && uuid_match;
        } else {
            return false;
        }
    }

    pub fn print_info(&self) {
        crate::printlnk!("GPT Disk UUID: {:02x?}", self.header.disk_uuid);
        crate::printlnk!("Usable LBAs: {} - {}",
            self.header.first_usable_lba,
            self.header.last_usable_lba
        );
        crate::printlnk!("Protective MBR: {}",
            if self.has_protective_mbr { "Yes" } else { "No" }
        );
        crate::printlnk!("Backup GPT: {}",
            if self.backup_header_valid { "Valid" } else { "Invalid/Not checked" }
        );
    }
}