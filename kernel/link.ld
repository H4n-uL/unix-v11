ENTRY(ignite)

SECTIONS {
    . = 0x0;

    PROVIDE(_kernel_start = .);
    .text   ALIGN(0x1000) : AT(.) { *(.text*) }
    .rodata ALIGN(0x1000) : AT(.) { *(.rodata*) }
    .data   ALIGN(0x1000) : AT(.) { *(.data*) }
    .bss    ALIGN(0x1000) : AT(.) { *(.bss*) }

    . = ALIGN(0x1000);
    PROVIDE(_kernel_end = .);

    /DISCARD/ : {
        *(.comment)
        *(.dynamic)
        *(.eh_frame)
        *(.interp)
        *(.note*)
    }
}