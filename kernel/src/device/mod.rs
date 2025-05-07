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
    pub config: PciHeader
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

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PciConfigT0 {
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PciConfigT1 {
    pub bar0: u32,
    pub bar1: u32,
    pub cardbus_cis_ptr: u32,
    pub subsystem_id: u16,
    pub vendor_id: u16,
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

impl PciDevice {
    pub fn read(mcfg_base: u64, bus: u8, device: u8, function: u8) -> Option<Self> {
        let base_ptr = mcfg_base + ((bus as u64) << 20) + ((device as u64) << 15) + ((function as u64) << 12);
        let config = unsafe { *(base_ptr as *const PciHeader) };
        if config.vendor_id == 0xFFFF { return None; }
        return Some(PciDevice { bus, device, function, config });
    }
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
                dev.config.vendor_id, dev.config.device_id,
                dev.config.class, dev.config.subclass, dev.config.prog_if
            );

            if dev.config.class == 0x01 && dev.config.subclass == 0x08 { printk!(" --> NVMe Controller Detected!"); }
            if dev.config.class == 0x03 && dev.config.subclass == 0x00 { printk!(" --> VGA Compatible Controller Detected!"); }
            printk!("\n");
        }
    }
}