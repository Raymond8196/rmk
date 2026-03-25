#![no_main]
#![no_std]

use rmk::macros::rmk_central;

#[rmk_central]
mod keyboard_central {
    // E104-BT5040U dongle does not have a hardware VBUS detect pin.
    // Override to use SoftwareVbusDetect (force VBUS present).
    #[Overwritten(usb)]
    fn usb_init() {
        static VBUS: ::static_cell::StaticCell<
            ::embassy_nrf::usb::vbus_detect::SoftwareVbusDetect,
        > = ::static_cell::StaticCell::new();
        let vbus = VBUS.init(::embassy_nrf::usb::vbus_detect::SoftwareVbusDetect::new(
            true, true,
        ));
        ::embassy_nrf::usb::Driver::new(p.USBD, Irqs, &*vbus)
    }
}
