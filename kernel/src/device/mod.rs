use crate::{ember::Ember, printk};
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
    pub config: PciConfigSpaceHeader
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PciConfigSpaceHeader {
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

impl PciDevice {
    pub fn read(mcfg_base: u64, bus: u8, device: u8, function: u8) -> Option<Self> {
        let base_ptr = mcfg_base + ((bus as u64) << 20) + ((device as u64) << 15) + ((function as u64) << 12);
        let config = unsafe { *(base_ptr as *const PciConfigSpaceHeader) };
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

pub fn init_device(ember: &Ember) {
    let acpi = init_acpi(ember.acpi_rsdp_ptr);
    if acpi.is_err() { panic!("ACPI init failed: {:?}", e); }
    let tables = acpi.unwrap();
    let mcfg = tables.find_table::<Mcfg>();
    if mcfg.is_err() { panic!("MCFG not found! Cannot scan PCIe devices."); }
    let mcfg = mcfg.unwrap();
    let mcfg_data = mcfg.get();

    for entry in mcfg_data.entries() {
        let devices = scan_pcie_devices(
            entry.base_address, entry.bus_number_start, entry.bus_number_end
        );

        for dev in devices {
            printk!(
                "/bus{}/dev{}/fn{} | {:04X}:{:04X} Class {:02X}.{:02X} IF {:02X}",
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