MEMORY
{
  /* nRF52840 with nice!nano UF2 bootloader (NO SoftDevice)
   *
   * INFO_UF2.TXT shows "SoftDevice: not found", so the app starts
   * right after the MBR at 0x1000.
   */
  FLASH : ORIGIN = 0x00001000, LENGTH = 972K
  RAM : ORIGIN = 0x20000008, LENGTH = 255K
}
