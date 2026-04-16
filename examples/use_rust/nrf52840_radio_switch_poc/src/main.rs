#![no_std]
#![no_main]

//! Phase 4.3 Milestone 2: BLE <-> Gazell switching with full MPSL/SDC reinit
//!
//! Commands (type in serial terminal, 115200 baud):
//!   g - Switch to / ensure Gazell mode
//!   b - Switch to BLE mode (advertise 30s, auto-return to Gazell)
//!   s - Print current mode
//!   ? - Print help
//!
//! Architecture: Each BLE session fully reinitializes MPSL + SDC + trouble-host,
//! because MPSL's internal scheduler gets corrupted when RADIO interrupts are
//! diverted to Gazell while TIMER0/RTC0 continue firing into MPSL.
//!
//! gz_deinit() cleans up RADIO + TIMER2 hardware registers internally.

use core::future::poll_fn;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use core::task::Poll;

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_nrf::interrupt::{self, InterruptExt, typelevel};
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embassy_nrf::{bind_interrupts, rng, usb};
use embassy_sync::waitqueue::AtomicWaker;
use embassy_time::Timer;
use nrf_mpsl::raw as mpsl_raw;
use nrf_sdc::{self as sdc, mpsl};
use panic_halt as _;
use static_cell::StaticCell;
use trouble_host::prelude::*;

// ─── Dynamic RADIO interrupt dispatch ─────────────────────────────────────

static RADIO_MODE: AtomicU8 = AtomicU8::new(0); // 0=idle, 1=Gazell, 2=BLE
static EGU0_MODE: AtomicU8 = AtomicU8::new(0);
static MPSL_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Waker for our MPSL low-priority processing task
static MPSL_LP_WAKER: AtomicWaker = AtomicWaker::new();

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
                // Wake our mpsl low-priority processing task
                MPSL_LP_WAKER.wake();
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

/// Dummy binding struct to satisfy MPSL compile-time constraints.
/// We handle actual ISR dispatch manually above.
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
    interrupt::TIMER2.disable();
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    interrupt::TIMER2.unpend();
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

// ─── BLE deinit (raw C API) ──────────────────────────────────────────────

/// Teardown BLE stack: disable SDC + uninit MPSL.
/// Must be called before switching to Gazell.
///
/// SAFETY: Caller must ensure no BLE operations are in flight.
unsafe fn ble_deinit_raw() {
    MPSL_INITIALIZED.store(false, Ordering::SeqCst);

    // Disable all MPSL-managed interrupts to prevent ISR firing into uninitialized code
    interrupt::RADIO.disable();
    interrupt::EGU0_SWI0.disable();
    interrupt::TIMER0.disable();
    interrupt::RTC0.disable();
    interrupt::RADIO.unpend();
    interrupt::EGU0_SWI0.unpend();
    interrupt::TIMER0.unpend();
    interrupt::RTC0.unpend();

    unsafe {
        nrf_sdc::raw::sdc_disable();
        mpsl_raw::mpsl_uninit();
    }

    RADIO_MODE.store(0, Ordering::SeqCst);
    EGU0_MODE.store(0, Ordering::SeqCst);
}

// ─── Tasks ────────────────────────────────────────────────────────────────

type UsbDriver = usb::Driver<'static, &'static SoftwareVbusDetect>;
type Cdc = embassy_usb::class::cdc_acm::CdcAcmClass<'static, UsbDriver>;

async fn cdc_print(cdc: &mut Cdc, s: &str) {
    let _ = cdc.write_packet(s.as_bytes()).await;
}

#[embassy_executor::task]
async fn usb_task(mut device: embassy_usb::UsbDevice<'static, UsbDriver>) {
    device.run().await
}

fn ble_addr() -> [u8; 6] {
    let ficr = embassy_nrf::pac::FICR;
    let high = u64::from(ficr.deviceid(1).read());
    let addr = (high << 32 | u64::from(ficr.deviceid(0).read())) | 0x0000_c000_0000_0000;
    addr.to_le_bytes()[..6].try_into().unwrap()
}

// ─── Static buffers for BLE stack (reused across init cycles) ─────────────

// These are `static mut` because they need to be reused across multiple BLE
// init/deinit cycles. StaticCell only allows one-shot initialization.
// SAFETY: Only accessed from the main task (single-threaded access).
static mut SDC_MEM_BUF: MaybeUninit<sdc::Mem<8192>> = MaybeUninit::uninit();
static mut HOST_RES_BUF: MaybeUninit<HostResources<DefaultPacketPool, 1, 4>> = MaybeUninit::uninit();
static mut SESSION_MEM_BUF: MaybeUninit<mpsl::SessionMem<1>> = MaybeUninit::uninit();

/// Get mutable pointer to static buffer, write value, return &'static mut reference.
/// SAFETY: Must only be called from single-threaded context (main task).
macro_rules! static_buf_init {
    ($static:ident, $val:expr) => {{
        let ptr = core::ptr::addr_of_mut!($static);
        (*ptr).write($val)
    }};
}

// ─── MPSL low-priority processing ─────────────────────────────────────────

/// Process MPSL low-priority work. Must be polled concurrently during BLE sessions.
/// Woken by EGU0_SWI0 ISR via MPSL_LP_WAKER.
async fn mpsl_low_prio_process() -> ! {
    poll_fn(|ctx| {
        MPSL_LP_WAKER.register(ctx.waker());
        if MPSL_INITIALIZED.load(Ordering::Relaxed) {
            unsafe { mpsl_raw::mpsl_low_priority_process() };
        }
        Poll::Pending
    })
    .await
}

// ─── Main ─────────────────────────────────────────────────────────────────

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_nrf::config::Config::default();
    config.dcdc.reg0_voltage = Some(embassy_nrf::config::Reg0Voltage::_3V3);
    config.dcdc.reg0 = true;
    config.dcdc.reg1 = true;
    let p = embassy_nrf::init(config);

    // ── USB CDC setup (bidirectional) ──
    static VBUS: StaticCell<SoftwareVbusDetect> = StaticCell::new();
    let vbus = VBUS.init(SoftwareVbusDetect::new(true, true));
    let driver = usb::Driver::new(p.USBD, UsbIrqs, &*vbus);

    static CDC_STATE: StaticCell<embassy_usb::class::cdc_acm::State> = StaticCell::new();
    let cdc_state = CDC_STATE.init(embassy_usb::class::cdc_acm::State::new());

    let usb_config = embassy_usb::Config::new(0xc0de, 0xcafe);
    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

    let mut builder = embassy_usb::Builder::new(
        driver,
        usb_config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; 64]),
    );
    let mut cdc = embassy_usb::class::cdc_acm::CdcAcmClass::new(&mut builder, cdc_state, 64);
    let usb_device = builder.build();
    spawner.spawn(usb_task(usb_device).expect("usb_task"));

    // ── Start HFCLK ──
    let clock = embassy_nrf::pac::CLOCK;
    clock.tasks_hfclkstart().write_value(1);
    while clock.events_hfclkstarted().read() != 1 {}

    // ── RNG (shared, initialized once) ──
    // We store the raw pointer because we need to pass &mut Rng to SDC Builder::build
    // multiple times across BLE init cycles. The mutable reference is exclusive at each
    // call site (sequential, not concurrent).
    static RNG_INST: StaticCell<rng::Rng<'static, embassy_nrf::mode::Async>> = StaticCell::new();
    let rng_ptr: *mut rng::Rng<'static, embassy_nrf::mode::Async> =
        RNG_INST.init(rng::Rng::new(p.RNG, UsbIrqs)) as *mut _;

    // ── MPSL init (first time through Rust wrapper to validate, then mem::forget) ──
    let mpsl_p = mpsl::Peripherals::new(
        p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31,
    );
    let lfclk_cfg = mpsl_raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl_raw::MPSL_CLOCK_LF_SRC_RC as u8,
        rc_ctiv: mpsl_raw::MPSL_RECOMMENDED_RC_CTIV as u8,
        rc_temp_ctiv: mpsl_raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
        accuracy_ppm: mpsl_raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
        skip_wait_lfclk_started: mpsl_raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
    };

    // Save PPI peripherals for SDC (consumed by Builder::build, so we need to handle this)
    // We pass them to the first BLE init. For subsequent inits, the raw C API
    // doesn't need Rust peripheral tokens (it manages hardware directly).
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23,
        p.PPI_CH24, p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );

    // ── First BLE init: validate stack works, then teardown ──
    switch_to_ble();

    // Init MPSL via Rust wrapper
    let session_mem = unsafe { static_buf_init!(SESSION_MEM_BUF, mpsl::SessionMem::new()) };
    let mpsl_inst = mpsl::MultiprotocolServiceLayer::with_timeslots(
        mpsl_p, MpslIrqs, lfclk_cfg, session_mem,
    )
    .unwrap();
    MPSL_INITIALIZED.store(true, Ordering::SeqCst);

    // Init SDC via Builder (this also sets up SDC_RNG internally)
    let sdc_mem = unsafe { static_buf_init!(SDC_MEM_BUF, sdc::Mem::new()) };
    let sdc_ctrl = {
        let b = Result::unwrap(sdc::Builder::new());
        let b = b.support_adv().support_peripheral();
        let b: sdc::Builder = Result::unwrap(b.peripheral_count(1));
        let b: sdc::Builder = Result::unwrap(b.buffer_cfg(251, 251, 3, 3));
        Result::unwrap(b.build(sdc_p, unsafe { &mut *rng_ptr }, &mpsl_inst, sdc_mem))
    };

    // Build trouble-host stack to validate it works
    {
        let host_res = unsafe { static_buf_init!(HOST_RES_BUF, HostResources::new()) };
        let stack = trouble_host::new(sdc_ctrl, host_res)
            .set_random_address(Address::random(ble_addr()));

        // Validation: build Host to confirm stack is functional
        let _host = stack.build();
        // _host, stack, sdc_ctrl all drop here (sdc_disable + clear SDC_RNG)
    }

    // Forget MPSL to prevent Drop from calling mpsl_uninit()
    // (we'll call it manually via raw C API)
    core::mem::forget(mpsl_inst);

    // Now teardown MPSL via raw C API
    MPSL_INITIALIZED.store(false, Ordering::SeqCst);
    interrupt::TIMER0.disable();
    interrupt::RTC0.disable();
    unsafe { mpsl_raw::mpsl_uninit() };

    // ── Boot into Gazell mode (default) ──
    switch_to_gazell();
    let gz_ret = unsafe { rmk_gazell_sys::gz_init_default(1) };

    // Wait for USB enumeration
    Timer::after_secs(2).await;

    // ── Print banner ──
    cdc_print(&mut cdc, "\r\n=== RMK Radio Switch PoC v2 ===\r\n").await;
    cdc_print(&mut cdc, "g=Gazell b=BLE(30s) s=status ?=help\r\n").await;
    if gz_ret == 0 {
        cdc_print(&mut cdc, "Boot: Gazell OK (BLE validated)\r\n\r\n").await;
    } else {
        cdc_print(&mut cdc, "Boot: Gazell FAIL!\r\n\r\n").await;
    }

    // ── Prepare adv data (reusable across cycles) ──
    let mut adv_buf = [0u8; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(b"RMK-PoC"),
        ],
        &mut adv_buf,
    )
    .unwrap_or(0);

    // ── Command loop (sequential state machine) ──
    let mut mode: u8 = 1; // 0=idle, 1=gazell, 2=ble
    let mut buf = [0u8; 64];

    loop {
        // Read CDC command (non-blocking, continues on error)
        let n = match cdc.read_packet(&mut buf).await {
            Ok(n) => n,
            Err(_) => {
                Timer::after_millis(100).await;
                continue;
            }
        };

        for i in 0..n {
            match buf[i] {
                b'b' => {
                    if mode == 2 {
                        cdc_print(&mut cdc, "Already in BLE\r\n").await;
                        continue;
                    }
                    cdc_print(&mut cdc, "-> BLE (full reinit)...\r\n").await;

                    // ── Deinit Gazell ──
                    if mode == 1 {
                        unsafe { rmk_gazell_sys::gz_deinit() };
                        Timer::after_millis(200).await;
                    }

                    // ── Init BLE: MPSL + SDC + trouble-host (full reinit) ──
                    switch_to_ble();
                    Timer::after_millis(50).await;

                    // MPSL init via raw C API
                    // SWI0_EGU0_IRQn = 20 on nRF52840
                    let mpsl_ret = unsafe {
                        mpsl_raw::mpsl_init(&lfclk_cfg, 20, Some(mpsl_assert_handler))
                    };
                    if mpsl_ret != 0 {
                        cdc_print(&mut cdc, "MPSL init failed!\r\n").await;
                        // Fall back to Gazell
                        switch_to_gazell();
                        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                        mode = if ret == 0 { 1 } else { 0 };
                        continue;
                    }

                    // Set up timeslot session
                    let session_mem = unsafe { static_buf_init!(SESSION_MEM_BUF, mpsl::SessionMem::new()) };
                    let ts_ret = unsafe {
                        mpsl_raw::mpsl_timeslot_session_count_set(
                            session_mem as *mut _ as *mut _,
                            1,
                        )
                    };
                    if ts_ret != 0 {
                        cdc_print(&mut cdc, "MPSL timeslot cfg failed!\r\n").await;
                        unsafe { mpsl_raw::mpsl_uninit() };
                        switch_to_gazell();
                        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                        mode = if ret == 0 { 1 } else { 0 };
                        continue;
                    }

                    // Set interrupt priorities (MPSL expects these)
                    interrupt::RADIO.set_priority(interrupt::Priority::P0);
                    interrupt::RTC0.set_priority(interrupt::Priority::P0);
                    interrupt::TIMER0.set_priority(interrupt::Priority::P0);
                    interrupt::EGU0_SWI0.set_priority(interrupt::Priority::P4);

                    // Enable TIMER0 and RTC0 (MPSL needs them)
                    unsafe {
                        interrupt::TIMER0.enable();
                        interrupt::RTC0.enable();
                    }

                    MPSL_INITIALIZED.store(true, Ordering::SeqCst);

                    // SDC init via Rust Builder (handles sdc_init + sdc_cfg_set + sdc_enable)
                    // Reinitialize the Mem buffer (sdc_enable expects fresh memory)
                    let sdc_mem = unsafe { static_buf_init!(SDC_MEM_BUF, sdc::Mem::new()) };

                    // SAFETY: rng_inst was initialized at boot and persists.
                    // We create a fake MPSL reference since Builder::build only uses it
                    // for ownership proof (let _ = (p, mpsl)), not actual access.
                    // Similarly, SDC peripherals are only consumed for ownership —
                    // the raw C library manages PPI channels directly.
                    let fake_mpsl: &mpsl::MultiprotocolServiceLayer =
                        unsafe { &*core::ptr::NonNull::dangling().as_ptr() };
                    let fake_sdc_p: sdc::Peripherals = unsafe { core::mem::zeroed() };

                    let sdc_result: Result<sdc::SoftdeviceController, nrf_mpsl::Error> = (|| {
                        let b = sdc::Builder::new()?;
                        let b = b.support_adv().support_peripheral();
                        let b: sdc::Builder = b.peripheral_count(1)?;
                        let b: sdc::Builder = b.buffer_cfg(251, 251, 3, 3)?;
                        b.build(fake_sdc_p, unsafe { &mut *rng_ptr }, fake_mpsl, sdc_mem)
                    })();

                    let sdc_ctrl = match sdc_result {
                        Ok(ctrl) => ctrl,
                        Err(_) => {
                            cdc_print(&mut cdc, "SDC init failed!\r\n").await;
                            unsafe { ble_deinit_raw() };
                            switch_to_gazell();
                            let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                            mode = if ret == 0 { 1 } else { 0 };
                            continue;
                        }
                    };

                    // Build trouble-host stack
                    let host_res = unsafe { static_buf_init!(HOST_RES_BUF, HostResources::new()) };
                    let stack = trouble_host::new(sdc_ctrl, host_res)
                        .set_random_address(Address::random(ble_addr()));

                    let Host {
                        mut peripheral,
                        mut runner,
                        ..
                    } = stack.build();

                    mode = 2;
                    cdc_print(&mut cdc, "BLE stack ready\r\n").await;

                    // ── Run BLE session ──
                    // Three concurrent tasks:
                    // 1. MPSL low-priority processing (woken by EGU0_SWI0)
                    // 2. trouble-host runner (HCI processing)
                    // 3. Advertising session with timeout

                    let ble_session_result = embassy_futures::select::select3(
                        // MPSL low-priority processing
                        mpsl_low_prio_process(),
                        // Runner (processes HCI commands/events)
                        async {
                            loop {
                                if let Err(_) = runner.run().await {
                                    Timer::after_millis(50).await;
                                }
                            }
                        },
                        // Advertising session with 30s timeout
                        async {
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
                                    cdc_print(&mut cdc, "BLE advertising (30s)\r\n").await;
                                    cdc_print(&mut cdc, "Check nRF Connect for RMK-PoC\r\n")
                                        .await;

                                    match select(advertiser.accept(), Timer::after_secs(30)).await {
                                        Either::First(Ok(_conn)) => {
                                            cdc_print(&mut cdc, "BLE connected!\r\n").await;
                                            Timer::after_secs(5).await;
                                        }
                                        Either::First(Err(_)) => {
                                            cdc_print(&mut cdc, "BLE accept err\r\n").await;
                                        }
                                        Either::Second(_) => {
                                            cdc_print(&mut cdc, "BLE timeout\r\n").await;
                                        }
                                    }
                                }
                                Err(_) => {
                                    cdc_print(&mut cdc, "BLE adv failed\r\n").await;
                                }
                            }
                        },
                    )
                    .await;
                    // BLE session complete (advertising timed out or finished)

                    // ── Teardown BLE, return to Gazell ──
                    // Drop stack/sdc_ctrl (on stack, dropped automatically when select3 completes)
                    // Note: sdc_ctrl was moved into stack, which is dropped here.
                    // SoftdeviceController::Drop calls sdc_disable() and clears SDC_RNG.

                    // Wait for drops to settle, then uninit MPSL
                    drop(ble_session_result);
                    Timer::after_millis(50).await;

                    // MPSL uninit via raw C API (SoftdeviceController already dropped → sdc_disable called)
                    MPSL_INITIALIZED.store(false, Ordering::SeqCst);
                    interrupt::TIMER0.disable();
                    interrupt::RTC0.disable();
                    unsafe { mpsl_raw::mpsl_uninit() };

                    // Return to Gazell
                    cdc_print(&mut cdc, "-> Gazell...\r\n").await;
                    Timer::after_millis(200).await;
                    switch_to_gazell();
                    let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                    mode = if ret == 0 { 1 } else { 0 };
                    cdc_print(
                        &mut cdc,
                        if ret == 0 {
                            "Gazell OK\r\n"
                        } else {
                            "Gazell FAIL!\r\n"
                        },
                    )
                    .await;
                }
                b'g' => {
                    if mode == 1 {
                        cdc_print(&mut cdc, "Already Gazell\r\n").await;
                    } else {
                        cdc_print(&mut cdc, "-> Gazell...\r\n").await;
                        if mode == 2 {
                            // BLE is active — shouldn't reach here since BLE session
                            // blocks the main loop, but handle gracefully
                            unsafe { ble_deinit_raw() };
                        }
                        Timer::after_millis(200).await;
                        switch_to_gazell();
                        let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                        mode = if ret == 0 { 1 } else { 0 };
                        cdc_print(
                            &mut cdc,
                            if ret == 0 {
                                "Gazell OK\r\n"
                            } else {
                                "Gazell FAIL!\r\n"
                            },
                        )
                        .await;
                    }
                }
                b's' => {
                    let label = match mode {
                        0 => "Idle",
                        1 => "Gazell",
                        2 => "BLE",
                        _ => "???",
                    };
                    cdc_print(&mut cdc, "Mode: ").await;
                    cdc_print(&mut cdc, label).await;
                    cdc_print(&mut cdc, "\r\n").await;
                }
                b'?' => {
                    cdc_print(&mut cdc, "g=Gazell b=BLE(30s) s=status ?=help\r\n").await;
                }
                _ => {} // ignore \r \n etc
            }
        }
    }
}

// ─── MPSL assert handler (for raw C API reinit) ──────────────────────────

unsafe extern "C" fn mpsl_assert_handler(
    file: *const core::ffi::c_char,
    line: u32,
) {
    let _ = (file, line);
    panic!("MPSL assert");
}
