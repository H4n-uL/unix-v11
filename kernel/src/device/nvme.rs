use crate::{device::block::BlockDevice, glacier::{AllocParams, GLACIER}, printlnk, ram::PageAligned};
use super::PCI_DEVICES;
use alloc::{format, string::String, vec::Vec};
use nvme::{Allocator, Device};
use spin::Mutex;

pub struct NVMeAlloc;

impl Allocator for NVMeAlloc {
    unsafe fn allocate(&self, size: usize) -> usize {
        return GLACIER.alloc(AllocParams::new(size)).unwrap().addr();
    }

    unsafe fn deallocate(&self, addr: usize, size: usize) {
        unsafe { GLACIER.free_raw(addr as *mut u8, size); }
    }

    fn translate(&self, addr: usize) -> usize { addr }
}

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
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        let nvme_device = Device::init(mmio_addr, NVMeAlloc).unwrap();
        for ns in nvme_device.list_namespaces() {
            nvme_dev.push(NVMeBlockDevice::new(nvme_dev_low.len(), ns));
        }
        nvme_dev_low.push(nvme_device);
    }
}

pub fn test_nvme() {
    let nvme_dev_ls = NVME_DEV.lock();

    if nvme_dev_ls.is_empty() {
        printlnk!("No NVMe namespaces found");
        return;
    }

    let nvme_dev = &nvme_dev_ls[0];

    let mut buffer = PageAligned::new(4096);
    match nvme_dev.read(0, &mut buffer) {
        Ok(_) => printlnk!("Read success from NVMe device {}: {} bytes", nvme_dev.devid, buffer.len()),
        Err(e) => printlnk!("Read failed from NVMe device {}: {}", nvme_dev.devid, e),
    }
}