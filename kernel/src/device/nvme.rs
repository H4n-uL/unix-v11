use crate::{ember::ramtype, printlnk, ram::PageAligned, ramblock::RAM_BLOCK_MANAGER};
use super::{block::BlockDevice, PCI_DEVICES};
use alloc::{string::{String, ToString}, vec::Vec};
use nvme::{Allocator, Device, IoQueuePair, Namespace};
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
    namespace: Namespace,
    queue: Option<IoQueuePair<NVMeAlloc>>
}

impl NVMeBlockDevice {
    pub fn new(dev_idx: usize, namespace: Namespace) -> Self {
        NVMeBlockDevice { dev_idx, namespace, queue: None }
    }

    fn get_or_create_queue(&mut self) -> Result<&mut IoQueuePair<NVMeAlloc>, String> {
        if self.queue.is_none() {
            let device = &mut NVME_DEV.lock()[self.dev_idx];
            let max_queue = device.controller_data().max_queue_entries as usize;
            match device.create_io_queue_pair(self.namespace.clone(), max_queue) {
                Ok(queue) => self.queue = Some(queue),
                Err(e) => return Err(e.to_string())
            }
        }
        return Ok(self.queue.as_mut().unwrap());
    }

    pub fn namespace(&self) -> &Namespace { &self.namespace }
    pub fn devid(&self) -> usize { self.dev_idx }
    pub fn nsid(&self) -> usize { self.namespace.id() as usize }
}

impl<'a> BlockDevice for NVMeBlockDevice {
    fn read(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), String> {
        let queue = self.get_or_create_queue()?;
        return queue.read(buffer.as_mut_ptr(), buffer.len(), lba)
            .map_err(|e| e.to_string());
    }

    fn write(&mut self, lba: u64, buffer: &[u8]) -> Result<(), String> {
        let queue = self.get_or_create_queue()?;
        return queue.write(buffer.as_ptr(), buffer.len(), lba)
            .map_err(|e| e.to_string());
    }
}

static NVME_DEV: Mutex<Vec<Device<NVMeAlloc>>> = Mutex::new(Vec::new());
static NVME_NS: Mutex<Vec<NVMeBlockDevice>> = Mutex::new(Vec::new());

pub fn init_nvme() {
    let mut nvme_dev = NVME_DEV.lock();
    let mut nvme_ns = NVME_NS.lock();
    for pci_dev in PCI_DEVICES.lock().iter().filter(|&dev| dev.is_nvme()) {
        let base = pci_dev.bar(0).unwrap() as usize;
        let mmio_addr = if (base & 0b110) == 0b100 {
            ((pci_dev.bar(1).unwrap() as usize) << 32) | (base & !0b111)
        } else { base & !0b11 };

        let mut nvme_device = Device::init(mmio_addr, NVMeAlloc).unwrap();
        let dev_idx = nvme_dev.len();
        for ns in nvme_device.identify_namespaces(0).unwrap() {
            nvme_ns.push(NVMeBlockDevice::new(dev_idx, ns.clone()));
        }
        nvme_dev.push(nvme_device);
    }
}

pub fn test_nvme() {
    let mut namespaces = NVME_NS.lock();

    if namespaces.is_empty() {
        printlnk!("No NVMe namespaces found");
        return;
    }

    let dev = &mut namespaces[0];
    let mut buffer = PageAligned::new(dev.namespace().block_size() as usize);
    printlnk!("/dev/nvme{}n{}", dev.devid(), dev.nsid());
    match dev.read(0, &mut buffer) {
        Ok(_) => printlnk!("Read success: {} bytes", buffer.len()),
        Err(e) => printlnk!("Read failed: {}", e),
    }
}