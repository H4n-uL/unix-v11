use crate::{ember::ramtype, printlnk, ram::PageAligned, ramblock::RAM_BLOCK_MANAGER};
use super::PCI_DEVICES;
use alloc::vec::Vec;
use nvme::{Allocator, Device};
use spin::Mutex;

pub struct NVMeAlloc;

impl Allocator for NVMeAlloc {
    unsafe fn allocate(&self, size: usize) -> usize {
        let mut ramblock = RAM_BLOCK_MANAGER.lock();
        let ptr = ramblock.alloc(size, ramtype::CONVENTIONAL).unwrap();
        return ptr.addr();
    }

    unsafe fn deallocate(&self, addr: usize) {
        let mut ramblock = RAM_BLOCK_MANAGER.lock();
        ramblock.free_raw(addr as *mut u8);
    }

    fn translate(&self, addr: usize) -> usize { addr }
}

static NVME_DEV: Mutex<Vec<Device<NVMeAlloc>>> = Mutex::new(Vec::new());

pub fn init_nvme() {
    let mut nvme_dev = NVME_DEV.lock();
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        let nvme_device = Device::init(mmio_addr, NVMeAlloc).unwrap();
        nvme_dev.push(nvme_device);
    }
}

pub fn test_nvme() {
    let nvme_dev_ls = NVME_DEV.lock();

    if nvme_dev_ls.is_empty() {
        printlnk!("No NVMe namespaces found");
        return;
    }

    let nvme_dev = &nvme_dev_ls[0];

    printlnk!("{:?}", nvme_dev.nvme_version());

    let mut buffer = PageAligned::new(4096);
    for nsi in nvme_dev.list_namespaces() {
        match nvme_dev.get_ns(nsi).unwrap().read(0, &mut buffer) {
            Ok(_) => printlnk!("Read success from namespace {}: {} bytes", nsi, buffer.len()),
            Err(e) => printlnk!("Read failed from namespace {}: {}", nsi, e),
        }
    }
}