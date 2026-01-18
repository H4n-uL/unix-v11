use crate::{
    arch::rvm::flags, device::PciDevice, printk,
    ram::{
        align_up,
        glacier::{GLACIER, page_size},
        physalloc::{AllocParams, PHYS_ALLOC}
    }
};

use alloc::{string::String, sync::Arc, vec::Vec};
use spin::RwLock;
use usb_oxide::{Dma, UsbDevice, XhciCtrl};

pub struct UsbAlloc;

impl Dma for UsbAlloc {
    unsafe fn alloc(&self, size: usize, align: usize) -> usize {
        return PHYS_ALLOC.alloc(
            AllocParams::new(size).align(align)
        ).map(|p| p.addr()).unwrap_or(0)
    }

    unsafe fn free(&self, addr: usize, size: usize, align: usize) {
        unsafe {
            PHYS_ALLOC.free_raw(addr as *mut u8, align_up(size, align));
        }
    }

    unsafe fn map_mmio(&self, phys: usize, size: usize) -> usize {
        GLACIER.write().map_range(phys, phys, size, flags::D_RW);
        return phys;
    }

    unsafe fn unmap_mmio(&self, virt: usize, size: usize) {
        GLACIER.write().unmap_range(virt, size);
    }

    fn virt_to_phys(&self, va: usize) -> usize { return va; }

    fn page_size(&self) -> usize { return page_size(); }
}

pub static XHCI_CTRLS: RwLock<Vec<Arc<XhciCtrl<UsbAlloc>>>> = RwLock::new(Vec::new());
pub static USB_DEVICES: RwLock<Vec<UsbDevice<UsbAlloc>>> = RwLock::new(Vec::new());

pub fn add(dev: &mut PciDevice) -> Result<(), String> {
    if !dev.is_usb() {
        return Err("Not a USB device".into());
    }
    if dev.prog_if() != 0x30 {
        return Err("Not an xHCI controller".into());
    }

    dev.enable_pci_device();

    let ctrl = XhciCtrl::new(dev.mmio_addr(), UsbAlloc)
        .map_err(|e| alloc::format!("xHCI init failed: {:?}", e))?;
    let ctrl = Arc::new(ctrl);

    for port in 0..ctrl.max_ports() {
        if !ctrl.port_connected(port) {
            continue;
        }

        match UsbDevice::new(ctrl.clone(), port) {
            Ok(device) => {
                USB_DEVICES.write().push(device);
            }
            Err(e) => {
                printk!("\nUSB: Failed to address device on port {}: {:?}", port, e);
            }
        }
    }

    XHCI_CTRLS.write().push(ctrl);
    Ok(())
}
