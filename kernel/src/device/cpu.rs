use crate::{
    arch::{self, intc, rvm::flags},
    device::ACPI, kargs::AP_LIST,
    ram::{
        glacier::GLACIER,
        per_cpu_data, stack_top
    }
};

use core::sync::atomic::{AtomicUsize, Ordering as AtomOrd};
use acpi::sdt::madt::{Madt, MadtEntry};

pub static GICD_BASE: AtomicUsize = AtomicUsize::new(0);
pub static GICC_BASE: AtomicUsize = AtomicUsize::new(0); // GICv2 GIC CPU intfce
pub static GICR_BASE: AtomicUsize = AtomicUsize::new(0); // GICv3 GIC redistrib
pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

// AMD64:   LAPIC Doorbell  4KB
// AArch64: GICD Doorbell  64KB
pub const IC_DOORBELL_SIZE: usize = 0x10000;

// AMD64:   LAPIC   4KB
// AArch64: GICR  128KB
pub const IC_SIZE: usize = 0x20000;

// cpu-info Layout:
// +------------------+ - cpu_info_base() == ic_va()
// |   LAPIC / GICR   |       IC_SIZE
// +------------------+ - ic_va() + IC_SIZE

pub fn cpu_info_base() -> usize {
    return stack_top() - per_cpu_data();
}

pub fn ic_va() -> usize {
    return cpu_info_base();
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
    let mut ic_phys = None;
    let mut cpu_count = 0usize;

    #[cfg(target_arch = "x86_64")]
    { ic_phys = Some(madt.local_apic_address as usize); }

    for entry in madt.entries() {
        match entry {
            // AMD64
            LocalApic(_) => {
                cpu_count += 1;
            }
            LocalApicAddressOverride(ovr) => {
                ic_phys = Some(ovr.local_apic_address as usize);
            }
            IoApic(io) => {
                map_doorbell(io.io_apic_address as usize);
            }

            // AArch64
            Gicc(gicc) => {
                cpu_count += 1;
                // GICv2: GICC base
                if GICC_BASE.load(AtomOrd::Relaxed) == 0 && gicc.gic_registers_address != 0 {
                    GICC_BASE.store(gicc.gic_registers_address as usize, AtomOrd::Relaxed);
                }
                // GICv3: GICR base
                if GICR_BASE.load(AtomOrd::Relaxed) == 0 && gicc.gicr_base_address != 0 {
                    GICR_BASE.store(gicc.gicr_base_address as usize, AtomOrd::Relaxed);
                }
                if (gicc.mpidr as usize & 0xffff) == phys_id {
                    ic_phys = Some(if gicc.gicr_base_address != 0 {
                        gicc.gicr_base_address as usize
                    } else {
                        gicc.gic_registers_address as usize
                    });
                }
            }
            Gicd(gicd) => {
                let base = gicd.physical_base_address as usize;
                GICD_BASE.store(base, AtomOrd::Relaxed);
                map_doorbell(base);
            }
            GicRedistributor(gicr) => {
                let base = gicr.discovery_range_base_address as usize;
                let len = gicr.discovery_range_length as usize;
                if base != 0 {
                    GICR_BASE.store(base, AtomOrd::Relaxed);
                    GLACIER.write().map_range(base, base, len, flags::D_RW);
                }
            }

            _ => {}
        }
    }

    CPU_COUNT.store(cpu_count, AtomOrd::Relaxed);

    if let Some(phys) = ic_phys {
        GLACIER.write().map_range(ic_va(), phys, IC_SIZE, flags::D_RW);
        intc::init(AP_LIST.virtid_self());
    }
}
