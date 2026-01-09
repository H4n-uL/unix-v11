ENTRY(ignite)

PHDRS {
    text    PT_LOAD    FLAGS(5);
    rodata  PT_LOAD    FLAGS(4);
    data    PT_LOAD    FLAGS(6);
    dynamic PT_DYNAMIC FLAGS(6);
    reladyn PT_LOAD    FLAGS(4);
    bss     PT_LOAD    FLAGS(6);
}

SECTIONS {
    KERNEL_BASE = 0x0;
    . = KERNEL_BASE;

    .text     ALIGN(0x1000) : { *(.text*) }   : text
    .rodata   ALIGN(0x1000) : { *(.rodata*) } : rodata

    .dynsym   ALIGN(0x1000) : { *(.dynsym) }  : rodata
    .dynstr   ALIGN(0x1000) : { *(.dynstr) }  : rodata

    .data     ALIGN(0x1000) : { *(.data*) }   : data

    .dynamic  ALIGN(0x1000) : { *(.dynamic) } : data  : dynamic
    .got      ALIGN(0x1000) : { *(.got) *(.got.plt) } : data

    .rela.dyn ALIGN(0x1000) : {
        *(.rela.dyn)
        *(.rela.plt)
        *(.rela*)
    } : reladyn

    .bss      ALIGN(0x1000) : { *(.bss*) }    : bss

    . = ALIGN(0x1000);
}
