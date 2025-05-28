use crate::{ember::ramtype, printlnk, ram::PageAligned, ramblock::RAM_BLOCK_MANAGER};
use super::{block::BlockDevice, PCI_DEVICES};
use alloc::{string::{String, ToString}, vec::Vec};
use nvme::{Allocator, Device, Namespace};
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

pub struct NVMeBlockDevice {
    dev_idx: usize,
    ns_idx: usize,
    namespace: Namespace
}

impl<'a> BlockDevice for NVMeBlockDevice {
    fn read(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), String> {
        let device = &mut NVME_PHYSDEV.lock()[self.dev_idx];
        let max_queue = device.controller_data().max_queue_entries as usize;
        let queue = device.create_io_queue_pair(self.namespace.clone(), max_queue);
        if queue.is_err() { return Err(queue.err().unwrap().to_string()); }
        let res = queue.unwrap().read(buffer.as_mut_ptr(), buffer.len(), lba);
        return res.map_err(|e| e.to_string());
    }

    fn write(&mut self, lba: u64, buffer: &[u8]) -> Result<(), String> {
        let device = &mut NVME_PHYSDEV.lock()[self.dev_idx];
        let max_queue = device.controller_data().max_queue_entries as usize;
        let queue = device.create_io_queue_pair(self.namespace.clone(), max_queue);
        if queue.is_err() { return Err(queue.err().unwrap().to_string()); }
        let res = queue.unwrap().write(buffer.as_ptr(), buffer.len(), lba);
        return res.map_err(|e| e.to_string());
    }
}

static NVME_PHYSDEV: Mutex<Vec<Device<NVMeAlloc>>> = Mutex::new(Vec::new());
static NVME_NS: Mutex<Vec<NVMeBlockDevice>> = Mutex::new(Vec::new());

pub fn init_nvme() {
    let mut nvme_physdev = NVME_PHYSDEV.lock();
    let mut nvme_ns = NVME_NS.lock();
    for dev in PCI_DEVICES.lock().iter() {
        if dev.is_nvme() {
            let base = dev.bar(0).unwrap() as usize;
            let mmio_addr = if (base & 0b110) == 0b100 {
                ((dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
            } else { base & !0b11 };

            let mut device = Device::init(mmio_addr, NVMeAlloc).unwrap();
            let dev_idx = nvme_physdev.len();
            for (ns_idx, ns) in device.identify_namespaces(0).unwrap().iter().enumerate() {
                nvme_ns.push(NVMeBlockDevice { dev_idx, ns_idx, namespace: ns.clone() });
            }
            nvme_physdev.push(device);
        }
    }
}

pub fn test_nvme() {
    let mut namespaces = NVME_NS.lock();

    if namespaces.is_empty() {
        printlnk!("No NVMe namespaces found");
        return;
    }

    let dev = &mut namespaces[0];
    let mut buffer = PageAligned::<4096>::new();

    match dev.read(0, buffer.as_mut_slice()) {
        Ok(_) => printlnk!("Read success: {:?}", &buffer[..16]),
        Err(e) => printlnk!("Read failed: {}", e),
    }
}
