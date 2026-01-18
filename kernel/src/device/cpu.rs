use crate::{
    arch::{self, rvm::flags}, device::ACPI, printlnk,
    ram::{
        glacier::{GLACIER, page_size},
        per_cpu_data, stack_size, stack_top
    }
};

use acpi::sdt::madt::{Madt, MadtEntry};

pub const IC_DOORBELL_SIZE: usize = 0x10000; // I/O APIC 4K, GICD 64K

// cpu-info Layout:
// +------------------+ - cpu_info_base()
// |  GICC / LAPIC    |       ic_gicc_size() = PAGE_SIZE.max(0x2000)
// +------------------+ - ic_gicc_size()
// |      GICR        |       IC_GICR_SIZE = 128KB
// +------------------+ - ic_gicc_size() + IC_GICR_SIZE

pub fn ic_gicc_size() -> usize { page_size().max(0x2000) } // GICC 8K, LAPIC 4K
pub const IC_GICR_SIZE: usize = 0x20000; // GICR 128K

pub fn cpu_info_size() -> usize {
    per_cpu_data() - stack_size() - page_size() // guard page
}

pub fn cpu_info_base() -> usize {
    stack_top() - per_cpu_data()
}

pub fn ic_gicc_va() -> usize {
    cpu_info_base()
}

pub fn ic_gicr_va() -> usize {
    cpu_info_base() + ic_gicc_size()
}

fn map_doorbell(phys: usize) {
    GLACIER.write().map_range(phys, phys, IC_DOORBELL_SIZE, flags::D_RW);
}

pub fn init_cpu() {
    use MadtEntry::*;

    let acpi_lock = ACPI.read();
    let Some(acpi) = acpi_lock.as_ref() else { return };
    let Some(madt) = acpi.find_table::<Madt>() else { return };
    let madt = madt.get();

    let phys_id = arch::phys_id();
    let mut gicc_phys = None;
    let mut gicr_phys = None;

    #[cfg(target_arch = "x86_64")]
    { gicc_phys = Some(madt.local_apic_address as usize); }

    for entry in madt.entries() {
        match entry {
            // AMD64: LAPIC / I/O APIC
            LocalApic(lapic) => {
                if lapic.apic_id as usize == phys_id {
                    printlnk!("CPU {}: LAPIC @ {:#x}", phys_id, gicc_phys.unwrap_or(0));
                }
            }
            LocalApicAddressOverride(ovr) => {
                gicc_phys = Some(ovr.local_apic_address as usize);
            }
            IoApic(io) => {
                map_doorbell(io.io_apic_address as usize);
            }

            // AArch64: GICC / GICR / GICD
            Gicc(gicc) => {
                let mpidr = gicc.mpidr;
                let gicc_addr = gicc.gic_registers_address;
                let gicr_addr = gicc.gicr_base_address;
                if (mpidr as usize & 0xffff) == phys_id {
                    gicc_phys = Some(gicc_addr as usize);
                    gicr_phys = Some(gicr_addr as usize);
                    printlnk!("CPU {}: GICC @ {:#x}, GICR @ {:#x}", phys_id, gicc_addr, gicr_addr);
                }
            }
            Gicd(gicd) => {
                map_doorbell(gicd.physical_base_address as usize);
            }

            _ => {}
        }
    }

    if let Some(phys) = gicc_phys {
        GLACIER.write().map_range(
            ic_gicc_va(), phys,
            ic_gicc_size(), flags::D_RW
        );
    }

    if let Some(phys) = gicr_phys {
        GLACIER.write().map_range(
            ic_gicr_va(), phys,
            IC_GICR_SIZE, flags::D_RW
        );
    }
}
