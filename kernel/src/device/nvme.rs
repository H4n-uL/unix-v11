use crate::{
    arch::rvm::flags,
    device::{block::{BlockDevType, BlockDevice, DevId, BLOCK_DEVICES}, PCI_DEVICES},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}, PhysPageBuf, PAGE_4KIB}
};

use alloc::{collections::btree_map::BTreeMap, format, string::String, sync::Arc};
use nvme_oxide::{Dma, NVMeDev, Ns};
use spin::Mutex;

pub struct NVMeAlloc;

impl Dma for NVMeAlloc {
    unsafe fn alloc(&self, size: usize) -> usize {
        return PHYS_ALLOC.alloc(AllocParams::new(size)).unwrap().addr();
    }

    unsafe fn free(&self, addr: usize, size: usize) {
        unsafe { PHYS_ALLOC.free_raw(addr as *mut u8, size); }
    }

    fn virt_to_phys(&self, va: usize) -> usize { va }
}

pub struct BlockDeviceNVMe {
    ns: Arc<Ns<NVMeAlloc>>,
    devid: u16
}

impl BlockDeviceNVMe {
    pub fn new(ns: Arc<Ns<NVMeAlloc>>, devid: u16) -> Self {
        Self { ns, devid }
    }
}

impl BlockDevice for BlockDeviceNVMe {
    fn block_size(&self) -> u64 {
        return self.ns.blk_sz() as u64;
    }

    fn block_count(&self) -> u64 {
        return self.ns.blk_cnt();
    }

    fn read_block(&self, buf: &mut [u8], lba: u64) -> Result<(), String> {
        // PhysPageBuf ensures both address and size alignment to 4 kiB
        // via AllocParams settings.
        let bs = self.block_size() as usize;
        let mut pabuf = PhysPageBuf::new(bs);

        for (i, ck) in buf.chunks_mut(bs).enumerate() {
            self.ns.read(lba + i as u64, &mut pabuf).map_err(|e|
                format!("NVMe read error: {:?}", e)
            )?;
            ck.copy_from_slice(&pabuf[..ck.len()]);
        }

        return Ok(());
    }

    fn write_block(&self, buf: &[u8], lba: u64) -> Result<(), String> {
        // PhysPageBuf ensures both address and size alignment to 4 kiB
        // via AllocParams settings.
        let bs = self.block_size() as usize;
        let mut pabuf = PhysPageBuf::new(bs);

        for (i, ck) in buf.chunks(bs).enumerate() {
            if ck.len() < bs {
                self.ns.read(lba + i as u64, &mut pabuf).map_err(|e|
                    format!("NVMe read error: {:?}", e)
                )?;
            }
            pabuf[..ck.len()].copy_from_slice(ck);
            self.ns.write(lba + i as u64, &pabuf).map_err(|e|
                format!("NVMe write error: {:?}", e)
            )?;
        }

        return Ok(());
    }

    fn devid(&self) -> u64 {
        return DevId::new(0)
            .ty(BlockDevType::PCIe)
            .loc(((self.devid as u32) << 16) | self.ns.id())
            .build();
    }
}

pub static NVME_DEV: Mutex<BTreeMap<u16, Arc<NVMeDev<NVMeAlloc>>>> = Mutex::new(BTreeMap::new());

pub fn init_nvme() {
    let mut nvme_devices = NVME_DEV.lock();
    let mut block_devices = BLOCK_DEVICES.lock();
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        let devid = pci_dev.devid;
        GLACIER.write().map_range(mmio_addr, mmio_addr, PAGE_4KIB * 2, flags::D_RW);
        if let Ok(nvme) = NVMeDev::new(mmio_addr, NVMeAlloc) {
            for ns in nvme.ns_list() {
            block_devices.push(Arc::new(BlockDeviceNVMe::new(ns.clone(), devid)));
            }
            nvme_devices.insert(devid, nvme);
        }
    }
}
