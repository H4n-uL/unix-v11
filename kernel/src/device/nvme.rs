use crate::{
    device::block::{BlockDevice, BLOCK_DEVICES},
    ram::{glacier::GLACIER, physalloc::{AllocParams, PHYS_ALLOC}, PAGE_4KIB}
};
use super::PCI_DEVICES;
use alloc::{boxed::Box, format, string::String, vec::Vec};
use nvme::{Allocator, Device};
use spin::Mutex;

pub struct NVMeAlloc;

impl Allocator for NVMeAlloc {
    unsafe fn allocate(&self, size: usize) -> usize {
        return PHYS_ALLOC.alloc(AllocParams::new(size)).unwrap().addr();
    }

    unsafe fn deallocate(&self, addr: usize, size: usize) {
        unsafe { PHYS_ALLOC.free_raw(addr as *mut u8, size); }
    }

    fn translate(&self, addr: usize) -> usize { addr }
}

#[derive(Copy, Clone, Debug)]
pub struct NVMeBlockDevice {
    devid: usize,
    nsid: u32
}

impl NVMeBlockDevice {
    pub fn new(devid: usize, nsid: u32) -> Self {
        Self { devid, nsid }
    }
}

impl BlockDevice for NVMeBlockDevice {
    fn block_size(&self) -> usize {
        if self.devid >= NVME_DEV_LOW.lock().len() { return 0; }
        let device = &NVME_DEV_LOW.lock()[self.devid];
        return device.get_ns(self.nsid)
            .expect("Invalid namespace").block_size() as usize;
    }

    fn block_count(&self) -> usize {
        if self.devid >= NVME_DEV_LOW.lock().len() { return 0; }
        let device = &NVME_DEV_LOW.lock()[self.devid];
        return device.get_ns(self.nsid)
            .expect("Invalid namespace").block_count() as usize;
    }

    fn read(&self, lba: u64, buffer: &mut [u8]) -> Result<(), String> {
        if self.devid >= NVME_DEV_LOW.lock().len() { return Err("Invalid device index".into()); }
        let device = &NVME_DEV_LOW.lock()[self.devid];
        let ns = device.get_ns(self.nsid)
            .ok_or_else(|| String::from("Invalid namespace"))?;

        return ns.read(lba, buffer).map_err(|e|
            format!("NVMe read error: {}", e)
        );
    }

    fn write(&self, lba: u64, buffer: &[u8]) -> Result<(), String> {
        if self.devid >= NVME_DEV_LOW.lock().len() { return Err("Invalid device index".into()); }
        let device = &NVME_DEV_LOW.lock()[self.devid];
        let ns = device.get_ns(self.nsid)
            .ok_or_else(|| String::from("Invalid namespace"))?;

        return ns.write(lba, buffer).map_err(|e|
            format!("NVMe write error: {}", e)
        );
    }
}

static NVME_DEV_LOW: Mutex<Vec<Device<NVMeAlloc>>> = Mutex::new(Vec::new());
static NVME_DEV: Mutex<Vec<NVMeBlockDevice>> = Mutex::new(Vec::new());

pub fn init_nvme() {
    let mut nvme_dev_low = NVME_DEV_LOW.lock();
    let mut nvme_dev = NVME_DEV.lock();
    let mut block_devices = BLOCK_DEVICES.lock();
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        GLACIER.map_range(mmio_addr, mmio_addr, PAGE_4KIB * 2, crate::arch::mmu::flags::PAGE_DEVICE);
        let nvme_device = Device::init(mmio_addr, NVMeAlloc).unwrap();
        for ns in nvme_device.list_namespaces() {
            let dev = NVMeBlockDevice::new(nvme_dev_low.len(), ns);
            block_devices.push(Box::new(dev));
            nvme_dev.push(dev);
        }
        nvme_dev_low.push(nvme_device);
    }
}