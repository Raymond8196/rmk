#[cfg(feature = "_ble")]
use core::cell::RefCell;

#[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))]
use embedded_io_async::{Read, Write};
#[cfg(feature = "_ble")]
use {
    bt_hci::cmd::le::{LeReadLocalSupportedFeatures, LeSetPhy, LeSetScanParams},
    bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync},
    heapless::VecView,
    trouble_host::prelude::*,
};

/// Run central's peripheral manager task over BLE.
#[cfg(feature = "_ble")]
pub async fn run_peripheral_manager_ble<
    'a,
    const ROW: usize,
    const COL: usize,
    const ROW_OFFSET: usize,
    const COL_OFFSET: usize,
    C: Controller
        + ControllerCmdSync<LeSetScanParams>
        + ControllerCmdAsync<LeSetPhy>
        + ControllerCmdSync<LeReadLocalSupportedFeatures>,
>(
    id: usize,
    addr: &RefCell<VecView<Option<[u8; 6]>>>,
    stack: &'a Stack<'a, C, DefaultPacketPool>,
) {
    use crate::split::ble::central::run_ble_peripheral_manager;
    run_ble_peripheral_manager::<C, ROW, COL, ROW_OFFSET, COL_OFFSET>(id, addr, stack).await;
}

/// Run central's peripheral manager task over serial.
#[cfg(not(any(feature = "_ble", feature = "wireless_gazell")))]
pub async fn run_peripheral_manager_serial<
    const ROW: usize,
    const COL: usize,
    const ROW_OFFSET: usize,
    const COL_OFFSET: usize,
    S: Read + Write,
>(
    id: usize,
    receiver: S,
) {
    use crate::split::serial::run_serial_peripheral_manager;
    run_serial_peripheral_manager::<ROW, COL, ROW_OFFSET, COL_OFFSET, S>(id, receiver).await;
}
