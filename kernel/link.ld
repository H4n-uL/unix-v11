ENTRY(ignite)

PHDRS {
    text    PT_LOAD FLAGS(5);
    rodata  PT_LOAD FLAGS(4);
    data    PT_LOAD FLAGS(6);
    reladyn PT_LOAD FLAGS(4);
    bss     PT_LOAD FLAGS(6);
}

SECTIONS {
    KERNEL_BASE = 0x0;
    . = KERNEL_BASE;

    .text     ALIGN(0x1000) : { *(.text*) }    : text
    .rodata   ALIGN(0x1000) : { *(.rodata*) }  : rodata
    .data     ALIGN(0x1000) : { *(.data*) }    : data
    .rela.dyn ALIGN(8)      : { *(.rela.dyn) } : reladyn
    .bss      ALIGN(0x1000) : { *(.bss*) }     : bss

    . = ALIGN(0x1000);
}
