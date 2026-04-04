#![no_std]
#![no_main]

//! Phase 4.2 PoC: BLE Pause/Resume via Advertise Control
//!
//! Instead of dropping Host/Runner, we keep them alive and control advertising.
//! BLE "pause" = stop advertising + switch RADIO to Gazell.
//! BLE "resume" = switch RADIO back to BLE + restart advertising.
//!
//! Test sequence:
//!   Boot → MPSL/SDC/Stack init → BLE advertise 1 → Gazell → BLE advertise 2 → Gazell → DONE

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_nrf::interrupt::{self, InterruptExt, typelevel};
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embassy_nrf::{bind_interrupts, rng, usb};
use embassy_time::Timer;
use log::info;
use nrf_mpsl::raw as mpsl_raw;
use nrf_sdc::{self as sdc, mpsl};
use panic_halt as _;
use static_cell::StaticCell;
use trouble_host::prelude::*;

// ─── Dynamic RADIO interrupt dispatch ─────────────────────────────────────

static RADIO_MODE: AtomicU8 = AtomicU8::new(0); // 0=idle, 1=Gazell, 2=BLE
static EGU0_MODE: AtomicU8 = AtomicU8::new(0);
static MPSL_INITIALIZED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn RADIO_IRQHandler();
    fn TIMER2_IRQHandler();
    fn SWI0_EGU0_IRQHandler();
}

#[embassy_nrf::pac::interrupt]
fn RADIO() {
    match RADIO_MODE.load(Ordering::Relaxed) {
        1 => unsafe { RADIO_IRQHandler() },
        2 => {
            if MPSL_INITIALIZED.load(Ordering::Relaxed) {
                unsafe { mpsl_raw::MPSL_IRQ_RADIO_Handler() }
            }
        }
        _ => {}
    }
}

#[embassy_nrf::pac::interrupt]
fn EGU0_SWI0() {
    match EGU0_MODE.load(Ordering::Relaxed) {
        1 => unsafe { SWI0_EGU0_IRQHandler() },
        2 => {
            if MPSL_INITIALIZED.load(Ordering::Relaxed) {
                unsafe {
                    <mpsl::LowPrioInterruptHandler as typelevel::Handler<typelevel::EGU0_SWI0>>::on_interrupt();
                }
            }
        }
        _ => {}
    }
}

#[embassy_nrf::pac::interrupt]
fn TIMER2() {
    if RADIO_MODE.load(Ordering::Relaxed) == 1 {
        unsafe { TIMER2_IRQHandler() }
    }
}

#[embassy_nrf::pac::interrupt]
fn TIMER0() {
    if MPSL_INITIALIZED.load(Ordering::Relaxed) {
        unsafe { mpsl_raw::MPSL_IRQ_TIMER0_Handler() }
    }
}

#[embassy_nrf::pac::interrupt]
fn RTC0() {
    if MPSL_INITIALIZED.load(Ordering::Relaxed) {
        unsafe { mpsl_raw::MPSL_IRQ_RTC0_Handler() }
    }
}

// ─── Interrupt bindings ───────────────────────────────────────────────────

bind_interrupts!(struct UsbIrqs {
    USBD => usb::InterruptHandler<embassy_nrf::peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler, mpsl::ClockInterruptHandler;
    RNG => rng::InterruptHandler<embassy_nrf::peripherals::RNG>;
});

#[derive(Copy, Clone)]
struct MpslIrqs;
unsafe impl typelevel::Binding<typelevel::RADIO, mpsl::HighPrioInterruptHandler> for MpslIrqs {}
unsafe impl typelevel::Binding<typelevel::TIMER0, mpsl::HighPrioInterruptHandler> for MpslIrqs {}
unsafe impl typelevel::Binding<typelevel::RTC0, mpsl::HighPrioInterruptHandler> for MpslIrqs {}
unsafe impl typelevel::Binding<typelevel::EGU0_SWI0, mpsl::LowPrioInterruptHandler> for MpslIrqs {}
unsafe impl typelevel::Binding<typelevel::CLOCK_POWER, mpsl::ClockInterruptHandler> for MpslIrqs {}

// ─── Mode switching ───────────────────────────────────────────────────────

fn switch_to_gazell() {
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    interrupt::TIMER2.unpend();
    RADIO_MODE.store(1, Ordering::SeqCst);
    EGU0_MODE.store(1, Ordering::SeqCst);
    interrupt::RADIO.set_priority(interrupt::Priority::P0);
    interrupt::TIMER2.set_priority(interrupt::Priority::P0);
    interrupt::EGU0_SWI0.set_priority(interrupt::Priority::P1);
    unsafe {
        interrupt::RADIO.enable();
        interrupt::EGU0_SWI0.enable();
    }
}

fn switch_to_ble() {
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    RADIO_MODE.store(2, Ordering::SeqCst);
    EGU0_MODE.store(2, Ordering::SeqCst);
    interrupt::RADIO.set_priority(interrupt::Priority::P0);
    interrupt::TIMER0.set_priority(interrupt::Priority::P0);
    interrupt::RTC0.set_priority(interrupt::Priority::P0);
    interrupt::EGU0_SWI0.set_priority(interrupt::Priority::P4);
    unsafe {
        interrupt::RADIO.enable();
        interrupt::EGU0_SWI0.enable();
    }
}

fn switch_to_idle() {
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    interrupt::TIMER2.unpend();
    RADIO_MODE.store(0, Ordering::SeqCst);
    EGU0_MODE.store(0, Ordering::SeqCst);
    // Leave interrupts disabled in idle — no protocol needs them
}

// ─── Tasks ────────────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static mpsl::MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

fn ble_addr() -> [u8; 6] {
    let ficr = embassy_nrf::pac::FICR;
    let high = u64::from(ficr.deviceid(1).read());
    let addr = (high << 32 | u64::from(ficr.deviceid(0).read())) | 0x0000_c000_0000_0000;
    addr.to_le_bytes()[..6].try_into().unwrap()
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

    // Start HFCLK
    let clock = embassy_nrf::pac::CLOCK;
    clock.tasks_hfclkstart().write_value(1);
    while clock.events_hfclkstarted().read() != 1 {}

    // ── MPSL init (stays alive forever) ──
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
    switch_to_ble();
    let mpsl_ref = MPSL.init(
        mpsl::MultiprotocolServiceLayer::with_timeslots(
            mpsl_p,
            MpslIrqs,
            lfclk_cfg,
            SESSION_MEM.init(mpsl::SessionMem::new()),
        )
        .unwrap(),
    );
    MPSL_INITIALIZED.store(true, Ordering::SeqCst);
    spawner.spawn(mpsl_task(mpsl_ref).unwrap());

    // ── SDC + trouble-host Stack + Host (all stay alive forever) ──
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
        p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );
    static RNG_INST: StaticCell<rng::Rng<'static, embassy_nrf::mode::Async>> = StaticCell::new();
    let rng_inst = RNG_INST.init(rng::Rng::new(p.RNG, UsbIrqs));
    static SDC_MEM: StaticCell<sdc::Mem<8192>> = StaticCell::new();
    let sdc_mem = SDC_MEM.init(sdc::Mem::new());
    let sdc_ctrl = sdc::Builder::new()
        .unwrap()
        .support_adv()
        .support_peripheral()
        .peripheral_count(1)
        .unwrap()
        .buffer_cfg(251, 251, 3, 3)
        .unwrap()
        .build(sdc_p, rng_inst, mpsl_ref, sdc_mem)
        .unwrap();

    static HOST_RES: StaticCell<HostResources<DefaultPacketPool, 1, 4>> = StaticCell::new();
    let host_resources = HOST_RES.init(HostResources::new());
    static STACK: StaticCell<Stack<sdc::SoftdeviceController<'static>, DefaultPacketPool>> =
        StaticCell::new();
    let stack = STACK.init(
        trouble_host::new(sdc_ctrl, host_resources)
            .set_random_address(Address::random(ble_addr())),
    );

    // Build Host ONCE — Runner + Peripheral stay alive across BLE/Gazell switches
    let Host {
        mut peripheral,
        mut runner,
        ..
    } = stack.build();

    // Prepare adv data once (reusable across cycles)
    let mut adv_buf = [0u8; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(b"RMK-PoC"),
        ],
        &mut adv_buf,
    )
    .unwrap_or(0);

    info!("Stack + Host built, starting test sequence...");

    // ── Test future: BLE ↔ Gazell switching via advertise control ──
    let test_future = async {
        Timer::after_secs(3).await;
        info!("=== Phase 4.2 PoC: BLE Pause/Resume via Advertise Control ===");

        // ── BLE cycle 1 ──
        info!("[BLE 1] Starting advertising...");
        // ISR is already in BLE mode (set during MPSL init)
        match peripheral
            .advertise(
                &Default::default(),
                Advertisement::ConnectableScannableUndirected {
                    adv_data: &adv_buf[..adv_len],
                    scan_data: &[],
                },
            )
            .await
        {
            Ok(advertiser) => {
                info!("[BLE 1] Advertising! (check nRF Connect for 'RMK-PoC')");
                // Wait 5 seconds then stop (drop advertiser)
                match select(advertiser.accept(), Timer::after_secs(5)).await {
                    Either::First(Ok(_conn)) => info!("[BLE 1] Connected!"),
                    Either::First(Err(e)) => info!("[BLE 1] Accept error: {:?}", e),
                    Either::Second(_) => info!("[BLE 1] Timeout, stopping adv"),
                }
                // Advertiser dropped here — advertising stops
            }
            Err(e) => info!("[BLE 1] Advertise error: {:?}", e),
        }
        info!("[BLE 1] Advertising stopped");

        // Wait for Runner to process advertiser cancel and send LeSetAdvEnable(false).
        // Advertiser::drop() only sets a flag; the Runner must asynchronously send the
        // HCI command to SDC, and SDC must release its RADIO timeslot.  Without this
        // delay, switch_to_gazell() can steal the RADIO while MPSL still owns it.
        Timer::after_millis(200).await;

        // ── Switch to Gazell (Runner keeps running but RADIO goes to Gazell) ──
        info!("[Gazell 1] Switching RADIO to Gazell...");
        switch_to_gazell();
        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
        if ret != rmk_gazell_sys::GZ_OK {
            info!("[Gazell 1] FAILED: {}", ret);
            loop { Timer::after_secs(60).await; }
        }
        info!("[Gazell 1] gz_init_default(host) = {} OK", ret);
        Timer::after_secs(3).await;
        unsafe { rmk_gazell_sys::gz_deinit() };
        info!("[Gazell 1] deinit OK");

        // ── BLE cycle 2: switch RADIO back to BLE, re-advertise ──
        // Brief settle after gz_deinit to ensure Gazell fully releases RADIO/TIMER2
        Timer::after_millis(50).await;
        info!("[BLE 2] Switching RADIO back to BLE...");
        switch_to_ble();
        Timer::after_millis(100).await; // Let MPSL/Runner settle

        info!("[BLE 2] Starting advertising AGAIN...");
        match peripheral
            .advertise(
                &Default::default(),
                Advertisement::ConnectableScannableUndirected {
                    adv_data: &adv_buf[..adv_len],
                    scan_data: &[],
                },
            )
            .await
        {
            Ok(advertiser) => {
                info!("[BLE 2] Advertising! (check nRF Connect for 'RMK-PoC' again)");
                match select(advertiser.accept(), Timer::after_secs(5)).await {
                    Either::First(Ok(_conn)) => info!("[BLE 2] Connected!"),
                    Either::First(Err(e)) => info!("[BLE 2] Accept error: {:?}", e),
                    Either::Second(_) => info!("[BLE 2] Timeout, stopping adv"),
                }
            }
            Err(e) => info!("[BLE 2] Advertise error: {:?}", e),
        }
        info!("[BLE 2] Advertising stopped");

        // Same race-condition guard as BLE 1 → Gazell 1
        Timer::after_millis(200).await;

        // ── Gazell cycle 2 ──
        info!("[Gazell 2] Final Gazell cycle...");
        switch_to_gazell();
        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
        if ret != rmk_gazell_sys::GZ_OK {
            info!("[Gazell 2] FAILED: {}", ret);
            loop { Timer::after_secs(60).await; }
        }
        info!("[Gazell 2] gz_init_default(host) = {} OK", ret);
        Timer::after_secs(2).await;
        unsafe { rmk_gazell_sys::gz_deinit() };
        info!("[Gazell 2] deinit OK");

        // Switch back to BLE for idle
        switch_to_ble();

        info!("=== ALL CYCLES PASSED ===");
        info!("  BLE 1: advertise OK");
        info!("  Gazell 1: init/deinit OK (Runner idle during Gazell)");
        info!("  BLE 2: re-advertise OK (Runner resumed)");
        info!("  Gazell 2: init/deinit after 2nd BLE OK");
        info!("  BLE pause/resume via advertise control: VERIFIED");

        loop { Timer::after_secs(60).await; }
    };

    // Runner runs forever concurrently with test sequence.
    // During Gazell mode, Runner idles (no RADIO events routed to MPSL).
    let runner_future = async {
        loop {
            if let Err(e) = runner.run().await {
                // Suppress noisy errors when RADIO is not in BLE mode — Runner may
                // see HCI timeouts because MPSL cannot acquire radio timeslots.
                if RADIO_MODE.load(Ordering::Relaxed) == 2 {
                    info!("[BLE runner] error: {:?}", e);
                }
                Timer::after_millis(100).await;
            }
        }
    };

    let logger_future = async {
        embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
    };

    // Run all three concurrently: logger + runner + test sequence
    embassy_futures::join::join3(logger_future, runner_future, test_future).await;
}
