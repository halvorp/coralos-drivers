/* Ring-3 user program layout, linked at 4 MiB (the static ELF the loader maps PT_LOAD 1:1; no page both writable and
 * executable). Copied from CoralOS's device layout, which rt/src/lib.rs publishes as the `_start(role)` entry fact. */
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FILEHDR PHDRS FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x400000 + SIZEOF_HEADERS;

    .text   : { *(.text .text.*) } :text
    .rodata : { *(.rodata .rodata.*) } :rodata
    . = ALIGN(4096) + (. & 4095);
    .data   : { *(.data .data.*) } :data
    .got    : { *(.got .got.*) } :data
    .bss    : { *(.bss .bss.*) *(COMMON) } :data

    /DISCARD/ : { *(.eh_frame*) *(.comment) *(.note.*) }
}
