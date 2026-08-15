/* The memory map the emulator provides. Kept deliberately small: one page of
   code and read-only data, one page of inputs, one page of outputs, and a stack
   below the top of the last page. */
ENTRY(_start)

SECTIONS {
  . = 0x80000000;
  .text : {
    *(.text._start)
    *(.text .text.*)
  }
  .rodata : { *(.rodata .rodata.*) }
  .data   : { *(.data .data.*) *(.sdata .sdata.*) }
  .bss    : { *(.bss .bss.*) *(.sbss .sbss.*) *(COMMON) }

  /* Inputs and outputs are written by the harness, not by the image. */
  _stack_top = 0x8001FF00;

  /DISCARD/ : { *(.eh_frame) *(.comment) }
}
