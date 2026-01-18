mod acpi;
pub mod block;
mod cpu;
mod nvme;
mod usb;
mod vga;

use crate::{
    arch::rvm::flags,
    device::acpi::KernelAcpiHandler,
    kargs::SYSINFO,
    printk, printlnk,
    ram::glacier::{GLACIER, page_size}
};

use alloc::{string::String, vec::Vec};
use acpi::{sdt::mcfg::Mcfg, AcpiTables};
use fdt::Fdt;
use spin::RwLock;

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    devid: u16,
    ptr: *mut u32
}

unsafe impl Send for PciDevice {}
unsafe impl Sync for PciDevice {}

#[allow(dead_code)]
impl PciDevice {
    pub fn read(base: u64, devid: u16) -> Option<Self> {
        let ptr = base as usize + ((devid as usize) << 12);
        GLACIER.write().map_range(ptr, ptr, page_size(), flags::D_RW);
        let dev = PciDevice { devid, ptr: ptr as *mut u32 };
        if dev.vendor_id() == 0xFFFF { return None; }
        return Some(dev);
    }

    pub fn bus(&self) -> u8 { (self.devid >> 8) as u8 }
    pub fn device(&self) -> u8 { (self.devid >> 3) as u8 & 0x1f }
    pub fn function(&self) -> u8 { self.devid as u8 & 0x07 }
    pub fn ptr(&self) -> *mut u32 { self.ptr }

    pub fn enable_pci_device(&mut self) { self.set_command(self.command() | 0x0006); }

    pub fn is_nvme(&self) -> bool { self.class() == 0x01 && self.subclass() == 0x08 }
    pub fn is_usb(&self) -> bool { self.class() == 0x0c && self.subclass() == 0x03 }
    pub fn is_display(&self) -> bool { self.class() == 0x03 }
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

    pub fn mmio_addr(&self) -> usize {
        let base = self.bar(0).unwrap() as usize;
        if (base & 0b110) == 0b100 {
            return ((self.bar(1).unwrap() as usize) << 32) | (base & !0b111);
        } else {
            return base & !0b11;
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

    pub fn secondary_latency(&self) -> u8      { (self.blob()[6] >> 24) as u8 }
    pub fn subordinate_bus(&self) -> u8        { (self.blob()[6] >> 16) as u8 }
    pub fn secondary_bus(&self) -> u8          { (self.blob()[6] >> 8) as u8 }
    pub fn primary_bus(&self) -> u8            { self.blob()[6] as u8 }

    pub fn secondary_status(&self) -> u16      { (self.blob()[7] >> 16) as u16 }
    pub fn io_limit(&self) -> u8               { (self.blob()[7] >> 8) as u8 }
    pub fn io_base(&self) -> u8                { self.blob()[7] as u8 }

    pub fn memory_limit(&self) -> u16          { (self.blob()[8] >> 16) as u16 }
    pub fn memory_base(&self) -> u16           { self.blob()[8] as u16 }

    pub fn prefetch_memory_limit(&self) -> u16 { (self.blob()[9] >> 16) as u16 }
    pub fn prefetch_memory_base(&self) -> u16  { self.blob()[9] as u16 }

    pub fn prefetch_base_upper(&self) -> u32   { self.blob()[10] }
    pub fn prefetch_limit_upper(&self) -> u32  { self.blob()[11] }

    pub fn io_limit_upper(&self) -> u16        { (self.blob()[12] >> 16) as u16 }
    pub fn io_base_upper(&self) -> u16         {  self.blob()[12] as u16 }

    pub fn bridge_control(&self) -> u16        { (self.blob()[15] >> 16) as u16 }
}

fn scan_pcie_devices(base: u64, start_bus: u8, end_bus: u8) -> Vec<PciDevice> {
    let mut devices = Vec::new();
    let (start, end) = ((start_bus as u16) << 8, ((end_bus as u16 + 1) << 8).wrapping_sub(1));

    for devid in start..=end {
        if let Some(mut dev) = PciDevice::read(base, devid) {
            dev.enable_pci_device();
            devices.push(dev);
        }
    }

    return devices;
}

pub static PCI_DEVICES: RwLock<Vec<PciDevice>> = RwLock::new(Vec::new());
pub static ACPI: RwLock<Option<AcpiTables<KernelAcpiHandler>>> = RwLock::new(None);
pub static DEVICETREE: RwLock<Option<Fdt>> = RwLock::new(None);

pub fn scan_pci() {
    let mut pci = PCI_DEVICES.write();

    if let Some(acpi) = ACPI.read().as_ref() {
        if let Some(mcfg) = acpi.find_table::<Mcfg>() {
            *pci = mcfg.get().entries().iter().flat_map(|entry| {
                let mcfg_base = entry.base_address;
                let start_bus = entry.bus_number_start;
                let end_bus = entry.bus_number_end;
                scan_pcie_devices(mcfg_base, start_bus, end_bus)
            }).collect();
        } else {
            panic!("No PCIe devices found")
        }
    }
    if let Some(dtb) = DEVICETREE.read().as_ref() {
        *pci = dtb.all_nodes().flat_map(|node| {
            if let Some(compatible) = node.properties().find(|p| p.name == "compatible") {
                let compat_str = String::from_utf8_lossy(compatible.value);

                if compat_str.contains("pcie") || compat_str.contains("pci") {
                    if let Some(reg_prop) = node.properties().find(|p| p.name == "reg") {
                        let reg_data = reg_prop.value;
                        if reg_data.len() < 8 { return Vec::new(); }
                        let ecam_base = u64::from_be_bytes([
                            reg_data[0], reg_data[1], reg_data[2], reg_data[3],
                            reg_data[4], reg_data[5], reg_data[6], reg_data[7]
                        ]);

                        let (start_bus, end_bus) =
                        match node.properties().find(|p| p.name == "bus-range") {
                            Some(bus_range) => (bus_range.value[3], bus_range.value[7]),
                            None => (0, 255)
                        };

                        return scan_pcie_devices(ecam_base, start_bus, end_bus);
                    }
                }
            }
            return Vec::new();
        }).collect();
    }
}

pub fn init_acpi() {
    let ptr = SYSINFO.read().acpi_ptr;
    *ACPI.write() = match unsafe { AcpiTables::from_rsdp(KernelAcpiHandler, ptr) } {
        Ok(tables) => Some(tables),
        Err(_) => None
    };
}

pub fn init_device_tree() {
    let ptr = SYSINFO.read().dtb_ptr;
    *DEVICETREE.write() = match unsafe { Fdt::from_ptr(ptr as *const u8) } {
        Ok(devtree) => Some(devtree),
        Err(_) => None
    }
}

pub fn init_device() {
    init_acpi();
    init_device_tree();
    scan_pci();

    for dev in PCI_DEVICES.write().iter_mut() {
        printk!(
            "/bus{}/dev{}/fn{} | {:04x}:{:04x} Class {:02x}.{:02x} IF {:02x}",
            dev.bus(), dev.device(), dev.function(),
            dev.vendor_id(), dev.device_id(),
            dev.class(), dev.subclass(), dev.prog_if()
        );

        if dev.is_nvme() {
            printk!(" --> NVMe Controller");
            nvme::add(dev);
        }

        if dev.is_usb()     {
            printk!(" --> USB Controller");
            let _ = usb::add(dev);
        }

        if dev.is_display() { printk!(" --> Display Controller"); }
        if dev.is_bridge()  { printk!(" (PCI Bridge)"); }

        printlnk!();
    }

    cpu::init_cpu();
    vga::init_vga();
}
