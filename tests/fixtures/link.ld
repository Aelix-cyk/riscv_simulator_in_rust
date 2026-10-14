/* Places the loadable image at DRAM_BASE with no leading gap, so the ELF can be
   handed to spike directly and objcopy emits exactly the bytes the hart executes. */
ENTRY(_start)

SECTIONS
{
  . = 0x80000000;
  .text : { *(.text) }
  .data : { *(.data) }
  .bss  : { *(.bss) }
}
