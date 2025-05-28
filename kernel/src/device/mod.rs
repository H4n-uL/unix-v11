mod block; mod nvme;

use crate::{printk, printlnk, EMBER};
use acpi::{mcfg::Mcfg, AcpiHandler, AcpiTables, PhysicalMapping};
use alloc::vec::Vec;
use fdt::Fdt;
use spin::Mutex;

#[derive(Clone, Copy, Debug)]
pub struct KernelAcpiHandler;

impl AcpiHandler for KernelAcpiHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        PhysicalMapping::new(
            physical_address,
            core::ptr::NonNull::new(physical_address as *mut T).unwrap(),
            size, size, Self
        )
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    bus: u8,
    device: u8,
    function: u8,
    ptr: *mut u32
}

unsafe impl Send for PciDevice {}
unsafe impl Sync for PciDevice {}

impl PciDevice {
    pub fn read(mcfg_base: u64, bus: u8, device: u8, function: u8) -> Option<Self> {
        let ptr = (mcfg_base + ((bus as u64) << 20) + ((device as u64) << 15) + ((function as u64) << 12)) as *mut u32;
        let dev = PciDevice { bus, device, function, ptr };
        if dev.vendor_id() == 0xFFFF { return None; }
        return Some(dev);
    }

    pub fn bus(&self) -> u8 { self.bus }
    pub fn device(&self) -> u8 { self.device }
    pub fn function(&self) -> u8 { self.function }
    pub fn ptr(&self) -> *mut u32 { self.ptr }

    pub fn enable_pci_device(&mut self) { self.set_command(self.command() | 0x0006); }

    pub fn is_nvme(&self) -> bool { self.class() == 0x01 && self.subclass() == 0x08 }
    pub fn is_vga(&self) -> bool { self.class() == 0x03 && self.subclass() == 0x00 }
    pub fn is_bridge(&self) -> bool { self.is_type1() }

    fn blob(&self) -> &[u32] { unsafe { core::slice::from_raw_parts(self.ptr, 16) } }
    fn blob_mut(&self) -> &mut [u32] { unsafe { core::slice::from_raw_parts_mut(self.ptr, 16) } }

    // Common methods
    pub fn device_id(&self) -> u16       { (self.blob()[0] >> 16) as u16 }
    pub fn vendor_id(&self) -> u16       {  self.blob()[0] as u16 }

    pub fn status(&self) -> u16          { (self.blob()[1] >> 16) as u16 }
    pub fn command(&self) -> u16         {  self.blob()[1] as u16 }
    pub fn set_command(&mut self, command: u16) { self.blob_mut()[1] = ((self.status() as u32) << 16) | command as u32; }

    pub fn class(&self) -> u8            { (self.blob()[2] >> 24) as u8 }
    pub fn subclass(&self) -> u8         { (self.blob()[2] >> 16) as u8 }
    pub fn prog_if(&self) -> u8          { (self.blob()[2] >> 8) as u8 }
    pub fn reversion_id(&self) -> u8     {  self.blob()[2] as u8 }

    pub fn bist(&self) -> u8             { (self.blob()[3] >> 24) as u8 }
    pub fn header_type(&self) -> u8      { (self.blob()[3] >> 16) as u8 }
    pub fn latency_timer(&self) -> u8    { (self.blob()[3] >> 8) as u8 }
    pub fn cache_line_size(&self) -> u8  {  self.blob()[3] as u8 }

    pub fn capabilities_ptr(&self) -> u8 {  self.blob()[13] as u8 }
    pub fn interrupt_pin(&self) -> u8    { (self.blob()[15] >> 8) as u8 }
    pub fn interrupt_line(&self) -> u8   {  self.blob()[15] as u8 }

    pub fn bar(&self, index: usize) -> Option<u32> {
        let val = self.blob()[4 + index];
        match self.header_type() & 0x7f {
            0 => { if index < 6 { Some(val) } else { None } },
            1 => { if index < 2 { Some(val) } else { None } },
            _ => None
        }
    }

    pub fn expansion_rom_base(&self) -> u32 {
        match self.header_type() & 0x7f {
            0 => self.blob()[12],
            1 => self.blob()[14],
            _ => 0
        }
    }

    // Type 0 specific methods
    pub fn is_type0(&self) -> bool { self.header_type() & 0x7f == 0 }

    pub fn cardbus_cis_ptr(&self) -> u32    {  self.blob()[10] }
    pub fn subsys_id(&self) -> u16          { (self.blob()[11] >> 16) as u16 }
    pub fn subsys_vendor_id(&self) -> u16   {  self.blob()[11] as u16 }

    pub fn max_latency(&self) -> u8         { (self.blob()[15] >> 24) as u8 }
    pub fn min_grant(&self) -> u8           { (self.blob()[15] >> 16) as u8 }

    // Type 1 specific methods
    pub fn is_type1(&self) -> bool { self.header_type() & 0x7f == 1 }

    pub fn secondary_latency(&self) -> u8 { (self.blob()[6] >> 24) as u8 }
    pub fn subordinate_bus(&self) -> u8 { (self.blob()[6] >> 16) as u8 }
    pub fn secondary_bus(&self) -> u8 { (self.blob()[6] >> 8) as u8 }
    pub fn primary_bus(&self) -> u8 { self.blob()[6] as u8 }

    pub fn secondary_status(&self) -> u16 { (self.blob()[7] >> 16) as u16 }
    pub fn io_limit(&self) -> u8 { (self.blob()[7] >> 8) as u8 }
    pub fn io_base(&self) -> u8 { self.blob()[7] as u8 }

    pub fn memory_limit(&self) -> u16 { (self.blob()[8] >> 16) as u16 }
    pub fn memory_base(&self) -> u16 { self.blob()[8] as u16 }

    pub fn prefetch_memory_limit(&self) -> u16 { (self.blob()[9] >> 16) as u16 }
    pub fn prefetch_memory_base(&self) -> u16  { self.blob()[9] as u16 }

    pub fn prefetch_base_upper(&self) -> u32   { self.blob()[10] }
    pub fn prefetch_limit_upper(&self) -> u32  { self.blob()[11] }

    pub fn io_limit_upper(&self) -> u16 { (self.blob()[12] >> 16) as u16 }
    pub fn io_base_upper(&self) -> u16  {  self.blob()[12] as u16 }

    pub fn bridge_control(&self) -> u16 { (self.blob()[15] >> 16) as u16 }
}

fn scan_pcie_devices(mcfg_base: u64, start_bus: u8, end_bus: u8) -> Vec<PciDevice> {
    let mut devices = Vec::new();

    for bus in start_bus..=end_bus { for device in 0..32 { for function in 0..8 {
        if let Some(mut dev) = PciDevice::read(mcfg_base, bus, device, function) {
            dev.enable_pci_device();
            devices.push(dev);
        }
    }}}

    return devices;
}

pub static PCI_DEVICES: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());
pub static ACPI: Mutex<Option<AcpiTables<KernelAcpiHandler>>> = Mutex::new(None);
pub static DEVICETREE: Mutex<Option<Fdt>> = Mutex::new(None);

pub fn scan_pci() {
    if let Some(acpi) = ACPI.lock().as_ref() {
        match acpi.find_table::<Mcfg>() {
            Ok(mcfg) => {
                *PCI_DEVICES.lock() = mcfg.get().entries().iter().flat_map(|entry| {
                    let mcfg_base = entry.base_address as u64;
                    let start_bus = entry.bus_number_start;
                    let end_bus = entry.bus_number_end;
                    scan_pcie_devices(mcfg_base, start_bus, end_bus)
                }).collect();
            }
            Err(_) => panic!("No PCIe devices found")
        }
    }
    else if let Some(dtb) = DEVICETREE.lock().as_ref() {
        // dummy
        *PCI_DEVICES.lock() = dtb.all_nodes().filter_map(|node| {
            printlnk!("{:?}", node);
            None
        }).collect();
    }
}

pub fn init_acpi() {
    *ACPI.lock() = match unsafe { AcpiTables::from_rsdp(KernelAcpiHandler, EMBER.lock().acpi_ptr) } {
        Ok(tables) => Some(tables),
        Err(_) => None
    };
}

pub fn init_device_tree() {
    *DEVICETREE.lock() = match unsafe { Fdt::from_ptr(EMBER.lock().dtb_ptr as *const u8) } {
        Ok(devtree) => Some(devtree),
        Err(_) => None
    }
}

pub fn init_device() {
    init_acpi();
    init_device_tree();
    scan_pci();

    for dev in PCI_DEVICES.lock().iter() {
        printk!(
            "/bus{}/dev{}/fn{} | {:04x}:{:04x} Class {:02x}.{:02x} IF {:02x}",
            dev.bus(), dev.device(), dev.function(),
            dev.vendor_id(), dev.device_id(),
            dev.class(), dev.subclass(), dev.prog_if()
        );

        if dev.is_nvme()   { printk!(" --> NVMe Controller"); }
        if dev.is_vga()    { printk!(" --> VGA Compatible Controller"); }
        if dev.is_bridge() { printk!(" (PCI Bridge)"); }
        printlnk!();
    }

    nvme::init_nvme();
    nvme::test_nvme();
}