ENTRY(_start)

PHDRS {
    text   PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
    bss    PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x10000;
    .text   ALIGN(0x10000) : { *(.text*) }   : text
    .rodata ALIGN(0x10000) : { *(.rodata*) } : rodata
    .data   ALIGN(0x10000) : { *(.data*) }   : data
    .bss    ALIGN(0x10000) : { *(.bss*) }    : bss
}
