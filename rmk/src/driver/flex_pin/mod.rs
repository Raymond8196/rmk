use embedded_hal::digital::{ErrorType, InputPin, OutputPin};

// Enabled when embassy-nrf is available (BLE or Gazell chip-specific features pull it in)
#[cfg(any(feature = "_nrf_ble", feature = "wireless_gazell_nrf52840", feature = "wireless_gazell_nrf52833", feature = "wireless_gazell_nrf52832"))]
pub mod nrf;
#[cfg(feature = "rp2040")]
pub mod rp;

/// Pin that can be switched between input and output.
pub trait FlexPin: ErrorType + InputPin + OutputPin {
    fn set_as_input(&mut self) -> ();

    fn set_as_output(&mut self) -> ();
}
