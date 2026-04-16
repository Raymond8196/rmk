#![no_std]
#![no_main]

//! Phase 4.3 Milestone 1: Interactive BLE <-> Gazell switching via USB CDC
//!
//! Commands (type in serial terminal, 115200 baud):
//!   g - Switch to / ensure Gazell mode
//!   b - Switch to BLE mode (advertise 30s, auto-return to Gazell)
//!   s - Print current mode
//!   ? - Print help
//!
//! Default boot mode: Gazell
//!
//! gz_deinit() now cleans up RADIO + TIMER2 hardware registers internally.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_nrf::interrupt::{self, InterruptExt, typelevel};
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embassy_nrf::{bind_interrupts, rng, usb};
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
    // Disable all RADIO-related interrupts
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

// ─── Tasks ────────────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static mpsl::MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

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
        driver, usb_config,
        CONFIG_DESC.init([0; 256]), BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]), CONTROL_BUF.init([0; 64]),
    );
    let cdc = embassy_usb::class::cdc_acm::CdcAcmClass::new(&mut builder, cdc_state, 64);
    let usb_device = builder.build();
    spawner.spawn(usb_task(usb_device).unwrap());

    // ── Start HFCLK ──
    let clock = embassy_nrf::pac::CLOCK;
    clock.tasks_hfclkstart().write_value(1);
    while clock.events_hfclkstarted().read() != 1 {}

    // ── MPSL init (needs BLE ISR mode) ──
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
            mpsl_p, MpslIrqs, lfclk_cfg,
            SESSION_MEM.init(mpsl::SessionMem::new()),
        )
        .unwrap(),
    );
    MPSL_INITIALIZED.store(true, Ordering::SeqCst);
    spawner.spawn(mpsl_task(mpsl_ref).unwrap());

    // ── SDC + trouble-host Stack + Host ──
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24, p.PPI_CH25, p.PPI_CH26,
        p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
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
    static STACK: StaticCell<Stack<sdc::SoftdeviceController<'static>, DefaultPacketPool>> = StaticCell::new();
    let stack = STACK.init(trouble_host::new(sdc_ctrl, host_resources).set_random_address(Address::random(ble_addr())));

    let Host {
        mut peripheral,
        mut runner,
        ..
    } = stack.build();

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

    // ── Boot into Gazell mode (default) ──
    switch_to_gazell();
    let gz_ret = unsafe { rmk_gazell_sys::gz_init_default(1) };

    // Wait for USB enumeration
    Timer::after_secs(2).await;

    // ── Print banner ──
    let mut cdc = cdc;
    cdc_print(&mut cdc, "\r\n=== RMK Radio Switch PoC ===\r\n").await;
    cdc_print(&mut cdc, "g=Gazell b=BLE(30s) s=status ?=help\r\n").await;
    if gz_ret == 0 {
        cdc_print(&mut cdc, "Boot: Gazell OK\r\n\r\n").await;
    } else {
        cdc_print(&mut cdc, "Boot: Gazell FAIL!\r\n\r\n").await;
    }

    // ── Run: Runner + Command loop ──
    let mut mode: u8 = 1; // 0=idle, 1=gazell, 2=ble

    embassy_futures::join::join(
        // Runner runs forever (errors suppressed during Gazell mode)
        async {
            loop {
                if let Err(_) = runner.run().await {
                    Timer::after_millis(100).await;
                }
            }
        },
        // Command loop
        async {
            let mut buf = [0u8; 64];
            loop {
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
                            cdc_print(&mut cdc, "-> BLE...\r\n").await;

                            // Deinit Gazell (also cleans up RADIO/TIMER2 hardware)
                            unsafe { rmk_gazell_sys::gz_deinit() };
                            Timer::after_millis(200).await;
                            switch_to_ble();
                            Timer::after_millis(200).await;

                            // Start advertising
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
                                    mode = 2;
                                    cdc_print(&mut cdc, "BLE advertising (30s)\r\n").await;
                                    cdc_print(&mut cdc, "Check nRF Connect for RMK-PoC\r\n").await;

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
                                    // advertiser dropped -> stops advertising
                                }
                                Err(_) => {
                                    cdc_print(&mut cdc, "BLE adv failed\r\n").await;
                                }
                            }

                            // Auto-return to Gazell
                            cdc_print(&mut cdc, "-> Gazell...\r\n").await;
                            Timer::after_millis(200).await;
                            switch_to_gazell();
                            let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                            mode = if ret == 0 { 1 } else { 0 };
                            cdc_print(&mut cdc, if ret == 0 { "Gazell OK\r\n" } else { "Gazell FAIL!\r\n" }).await;
                        }
                        b'g' => {
                            if mode == 1 {
                                cdc_print(&mut cdc, "Already Gazell\r\n").await;
                            } else {
                                cdc_print(&mut cdc, "-> Gazell...\r\n").await;
                                Timer::after_millis(200).await;
                                switch_to_gazell();
                                let ret = unsafe { rmk_gazell_sys::gz_init_default(1) };
                                mode = if ret == 0 { 1 } else { 0 };
                                cdc_print(&mut cdc, if ret == 0 { "Gazell OK\r\n" } else { "Gazell FAIL!\r\n" }).await;
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
                            cdc_print(&mut cdc, "g=Gazell b=BLE s=status ?=help\r\n").await;
                        }
                        _ => {} // ignore \r \n etc
                    }
                }
            }
        },
    )
    .await;
}
