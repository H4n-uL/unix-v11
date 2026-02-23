PHDRS {
    text    PT_LOAD FLAGS(5);
    rodata  PT_LOAD FLAGS(4);
    data    PT_LOAD FLAGS(6);
    reladyn PT_LOAD FLAGS(4);
    bss     PT_LOAD FLAGS(6);
}

SECTIONS {
    . = 0x0;
    .text     ALIGN(0x10000) : { *(.text*) }    : text
    .rodata   ALIGN(0x10000) : { *(.rodata*) }  : rodata
    .data     ALIGN(0x10000) : { *(.data*) }    : data
    .rela.dyn ALIGN(0x10000) : { *(.rela.dyn) } : reladyn
    .bss      ALIGN(0x10000) : { *(.bss*) }     : bss
}
