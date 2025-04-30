ENTRY(flame)

SECTIONS {
    KERNEL_BASE = 0x0;
    . = KERNEL_BASE;

    .text     ALIGN(0x1000) : { *(.text*) }
    .rodata   ALIGN(0x1000) : { *(.rodata*) }
    .data     ALIGN(0x1000) : { *(.data*) }
    .rela.dyn ALIGN(8)      : { *(.rela.dyn) }
    .got      ALIGN(0x1000) : { *(.got*) }
    .bss      ALIGN(0x1000) : { *(.bss*) }

    . = ALIGN(0x1000);
}