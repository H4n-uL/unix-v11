use crate::{arch::rvm::flags, device::PCI_DEVICES, ram::glacier::GLACIER};

#[allow(unused)]
use core::{arch::asm, ptr::NonNull};
pub use acpi::*;
use acpi::aml::AmlError;

#[derive(Clone, Copy, Debug)]
pub struct KernelAcpiHandler;

fn find_dev_ptr(addr: PciAddress) -> Option<usize> {
    return PCI_DEVICES.read().iter().find(|d| {
        d.bus() == addr.bus()
        && d.device() == addr.device()
        && d.function() == addr.function()
    }).map(|d| d.ptr() as usize);
}

impl Handler for KernelAcpiHandler {
    unsafe fn map_physical_region<T>(
        &self, phys_addr: usize, size: usize
    ) -> PhysicalMapping<Self, T> {
        GLACIER.write().map_range(phys_addr, phys_addr, size, flags::K_RWO);

        return unsafe { PhysicalMapping {
            physical_start: phys_addr,
            virtual_start: NonNull::new_unchecked(phys_addr as *mut T),
            region_length: size,
            mapped_length: size,
            handler: *self
        } };
    }

    fn unmap_physical_region<T>(region: &PhysicalMapping<Self, T>) {
        GLACIER.write().unmap_range(
            region.virtual_start.as_ptr() as usize,
            region.mapped_length
        );
    }

    fn read_u8(&self, addr: usize) -> u8 { unsafe { *(addr as *const u8) } }
    fn read_u16(&self, addr: usize) -> u16 { unsafe { *(addr as *const u16) } }
    fn read_u32(&self, addr: usize) -> u32 { unsafe { *(addr as *const u32) } }
    fn read_u64(&self, addr: usize) -> u64 { unsafe { *(addr as *const u64) } }

    fn write_u8(&self, addr: usize, val: u8) { unsafe { *(addr as *mut u8) = val; } }
    fn write_u16(&self, addr: usize, val: u16) { unsafe { *(addr as *mut u16) = val; } }
    fn write_u32(&self, addr: usize, val: u32) { unsafe { *(addr as *mut u32) = val; } }
    fn write_u64(&self, addr: usize, val: u64) { unsafe { *(addr as *mut u64) = val; } }

    fn read_io_u8(&self, port: u16) -> u8 {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let val: u8;
            asm!("in al, dx", in("dx") port, out("al") val);
            return val;
        }
        #[cfg(not(target_arch = "x86_64"))]
        { let _ = port; return 0; }
    }
    fn read_io_u16(&self, port: u16) -> u16 {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let val: u16;
            asm!("in ax, dx", in("dx") port, out("ax") val);
            return val;
        }
        #[cfg(not(target_arch = "x86_64"))]
        { let _ = port; return 0; }
    }
    fn read_io_u32(&self, port: u16) -> u32 {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let val: u32;
            asm!("in eax, dx", in("dx") port, out("eax") val);
            return val;
        }
        #[cfg(not(target_arch = "x86_64"))]
        { let _ = port; return 0; }
    }
    fn write_io_u8(&self, port: u16, val: u8) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") val);
        }
        #[cfg(not(target_arch = "x86_64"))]
        { let _ = (port, val); }
    }
    fn write_io_u16(&self, port: u16, val: u16) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            asm!("out dx, ax", in("dx") port, in("ax") val);
        }
        #[cfg(not(target_arch = "x86_64"))]
        { let _ = (port, val); }
    }
    fn write_io_u32(&self, port: u16, val: u32) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            asm!("out dx, eax", in("dx") port, in("eax") val);
        }
        #[cfg(not(target_arch = "x86_64"))]
        { let _ = (port, val); }
    }

    fn read_pci_u8(&self, addr: PciAddress, offset: u16) -> u8 {
        if let Some(dev_ptr) = find_dev_ptr(addr) {
            unsafe { *((dev_ptr + offset as usize) as *const u8) }
        } else {
            0xff
        }
    }
    fn read_pci_u16(&self, addr: PciAddress, offset: u16) -> u16 {
        if let Some(dev_ptr) = find_dev_ptr(addr) {
            unsafe { *((dev_ptr + offset as usize) as *const u16) }
        } else {
            0xffff
        }
    }
    fn read_pci_u32(&self, addr: PciAddress, offset: u16) -> u32 {
        if let Some(dev_ptr) = find_dev_ptr(addr) {
            unsafe { *((dev_ptr + offset as usize) as *const u32) }
        } else {
            0xffffffff
        }
    }
    fn write_pci_u8(&self, addr: PciAddress, offset: u16, val: u8) {
        if let Some(dev_ptr) = find_dev_ptr(addr) {
            unsafe { *((dev_ptr + offset as usize) as *mut u8) = val; }
        }
    }
    fn write_pci_u16(&self, addr: PciAddress, offset: u16, val: u16) {
        if let Some(dev_ptr) = find_dev_ptr(addr) {
            unsafe { *((dev_ptr + offset as usize) as *mut u16) = val; }
        }
    }
    fn write_pci_u32(&self, addr: PciAddress, offset: u16, val: u32) {
        if let Some(dev_ptr) = find_dev_ptr(addr) {
            unsafe { *((dev_ptr + offset as usize) as *mut u32) = val; }
        }
    }

    fn nanos_since_boot(&self) -> u64 { 0 }
    fn stall(&self, _us: u64) {}
    fn sleep(&self, _ms: u64) {}

    fn create_mutex(&self) -> Handle { Handle(0) }
    fn acquire(&self, _mutex: Handle, _timeout: u16) -> Result<(), AmlError> { Ok(()) }
    fn release(&self, _mutex: Handle) {}
}
