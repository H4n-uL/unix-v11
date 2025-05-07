use crate::{printk, EMBER};
use acpi::{mcfg::Mcfg, AcpiHandler, AcpiTables, PhysicalMapping};

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
                _ => panic!("Unknown PCI header type")
            }
        };

        return Some(PciDevice { bus, device, function, header, config });
    }

    pub fn cfg(&self) -> &PciConfig { &self.config }
    pub fn is_nvme(&self) -> bool { self.header.class == 0x01 && self.header.subclass == 0x08 }
    pub fn is_vga(&self) -> bool { self.header.class == 0x03 && self.header.subclass == 0x00 }
    pub fn is_bridge(&self) -> bool { self.header.header_type & 0x7f == 1 }
}

fn scan_pcie_devices(mcfg_base: u64, start_bus: u8, end_bus: u8) -> alloc::vec::Vec<PciDevice> {
    let mut devices = alloc::vec::Vec::new();

    for bus in start_bus..=end_bus { for device in 0..32 { for function in 0..8 {
        if let Some(dev) = PciDevice::read(mcfg_base, bus, device, function) {
            devices.push(dev);
        }
    }}}

    return devices;
}

pub fn init_acpi(rsdp_addr: usize) -> Result<AcpiTables<KernelAcpiHandler>, &'static str> {
    let handler = KernelAcpiHandler;
    let tables = unsafe { AcpiTables::from_rsdp(handler, rsdp_addr) }
        .map_err(|_| "Failed to initialize ACPI tables")?;

    return Ok(tables);
}

pub fn init_device() {
    let acpi = init_acpi(EMBER.lock().acpi_rsdp_ptr);
    if let Err(e) = acpi { panic!("ACPI init failed: {}", e); }
    let tables = acpi.unwrap();
    let mcfg = tables.find_table::<Mcfg>();
    if mcfg.is_err() { panic!("MCFG not found! Cannot scan PCIe devices."); }
    let mcfg = mcfg.unwrap();
    let mcfg_data = mcfg.get();

    for entry in mcfg_data.entries() {
        let mcfg_base = entry.base_address;
        let start_bus = entry.bus_number_start;
        let end_bus = entry.bus_number_end;
        let devices = scan_pcie_devices(mcfg_base, start_bus, end_bus);

        for dev in devices {
            printk!(
                "/bus{}/dev{}/fn{} | {:04x}:{:04x} Class {:02x}.{:02x} IF {:02x}",
                dev.bus, dev.device, dev.function,
                dev.header.vendor_id, dev.header.device_id,
                dev.header.class, dev.header.subclass, dev.header.prog_if
            );

            if dev.is_nvme()   { printk!(" --> NVMe Controller"); }
            if dev.is_vga()    { printk!(" --> VGA Compatible Controller"); }
            if dev.is_bridge() { printk!(" (PCI Bridge)"); }
            printk!("\n");
        }
    }
}