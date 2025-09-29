use crate::{
    arch::mmu::flags,
    device::{block::{BlockDevType, BlockDevice, DevId, BLOCK_DEVICES}, PCI_DEVICES},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}, PageAligned, PAGE_4KIB}
};
use alloc::{collections::btree_map::BTreeMap, format, string::String, sync::Arc};
use nvme_rs::{Allocator, NVMeDevice};
use spin::Mutex;

pub struct NVMeAlloc;

impl Allocator for NVMeAlloc {
    unsafe fn alloc(&self, size: usize) -> usize {
        return PHYS_ALLOC.alloc(AllocParams::new(size)).unwrap().addr();
    }

    unsafe fn free(&self, addr: usize, size: usize) {
        unsafe { PHYS_ALLOC.free_raw(addr as *mut u8, size); }
    }

    fn trans(&self, addr: usize) -> usize { addr }
}

pub struct BlockDeviceNVMe {
    dev: Arc<NVMeDevice<NVMeAlloc>>,
    devid: u16,
    nsid: u32
}

impl BlockDeviceNVMe {
    pub fn new(dev: Arc<NVMeDevice<NVMeAlloc>>, devid: u16, nsid: u32) -> Self {
        Self { dev, devid, nsid }
    }
}

impl BlockDevice for BlockDeviceNVMe {
    fn block_size(&self) -> u64 {
        return self.dev.get_ns(self.nsid).map_or(0, |ns| ns.block_size());
    }

    fn block_count(&self) -> u64 {
        return self.dev.get_ns(self.nsid).map_or(0, |ns| ns.block_count());
    }

    fn read_block(&self, buf: &mut [u8], lba: u64) -> Result<(), String> {
        let ns = self.dev.get_ns(self.nsid)
            .ok_or_else(|| String::from("Invalid namespace"))?;

        let mut pabuf = PageAligned::new(buf.len());
        ns.read(lba, &mut pabuf).map_err(|e|
            format!("NVMe read error: {}", e)
        )?;
        buf.copy_from_slice(&pabuf[..buf.len()]);
        return Ok(());
    }

    fn write_block(&self, buf: &[u8], lba: u64) -> Result<(), String> {
        let ns = self.dev.get_ns(self.nsid)
            .ok_or_else(|| String::from("Invalid namespace"))?;

        // PageAligned ensures both address and size alignment to 4 kiB
        // via AllocParams' default settings.
        let mut pabuf = PageAligned::new(buf.len());
        if buf.len() % self.block_size() as usize != 0 {
            ns.read(lba, &mut pabuf).map_err(|e|
                format!("NVMe read error: {}", e)
            )?;
        }
        pabuf[..buf.len()].copy_from_slice(buf);
        return ns.write(lba, &pabuf).map_err(|e|
            format!("NVMe write error: {}", e)
        );
    }

    fn devid(&self) -> u64 {
        return DevId::new(0)
            .ty(BlockDevType::PCIe)
            .loc(((self.devid as u32) << 16) | self.nsid)
            .build();
    }
}

pub static NVME_DEV: Mutex<BTreeMap<u16, Arc<NVMeDevice<NVMeAlloc>>>> = Mutex::new(BTreeMap::new());

pub fn init_nvme() {
    let mut nvme_devices = NVME_DEV.lock();
    let mut block_devices = BLOCK_DEVICES.lock();
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        let devid = pci_dev.devid;
        GLACIER.map_range(mmio_addr, mmio_addr, PAGE_4KIB * 2, flags::D_RW);
        let nvme_arc = NVMeDevice::init(mmio_addr, NVMeAlloc).unwrap();
        for ns in nvme_arc.list_ns() {
            block_devices.push(Arc::new(BlockDeviceNVMe::new(nvme_arc.clone(), devid, ns)));
        }
        nvme_devices.insert(devid, nvme_arc.clone());
    }
}
