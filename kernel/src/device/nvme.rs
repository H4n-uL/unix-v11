use crate::{
    arch::rvm::flags,
    device::{
        PciDevice,
        block::{BLOCK_DEVICES, BlockDevType, BlockDevice, DevId}
    },
    ram::{
        PhysPageBuf,
        glacier::{GLACIER, page_size},
        physalloc::{AllocParams, PHYS_ALLOC}
    }
};

use alloc::{collections::btree_map::BTreeMap, format, string::String, sync::Arc};
use nvme_oxide::{Dma, NVMeDev, Ns};
use spin::RwLock;

pub struct NVMeAlloc;

impl Dma for NVMeAlloc {
    unsafe fn alloc(&self, size: usize, align: usize) -> Option<usize> {
        return PHYS_ALLOC.alloc(
            AllocParams::new(size)
                .align(align)
        ).map(|p| p.addr());
    }

    unsafe fn free(&self, addr: usize, size: usize, _: usize) {
        unsafe {
            PHYS_ALLOC.free_raw(
                addr as *mut u8,
                size.next_power_of_two()
            );
        }
    }

    unsafe fn map_mmio(&self, phys: usize, size: usize) -> Option<usize> {
        GLACIER.write().map_range(phys, phys, size, flags::D_RW).ok()?;
        return Some(phys);
    }

    unsafe fn unmap_mmio(&self, virt: usize, size: usize) {
        GLACIER.write().unmap_range(virt, size);
    }

    fn virt_to_phys(&self, va: usize) -> usize { return va; }

    fn page_size(&self) -> usize { return page_size(); }
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
        let mut pabuf = PhysPageBuf::new(bs)
            .ok_or("Failed to allocate DMA buffer")?;

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
        let mut pabuf = PhysPageBuf::new(bs)
            .ok_or("Failed to allocate DMA buffer")?;

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

pub static NVME_DEV: RwLock<BTreeMap<u16, Arc<NVMeDev<NVMeAlloc>>>> = RwLock::new(BTreeMap::new());

pub fn add(dev: &mut PciDevice) {
    if !dev.is_nvme() {
        return;
    }

    dev.enable_pci_device();

    let devid = dev.devid;
    if let Ok(nvme) = NVMeDev::new(dev.mmio_addr(), NVMeAlloc) {
        let mut nvme_devices = NVME_DEV.write();
        let mut block_devices = BLOCK_DEVICES.write();
        for ns in nvme.ns_list() {
            block_devices.push(Arc::new(BlockDeviceNVMe::new(ns.clone(), devid)));
        }
        nvme_devices.insert(devid, nvme);
    }
}
