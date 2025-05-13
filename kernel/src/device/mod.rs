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
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub header: PciHeader,
    pub config: PciConfig
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PciHeader {
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class: u8,
    pub cache_line_size: u8,
    pub latency_timer: u8,
    pub header_type: u8,
    pub bist: u8
}

#[derive(Clone, Copy, Debug)]
pub enum PciConfig {
    Type0(PciConfigType0),
    Type1(PciConfigType1)
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PciConfigType0 {
    pub bar: [u32; 6],
    pub cardbus_cis_ptr: u32,
    pub subsystem_id: u16,
    pub subsystem_vendor_id: u16,
    pub expansion_rom_base: u32,
    pub capabilities_ptr: u8,
    pub reserved: [u8; 7],
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub min_grant: u8,
    pub max_latency: u8
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PciConfigType1 {
    pub bar: [u32; 2],
    pub primary_bus: u8,
    pub secondary_bus: u8,
    pub subordinate_bus: u8,
    pub secondary_latency: u8,
    pub io_base: u8,
    pub io_limit: u8,
    pub secondary_status: u16,
    pub memory_base: u16,
    pub memory_limit: u16,
    pub prefetch_memory_base: u16,
    pub prefetch_memory_limit: u16,
    pub prefetch_base_upper: u32,
    pub prefetch_limit_upper: u32,
    pub io_base_upper: u16,
    pub io_limit_upper: u16,
    pub capabilities_ptr: u8,
    pub reserved0: [u8; 3],
    pub expansion_rom_base: u32,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub bridge_control: u16
}

impl PciDevice {
    pub fn read(mcfg_base: u64, bus: u8, device: u8, function: u8) -> Option<Self> {
        let base_ptr = mcfg_base + ((bus as u64) << 20) + ((device as u64) << 15) + ((function as u64) << 12);
        let header = unsafe { *(base_ptr as *const PciHeader) };
        if header.vendor_id == 0xFFFF { return None; }

        let config_ptr = base_ptr + size_of::<PciHeader>() as u64;
        let config = unsafe {
            match header.header_type & 0x7f {
                0 => PciConfig::Type0(*(config_ptr as *const PciConfigType0)),
                1 => PciConfig::Type1(*(config_ptr as *const PciConfigType1)),
                _ => unreachable!()
            }
        };

        return Some(PciDevice { bus, device, function, header, config });
    }

    pub fn cfg(&self) -> &PciConfig { &self.config }
    pub fn is_nvme(&self) -> bool { self.header.class == 0x01 && self.header.subclass == 0x08 }
    pub fn is_vga(&self) -> bool { self.header.class == 0x03 && self.header.subclass == 0x00 }
    pub fn is_bridge(&self) -> bool { self.header.header_type & 0x7f == 1 }
}

fn scan_pcie_devices(mcfg_base: u64, start_bus: u8, end_bus: u8) -> Vec<PciDevice> {
    let mut devices = Vec::new();

    for bus in start_bus..=end_bus { for device in 0..32 { for function in 0..8 {
        if let Some(dev) = PciDevice::read(mcfg_base, bus, device, function) {
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
    // else if DEVICETREE.lock().is_some() {}
    else { panic!("No ACPI or Device Tree found"); }
}

pub fn init_acpi() {
    *ACPI.lock() = match unsafe { AcpiTables::from_rsdp(KernelAcpiHandler, EMBER.lock().acpi_ptr) } {
        Ok(tables) => Some(tables),
        Err(_) => None
    };
}

pub fn init_device_tree() {
    // *DEVICETREE.lock() = match unsafe { Fdt::from_ptr(EMBER.lock().dtb_ptr as *const u8) } {
    //     Ok(devtree) => Some(devtree),
    //     Err(_) => None
    // }
}

pub fn init_device() {
    init_acpi();
    init_device_tree();
    scan_pci();

    for dev in PCI_DEVICES.lock().iter() {
        printk!(
            "/bus{}/dev{}/fn{} | {:04x}:{:04x} Class {:02x}.{:02x} IF {:02x}",
            dev.bus, dev.device, dev.function,
            dev.header.vendor_id, dev.header.device_id,
            dev.header.class, dev.header.subclass, dev.header.prog_if
        );

        if dev.is_nvme()   { printk!(" --> NVMe Controller"); }
        if dev.is_vga()    { printk!(" --> VGA Compatible Controller"); }
        if dev.is_bridge() { printk!(" (PCI Bridge)"); }
        printlnk!();
    }
}