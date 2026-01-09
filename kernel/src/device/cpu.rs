use crate::{device::ACPI, printlnk};

use acpi::sdt::madt::{Madt, MadtEntry};

pub fn init_cpu() {
    let acpi_mutex = ACPI.read();
    let Some(acpi) = acpi_mutex.as_ref() else {
        return;
    };

    let Some(madt) = acpi.find_table::<Madt>() else {
        return;
    };

    for entry in madt.get().entries() {
        match entry {
            MadtEntry::LocalApic(lapic) => {
                let puid = lapic.processor_id;
                let apicid = lapic.apic_id;
                let flags = lapic.flags;
                printlnk!("APIC CPU ID {}, APIC ID {}, Flags {:#x}", puid, apicid, flags);
            }
            MadtEntry::Gicc(gicc) => {
                let puid = gicc.processor_uid;
                let mpidr = gicc.mpidr;
                let flags = gicc.flags;
                printlnk!("GIC CPU UID {}, MPIDR {:#x}, Flags {:#x}", puid, mpidr, flags);
            }
            _ => {}
        }
    }

    // for entry in madt.get().entries() {
    //     let puid = match entry {
    //         MadtEntry::LocalApic(lapic) => lapic.apic_id as usize,
    //         MadtEntry::Gicc(gicc) => gicc.processor_uid as usize,
    //         _ => continue
    //     };
    //     printlnk!("Waking up CPU {}", puid);
    // }
}
