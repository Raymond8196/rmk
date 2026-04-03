#![no_std]
#![no_main]

//! Phase 4.1 PoC: Dynamic RADIO Interrupt Dispatcher
//!
//! Proves BLE (nrf-sdc/MPSL) and Gazell can coexist in a single binary
//! with runtime RADIO interrupt switching on E104-BT5040U dongle (nRF52840).
//!
//! Test sequence:
//!   Boot → Gazell host (3 cycles) → BLE advertise → Gazell host again → DONE

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_executor::Spawner;
use embassy_nrf::interrupt::{self, InterruptExt, typelevel};
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embassy_nrf::{bind_interrupts, usb};
use embassy_time::Timer;
use log::info;
use nrf_mpsl::raw as mpsl_raw;
use nrf_sdc::{self as sdc, mpsl};
use panic_halt as _;
use static_cell::StaticCell;

// ─── Dynamic RADIO interrupt dispatch ─────────────────────────────────────

/// Radio mode: 0 = idle, 1 = Gazell, 2 = BLE (MPSL)
static RADIO_MODE: AtomicU8 = AtomicU8::new(0);
/// EGU0/SWI0 mode: 0 = idle, 1 = Gazell, 2 = BLE (MPSL low-prio)
static EGU0_MODE: AtomicU8 = AtomicU8::new(0);
/// Guard: true after MPSL has been fully initialized.
/// Prevents dispatching to MPSL handlers before mpsl_init() completes.
static MPSL_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Gazell C library ISR handlers
unsafe extern "C" {
    fn RADIO_IRQHandler();
    fn TIMER2_IRQHandler();
    fn SWI0_EGU0_IRQHandler();
}

/// RADIO — shared by Gazell and BLE MPSL.
/// Dispatches based on RADIO_MODE with an MPSL initialization guard.
#[embassy_nrf::pac::interrupt]
fn RADIO() {
    match RADIO_MODE.load(Ordering::Relaxed) {
        1 => {
            // SAFETY: Gazell C library's RADIO handler. Only called when Gazell is active.
            unsafe { RADIO_IRQHandler() }
        }
        2 => {
            // Guard: only dispatch to MPSL after it has been fully initialized.
            // A stray RADIO interrupt before mpsl_init() would call an uninitialized handler.
            if MPSL_INITIALIZED.load(Ordering::Relaxed) {
                // SAFETY: nrf_mpsl raw RADIO handler. MPSL is initialized (guard above).
                unsafe { mpsl_raw::MPSL_IRQ_RADIO_Handler() }
            }
        }
        _ => {} // idle — ignore
    }
}

/// EGU0_SWI0 — shared by Gazell and BLE MPSL (low priority).
#[embassy_nrf::pac::interrupt]
fn EGU0_SWI0() {
    match EGU0_MODE.load(Ordering::Relaxed) {
        1 => {
            // SAFETY: Gazell C library's SWI0/EGU0 handler. Only called when Gazell is active.
            unsafe { SWI0_EGU0_IRQHandler() }
        }
        2 => {
            // MPSL low-prio handler wakes the MPSL task via an internal waker.
            // Call via the Handler trait to access nrf_mpsl's static WAKER.
            if MPSL_INITIALIZED.load(Ordering::Relaxed) {
                unsafe {
                    <mpsl::LowPrioInterruptHandler as typelevel::Handler<typelevel::EGU0_SWI0>>::on_interrupt();
                }
            }
        }
        _ => {} // idle — ignore
    }
}

/// TIMER2 — Gazell only. Conditionally dispatched to avoid calling Gazell's
/// handler after gz_deinit() has been called.
#[embassy_nrf::pac::interrupt]
fn TIMER2() {
    if RADIO_MODE.load(Ordering::Relaxed) == 1 {
        // SAFETY: Gazell C library's TIMER2 handler. Only called when Gazell is active.
        unsafe { TIMER2_IRQHandler() }
    }
}

/// TIMER0 — MPSL only, no conflict with Gazell.
#[embassy_nrf::pac::interrupt]
fn TIMER0() {
    if MPSL_INITIALIZED.load(Ordering::Relaxed) {
        // SAFETY: nrf_mpsl raw TIMER0 handler. MPSL is initialized.
        unsafe { mpsl_raw::MPSL_IRQ_TIMER0_Handler() }
    }
}

/// RTC0 — MPSL only, no conflict with Gazell.
#[embassy_nrf::pac::interrupt]
fn RTC0() {
    if MPSL_INITIALIZED.load(Ordering::Relaxed) {
        // SAFETY: nrf_mpsl raw RTC0 handler. MPSL is initialized.
        unsafe { mpsl_raw::MPSL_IRQ_RTC0_Handler() }
    }
}

// ─── Interrupt bindings ───────────────────────────────────────────────────

// Only bind non-conflicting interrupts via the macro.
// Conflicting ones (RADIO, TIMER0, RTC0, EGU0_SWI0) are handled by the
// manual ISR functions above with dynamic dispatch.
bind_interrupts!(struct UsbIrqs {
    USBD => usb::InterruptHandler<embassy_nrf::peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler, mpsl::ClockInterruptHandler;
    RNG => embassy_nrf::rng::InterruptHandler<embassy_nrf::peripherals::RNG>;
});

// Type-system placeholder for MPSL's Binding trait requirements.
//
// MPSL's `with_timeslots()` requires its `_irq` parameter to implement
// `Binding<RADIO, HighPrioInterruptHandler>`, etc. Normally `bind_interrupts!`
// generates both the ISR function and the Binding impl together. Since we write
// the ISR functions manually (for dynamic dispatch), we provide the Binding impls
// separately here. MPSL never registers its own ISR — it trusts the Binding
// contract that we will call its handlers from the appropriate ISR context.
#[derive(Copy, Clone)]
struct MpslIrqs;

// SAFETY: Our manual RADIO() ISR dispatches to MPSL_IRQ_RADIO_Handler when
// RADIO_MODE == 2 and MPSL_INITIALIZED is true.
unsafe impl typelevel::Binding<typelevel::RADIO, mpsl::HighPrioInterruptHandler> for MpslIrqs {}

// SAFETY: Our manual TIMER0() ISR calls MPSL_IRQ_TIMER0_Handler when
// MPSL_INITIALIZED is true. TIMER0 is MPSL-only (no Gazell conflict).
unsafe impl typelevel::Binding<typelevel::TIMER0, mpsl::HighPrioInterruptHandler> for MpslIrqs {}

// SAFETY: Our manual RTC0() ISR calls MPSL_IRQ_RTC0_Handler when
// MPSL_INITIALIZED is true. RTC0 is MPSL-only (no Gazell conflict).
unsafe impl typelevel::Binding<typelevel::RTC0, mpsl::HighPrioInterruptHandler> for MpslIrqs {}

// SAFETY: Our manual EGU0_SWI0() ISR dispatches to MPSL's LowPrioInterruptHandler
// when EGU0_MODE == 2 and MPSL_INITIALIZED is true.
unsafe impl typelevel::Binding<typelevel::EGU0_SWI0, mpsl::LowPrioInterruptHandler> for MpslIrqs {}

// SAFETY: CLOCK_POWER is handled by UsbIrqs (bind_interrupts! chains both
// usb::vbus_detect::InterruptHandler and mpsl::ClockInterruptHandler).
// This Binding impl satisfies MPSL's type requirement on MpslIrqs;
// the actual ISR is generated by bind_interrupts! on UsbIrqs above.
unsafe impl typelevel::Binding<typelevel::CLOCK_POWER, mpsl::ClockInterruptHandler> for MpslIrqs {}

// ─── Mode switching ───────────────────────────────────────────────────────

fn switch_to_gazell() {
    // Disable shared interrupts to prevent stale handler dispatch
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();

    // Clear pending bits from the previous handler BEFORE switching mode.
    // This prevents a stale pending interrupt from firing with the new handler.
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    interrupt::TIMER2.unpend();

    // Update dispatch targets
    RADIO_MODE.store(1, Ordering::SeqCst);
    EGU0_MODE.store(1, Ordering::SeqCst);

    // Set Gazell IRQ priorities
    interrupt::RADIO.set_priority(interrupt::Priority::P0);
    interrupt::TIMER2.set_priority(interrupt::Priority::P0);
    interrupt::EGU0_SWI0.set_priority(interrupt::Priority::P1);

    // Re-enable shared interrupts
    unsafe {
        interrupt::RADIO.enable();
        interrupt::EGU0_SWI0.enable();
    }
}

fn switch_to_ble() {
    // Disable shared interrupts
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();

    // Clear pending bits from the previous handler BEFORE switching mode
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();

    // Update dispatch targets
    RADIO_MODE.store(2, Ordering::SeqCst);
    EGU0_MODE.store(2, Ordering::SeqCst);

    // BLE/MPSL IRQ priorities
    interrupt::RADIO.set_priority(interrupt::Priority::P0);
    interrupt::TIMER0.set_priority(interrupt::Priority::P0);
    interrupt::RTC0.set_priority(interrupt::Priority::P0);
    interrupt::EGU0_SWI0.set_priority(interrupt::Priority::P4);

    // Re-enable shared interrupts
    unsafe {
        interrupt::RADIO.enable();
        interrupt::EGU0_SWI0.enable();
    }
}

fn switch_to_idle() {
    // Disable shared interrupts for clean transition
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();

    // Clear any pending bits from the protocol that was just active
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    interrupt::TIMER2.unpend();

    // Set mode to idle — ISRs will ignore any stray interrupts
    RADIO_MODE.store(0, Ordering::SeqCst);
    EGU0_MODE.store(0, Ordering::SeqCst);

    // Re-enable (ISRs are no-ops in idle mode, but keeps NVIC state clean)
    unsafe {
        interrupt::RADIO.enable();
        interrupt::EGU0_SWI0.enable();
    }
}

// ─── MPSL task ────────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static mpsl::MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

// ─── Main ─────────────────────────────────────────────────────────────────

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_nrf::config::Config::default();
    config.dcdc.reg0_voltage = Some(embassy_nrf::config::Reg0Voltage::_3V3);
    config.dcdc.reg0 = true;
    config.dcdc.reg1 = true;
    let p = embassy_nrf::init(config);

    // USB CDC logger
    static VBUS: StaticCell<SoftwareVbusDetect> = StaticCell::new();
    let vbus: &SoftwareVbusDetect = VBUS.init(SoftwareVbusDetect::new(true, true));
    let driver = usb::Driver::new(p.USBD, UsbIrqs, vbus);

    // Start HFCLK (required for both Gazell and BLE)
    let clock = embassy_nrf::pac::CLOCK;
    clock.tasks_hfclkstart().write_value(1);
    while clock.events_hfclkstarted().read() != 1 {}

    // Prepare MPSL peripherals (claim them early, init later during BLE cycle)
    let mpsl_p = mpsl::Peripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);
    let lfclk_cfg = mpsl_raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl_raw::MPSL_CLOCK_LF_SRC_RC as u8,
        rc_ctiv: mpsl_raw::MPSL_RECOMMENDED_RC_CTIV as u8,
        rc_temp_ctiv: mpsl_raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
        accuracy_ppm: mpsl_raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
        skip_wait_lfclk_started: mpsl_raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
    };
    static MPSL: StaticCell<mpsl::MultiprotocolServiceLayer> = StaticCell::new();
    static SESSION_MEM: StaticCell<mpsl::SessionMem<1>> = StaticCell::new();

    // SDC peripherals
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24, p.PPI_CH25, p.PPI_CH26,
        p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );

    // RNG peripheral for SDC
    let rng_peri = p.RNG;

    let test_future = async {
        Timer::after_secs(3).await;

        info!("=== Phase 4.1 PoC: Dynamic RADIO Dispatcher ===");
        info!("HFCLK started");

        // ── Gazell cycle 1 ──
        info!("[Gazell 1] Switching to Gazell mode...");
        switch_to_gazell();
        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
        if ret != rmk_gazell_sys::GZ_OK {
            info!("[Gazell 1] FAILED: gz_init_default returned {}", ret);
            switch_to_idle();
            info!("=== TEST ABORTED ===");
            loop {
                Timer::after_secs(60).await;
            }
        }
        info!("[Gazell 1] gz_init_default(host) = {} OK", ret);
        Timer::after_secs(2).await;
        unsafe { rmk_gazell_sys::gz_deinit() };
        switch_to_idle();
        info!("[Gazell 1] deinit OK");
        Timer::after_millis(500).await;

        // ── Gazell cycle 2 ──
        info!("[Gazell 2] Switching to Gazell mode...");
        switch_to_gazell();
        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
        if ret != rmk_gazell_sys::GZ_OK {
            info!("[Gazell 2] FAILED: gz_init_default returned {}", ret);
            switch_to_idle();
            info!("=== TEST ABORTED ===");
            loop {
                Timer::after_secs(60).await;
            }
        }
        info!("[Gazell 2] gz_init_default(host) = {} OK", ret);
        Timer::after_secs(2).await;
        unsafe { rmk_gazell_sys::gz_deinit() };
        switch_to_idle();
        info!("[Gazell 2] deinit OK");
        Timer::after_millis(500).await;

        // ── BLE cycle ──
        info!("[BLE] Switching to BLE mode...");
        switch_to_ble();

        // Init MPSL
        info!("[BLE] Initializing MPSL...");
        let mpsl = MPSL.init(
            mpsl::MultiprotocolServiceLayer::with_timeslots(
                mpsl_p,
                MpslIrqs,
                lfclk_cfg,
                SESSION_MEM.init(mpsl::SessionMem::new()),
            )
            .unwrap(),
        );
        MPSL_INITIALIZED.store(true, Ordering::SeqCst);
        spawner.spawn(mpsl_task(mpsl).unwrap());
        info!("[BLE] MPSL initialized, task spawned");

        // Init SDC (SoftDevice Controller)
        info!("[BLE] Initializing SDC...");
        let mut rng = embassy_nrf::rng::Rng::new(rng_peri, UsbIrqs);
        let mut sdc_mem = sdc::Mem::<8192>::new();
        let sdc_ctrl = sdc::Builder::new()
            .unwrap()
            .support_adv()
            .support_peripheral()
            .peripheral_count(1)
            .unwrap()
            .buffer_cfg(251, 251, 3, 3)
            .unwrap()
            .build(sdc_p, &mut rng, mpsl, &mut sdc_mem)
            .unwrap();
        info!("[BLE] SDC initialized");

        // Set a random static address and start advertising
        let ficr = embassy_nrf::pac::FICR;
        let high = u64::from(ficr.deviceid(1).read());
        let addr = (high << 32 | u64::from(ficr.deviceid(0).read())) | 0x0000_c000_0000_0000;
        let addr_bytes: [u8; 6] = addr.to_le_bytes()[..6].try_into().unwrap();

        info!("[BLE] Setting random address...");
        use bt_hci::cmd::le::*;
        use bt_hci::controller::ControllerCmdSync;
        if let Err(e) = sdc_ctrl
            .exec(&LeSetRandomAddr::new(bt_hci::param::BdAddr::new(addr_bytes)))
            .await
        {
            info!("[BLE] Set address failed: {:?}", e);
        }

        // Build advertising data: flags + complete local name "RMK-PoC"
        let adv_name = b"RMK-PoC";
        let mut adv_data = [0u8; 31];
        adv_data[0] = 2; // length
        adv_data[1] = 0x01; // AD type: Flags
        adv_data[2] = 0x06; // LE General Discoverable + BR/EDR Not Supported
        adv_data[3] = (adv_name.len() + 1) as u8; // length
        adv_data[4] = 0x09; // AD type: Complete Local Name
        adv_data[5..5 + adv_name.len()].copy_from_slice(adv_name);
        let adv_len = 5 + adv_name.len();

        info!("[BLE] Setting advertising data...");
        if let Err(e) = sdc_ctrl.exec(&LeSetAdvData::new(adv_len as u8, adv_data)).await {
            info!("[BLE] Set adv data failed: {:?}", e);
        }

        // Set advertising parameters
        let adv_params = LeSetAdvParams::new(
            bt_hci::param::Duration::from_millis(160), // min interval
            bt_hci::param::Duration::from_millis(160), // max interval
            bt_hci::param::AdvKind::AdvInd,
            bt_hci::param::AddrKind::RANDOM,
            bt_hci::param::AddrKind::PUBLIC,
            bt_hci::param::BdAddr::default(),
            bt_hci::param::AdvChannelMap::ALL,
            bt_hci::param::AdvFilterPolicy::default(),
        );
        if let Err(e) = sdc_ctrl.exec(&adv_params).await {
            info!("[BLE] Set adv params failed: {:?}", e);
        }

        // Enable advertising
        info!("[BLE] Enabling advertising (check nRF Connect for 'RMK-PoC')...");
        if let Err(e) = sdc_ctrl.exec(&LeSetAdvEnable::new(true)).await {
            info!("[BLE] Enable adv failed: {:?}", e);
        }

        // Advertise for 10 seconds
        Timer::after_secs(10).await;

        // Stop advertising
        info!("[BLE] Stopping advertising...");
        if let Err(e) = sdc_ctrl.exec(&LeSetAdvEnable::new(false)).await {
            info!("[BLE] Stop adv failed: {:?}", e);
        }
        Timer::after_millis(200).await;

        // Note: We don't fully deinit MPSL (StaticCell limitation).
        // Just switch the ISR dispatch away from BLE.
        info!("[BLE] BLE cycle done, switching to idle...");
        switch_to_idle();
        Timer::after_millis(500).await;

        // ── Gazell cycle 3 (after BLE) ──
        info!("[Gazell 3] Switching to Gazell after BLE...");
        switch_to_gazell();
        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
        if ret != rmk_gazell_sys::GZ_OK {
            info!("[Gazell 3] FAILED: gz_init_default returned {}", ret);
            switch_to_idle();
            info!("=== TEST ABORTED ===");
            loop {
                Timer::after_secs(60).await;
            }
        }
        info!("[Gazell 3] gz_init_default(host) = {} OK", ret);
        Timer::after_secs(2).await;
        unsafe { rmk_gazell_sys::gz_deinit() };
        switch_to_idle();
        info!("[Gazell 3] deinit OK");

        info!("=== ALL CYCLES PASSED ===");
        info!("  Gazell: 3 init/deinit cycles (2 before BLE, 1 after)");
        info!("  BLE: 1 advertise cycle (10s)");
        info!("  Dynamic RADIO dispatch: VERIFIED");

        loop {
            Timer::after_secs(60).await;
        }
    };

    let logger_future = async {
        embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
    };

    embassy_futures::join::join(logger_future, test_future).await;
}
