//! Gazell 2.4GHz split keyboard driver
//!
//! Implements `SplitReader` and `SplitWriter` for Nordic Gazell protocol.
//!
//! - `GazellPeripheralDriver`: keyboard half (device mode), sends key events via `gz_send`,
//!   receives central commands via ACK payloads.
//! - `GazellCentralHub`: single async task that owns `gz_recv()`, dispatches to per-pipe channels.
//! - `PipeDriver`: channel-based `SplitReader + SplitWriter` for each pipe (no FFI code).

use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};

use super::driver::{PeripheralManager, SplitDriverError, SplitReader, SplitWriter};
use super::peripheral::SplitPeripheral;
use super::{SPLIT_MESSAGE_MAX_SIZE, SplitMessage};
use crate::RawMutex;
use crate::wireless::config::GazellConfig;

#[cfg(feature = "wireless_gazell")]
use rmk_gazell_sys as sys;

/// Gazell max payload size (from Nordic SDK: NRF_GZLL_CONST_MAX_PAYLOAD_LENGTH)
const GAZELL_MAX_PAYLOAD: usize = 32;

/// Heartbeat marker — a 2-byte internal packet that is NOT a SplitMessage.
/// Used to keep the Gazell link alive and trigger ACK payload delivery.
const HEARTBEAT_MARKER: [u8; 2] = [0xFE, 0xFE];

/// Maximum retry count for gz_send / gz_set_ack_payload on BUSY
const MAX_SEND_RETRIES: u8 = 3;

// Compile-time assertion: SplitMessage must fit in Gazell payload
// Use core::assert! explicitly because defmt overrides assert! with non-const functions
const _: () = core::assert!(
    SPLIT_MESSAGE_MAX_SIZE <= GAZELL_MAX_PAYLOAD,
    "SplitMessage too large for Gazell 32-byte payload"
);

/// Check if a received packet is a heartbeat marker
fn is_heartbeat(buf: &[u8], len: u8) -> bool {
    len == 2 && buf[0] == HEARTBEAT_MARKER[0] && buf[1] == HEARTBEAT_MARKER[1]
}

// ---------------------------------------------------------------------------
// Peripheral Driver (keyboard half, device mode)
// ---------------------------------------------------------------------------

/// Split driver for Gazell peripheral (keyboard half, device mode).
///
/// - `write()`: serializes `SplitMessage` and sends via `gz_send()`, then checks
///   for ACK payload from central.
/// - `read()`: returns buffered ACK payload, or sends heartbeat to solicit one.
pub(crate) struct GazellPeripheralDriver {
    pipe: u8,
    heartbeat_interval_ms: u16,
    idle_heartbeat_interval_ms: u16,
    idle_timeout_ms: u16,
    deep_idle_timeout_secs: u16,
    startup_grace_ms: u16,
    idle: bool,
    radio_off: bool,
    ack_buffer: Option<SplitMessage>,
    last_send_time: Instant,
    last_activity: Instant,
    startup_time: Instant,
}

impl GazellPeripheralDriver {
    pub(crate) fn new(config: &GazellConfig) -> Self {
        let now = Instant::now();
        Self {
            pipe: config.pipe,
            heartbeat_interval_ms: config.heartbeat_interval_ms,
            idle_heartbeat_interval_ms: config.idle_heartbeat_interval_ms,
            idle_timeout_ms: config.idle_timeout_ms,
            deep_idle_timeout_secs: config.deep_idle_timeout_secs,
            startup_grace_ms: config.startup_grace_ms,
            idle: false,
            radio_off: false,
            ack_buffer: None,
            last_send_time: now,
            last_activity: now,
            startup_time: now,
        }
    }

    /// Shut down the Gazell radio to save power.
    #[cfg(feature = "wireless_gazell")]
    fn radio_shutdown(&mut self) {
        info!("Gazell: radio shutdown (deep idle)");
        unsafe { sys::gz_deinit() };
        self.radio_off = true;
    }

    #[cfg(not(feature = "wireless_gazell"))]
    fn radio_shutdown(&mut self) {
        self.radio_off = true;
    }

    /// Re-initialize the Gazell radio after shutdown.
    #[cfg(feature = "wireless_gazell")]
    fn radio_reinit(&mut self) {
        info!("Gazell: radio reinit (activity detected)");
        let ret = unsafe { sys::gz_init_default(sys::GZ_MODE_DEVICE) };
        if ret != sys::GZ_OK {
            error!("Gazell: reinit failed: {}", ret);
            return;
        }
        self.radio_off = false;
        self.idle = false;
        self.last_send_time = Instant::now();
    }

    #[cfg(not(feature = "wireless_gazell"))]
    fn radio_reinit(&mut self) {
        self.radio_off = false;
        self.idle = false;
    }

    /// After a successful gz_send, check for piggybacked ACK payload from central.
    #[cfg(feature = "wireless_gazell")]
    fn check_ack_payload(&mut self) {
        let mut buf = [0u8; GAZELL_MAX_PAYLOAD];
        let mut len: u8 = 0;
        // SAFETY: buf and len are valid writable memory, max_len bounds the write.
        let ret = unsafe { sys::gz_get_ack_payload(buf.as_mut_ptr(), &mut len, buf.len() as u8) };
        if ret == sys::GZ_OK && len > 0 {
            match postcard::from_bytes::<SplitMessage>(&buf[..len as usize]) {
                Ok(msg) => {
                    self.ack_buffer = Some(msg);
                }
                Err(e) => {
                    warn!("Gazell: ACK payload deserialize error: {}", e);
                }
            }
        }
    }

    #[cfg(not(feature = "wireless_gazell"))]
    fn check_ack_payload(&mut self) {
        // Mock: no ACK payload on host
    }

    /// Send raw bytes via Gazell (non-blocking enqueue + async poll).
    ///
    /// Enqueues the packet into the Gazell TX FIFO via `gz_send_start()`,
    /// then polls `gz_poll_tx_status()` with async yields between checks.
    /// This avoids blocking the Embassy executor during TX (~6ms worst case).
    ///
    /// Poll timing: 500µs intervals match the typical Gazell timeslot period
    /// (600µs at 2Mbps default). Most successful TXs complete in 1-2 polls.
    #[cfg(feature = "wireless_gazell")]
    async fn raw_send(&self, data: &[u8]) -> Result<(), SplitDriverError> {
        // Enqueue phase: retry on BUSY (FIFO full or previous TX in flight)
        for attempt in 0..MAX_SEND_RETRIES {
            // SAFETY: data points to valid memory of data.len() bytes.
            let ret = unsafe { sys::gz_send_start(data.as_ptr(), data.len() as u8, self.pipe) };
            if ret == sys::GZ_OK {
                break;
            }
            if ret == sys::GZ_ERR_BUSY && attempt < MAX_SEND_RETRIES - 1 {
                Timer::after_millis(1).await;
                continue;
            }
            return Err(SplitDriverError::SerialError);
        }

        // Poll phase: check TX completion with async yields.
        // 500µs interval ≈ 1 Gazell timeslot; 40 iterations = 20ms timeout.
        for _ in 0..40 {
            // SAFETY: gz_poll_tx_status() reads volatile flags, no pointer args.
            let status = unsafe { sys::gz_poll_tx_status() };
            match status {
                sys::GZ_TX_SUCCESS => return Ok(()),
                sys::GZ_TX_FAILED => return Err(SplitDriverError::SerialError),
                _ => Timer::after_micros(500).await, // Yield to executor
            }
        }
        // Timeout: flush to reset tx_pending and clear stale FIFO state,
        // so the next gz_send_start() is not permanently blocked.
        let _ = unsafe { sys::gz_flush() };
        Err(SplitDriverError::SerialError)
    }

    #[cfg(not(feature = "wireless_gazell"))]
    async fn raw_send(&self, _data: &[u8]) -> Result<(), SplitDriverError> {
        // Mock: always succeed
        Ok(())
    }
}

impl SplitWriter for GazellPeripheralDriver {
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        // Re-init radio if it was shut down for deep idle
        if self.radio_off {
            self.radio_reinit();
        }

        let mut buf = [0u8; SPLIT_MESSAGE_MAX_SIZE];
        let bytes = postcard::to_slice(message, &mut buf).map_err(|e| {
            error!("Gazell: serialize error: {}", e);
            SplitDriverError::SerializeError
        })?;
        let len = bytes.len();

        self.raw_send(bytes).await?;

        let now = Instant::now();
        self.last_send_time = now;
        self.last_activity = now;
        if self.idle {
            info!("Gazell: waking from idle");
            self.idle = false;
        }
        self.check_ack_payload();

        Ok(len)
    }
}

impl SplitReader for GazellPeripheralDriver {
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
        loop {
            // 0. Check cancel signal
            if GAZELL_PERIPHERAL_CANCEL.signaled() {
                return Err(SplitDriverError::Disconnected);
            }

            // 1. Radio is off (deep idle) — block until write() reinits.
            // Uses pending() for zero wakeups / maximum power savings.
            // Cancel during deep idle is handled at the outer level
            // (run_gazell_split_peripheral) by directly calling gz_deinit().
            if self.radio_off {
                core::future::pending::<()>().await;
            }

            // 2. Return buffered ACK payload if available
            if let Some(msg) = self.ack_buffer.take() {
                return Ok(msg);
            }

            // 3. Skip heartbeat during startup grace period
            let in_grace_period = self.startup_time.elapsed().as_millis() < self.startup_grace_ms as u64;

            // 4. Check idle transition
            if !self.idle
                && self.idle_timeout_ms > 0
                && self.last_activity.elapsed().as_millis() >= self.idle_timeout_ms as u64
            {
                info!("Gazell: entering idle mode ({}ms inactive)", self.idle_timeout_ms);
                self.idle = true;
            }

            // 5. Check deep idle transition — shut down radio entirely
            if self.idle
                && self.deep_idle_timeout_secs > 0
                && self.last_activity.elapsed().as_secs() >= self.deep_idle_timeout_secs as u64
            {
                self.radio_shutdown();
                continue;
            }

            // 6. Adaptive heartbeat: fast when active, slow when idle
            if !in_grace_period {
                let interval = if self.idle {
                    self.idle_heartbeat_interval_ms
                } else {
                    self.heartbeat_interval_ms
                };
                let elapsed = self.last_send_time.elapsed().as_millis() as u16;
                if interval > 0 && elapsed >= interval {
                    let _ = self.raw_send(&HEARTBEAT_MARKER).await;
                    self.last_send_time = Instant::now();
                    self.check_ack_payload();

                    if let Some(msg) = self.ack_buffer.take() {
                        return Ok(msg);
                    }
                }
            }

            // 7. Yield — longer sleep when idle (Embassy WFI during Timer)
            // Cancel is checked at the top of each loop iteration (step 0)
            Timer::after_millis(if self.idle { 50 } else { 5 }).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Central: Hub + PipeDriver (dongle, host mode)
// ---------------------------------------------------------------------------

/// Maximum number of Gazell pipes (Nordic hardware limit).
pub(crate) const MAX_GAZELL_PIPES: usize = 8;

const PIPE_RX_CAPACITY: usize = 4;
const PIPE_TX_CAPACITY: usize = 2;

/// Per-pipe inbound channel: hub dispatches received SplitMessages here.
/// `None` is a poison pill — signals that the hub has shut down.
static PIPE_RX: [Channel<RawMutex, Option<SplitMessage>, PIPE_RX_CAPACITY>; MAX_GAZELL_PIPES] = [
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
];

/// Per-pipe outbound channel: PipeDriver sends messages here, hub flushes via ACK payloads.
static PIPE_TX: [Channel<RawMutex, SplitMessage, PIPE_TX_CAPACITY>; MAX_GAZELL_PIPES] = [
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
    Channel::new(),
];

/// Flush outbound messages for all active pipes via ACK payloads.
#[cfg(feature = "wireless_gazell")]
fn flush_all_pipes(num_pipes: usize) {
    let mut ack_buf = [0u8; SPLIT_MESSAGE_MAX_SIZE];
    for (i, pipe_tx) in PIPE_TX.iter().enumerate().take(num_pipes) {
        if let Ok(msg) = pipe_tx.try_receive()
            && let Ok(bytes) = postcard::to_slice(&msg, &mut ack_buf)
        {
            let ack_len = bytes.len() as u8;
            // SAFETY: ack_buf is valid, ack_len <= SPLIT_MESSAGE_MAX_SIZE <= 32
            let _ = unsafe { sys::gz_set_ack_payload(i as u8, ack_buf.as_ptr(), ack_len) };
        }
    }
}

/// Static cancel signal for the central hub.
/// Signalling this causes `run_gazell_central_hub` to shut down gracefully.
pub static GAZELL_HUB_CANCEL: Signal<RawMutex, ()> = Signal::new();

/// Run the central Gazell hub: single task that owns `gz_recv()`, dispatches
/// received packets to per-pipe channels, and flushes outbound ACK payloads.
///
/// Returns when `GAZELL_HUB_CANCEL` is signalled, after calling `gz_deinit()`.
pub async fn run_gazell_central_hub(_config: GazellConfig, num_pipes: usize) {
    // NOTE: Caller (codegen) must call gz_init_default(1) + HFCLK + IRQ priorities
    // BEFORE spawning this task. Gazell init is NOT done here to avoid double-init.

    info!("Gazell central hub started (num_pipes={})", num_pipes);

    let mut buf = [0u8; GAZELL_MAX_PAYLOAD];

    loop {
        // Check cancel signal (non-blocking)
        if GAZELL_HUB_CANCEL.signaled() {
            info!("Gazell central hub: cancel signal received, shutting down");
            break;
        }

        #[cfg(feature = "wireless_gazell")]
        {
            let mut len: u8 = 0;
            let mut rx_pipe: u8 = 0;

            // SAFETY: buf, len, rx_pipe are valid writable memory
            let ret = unsafe { sys::gz_recv(buf.as_mut_ptr(), &mut len, &mut rx_pipe, buf.len() as u8) };

            if ret == sys::GZ_OK && len > 0 {
                // Flush outbound for all active pipes on every receive
                flush_all_pipes(num_pipes);

                let pipe_idx = rx_pipe as usize;
                if pipe_idx >= num_pipes {
                    warn!("Gazell: rx_pipe {} >= num_pipes {}", pipe_idx, num_pipes);
                    Timer::after_millis(1).await;
                    continue;
                }

                // Filter heartbeat
                if is_heartbeat(&buf, len) {
                    Timer::after_millis(1).await;
                    continue;
                }

                // Deserialize and dispatch to pipe channel
                match postcard::from_bytes::<SplitMessage>(&buf[..len as usize]) {
                    Ok(msg) => {
                        if PIPE_RX[pipe_idx].try_send(Some(msg)).is_err() {
                            warn!("Gazell: PIPE_RX[{}] full, dropping", pipe_idx);
                        }
                    }
                    Err(e) => {
                        warn!("Gazell: deserialize error on pipe {}: {}", pipe_idx, e);
                    }
                }
                Timer::after_millis(1).await;
                continue;
            }
        }

        #[cfg(not(feature = "wireless_gazell"))]
        {
            // Mock: yield forever
        }

        // No data: back off to reduce CPU wakeups
        Timer::after_millis(5).await;
    }

    // Send poison pill to all pipe managers so they exit cleanly
    for pipe_rx in PIPE_RX.iter().take(num_pipes) {
        let _ = pipe_rx.try_send(None);
    }

    // Cleanup: deinit Gazell radio
    #[cfg(feature = "wireless_gazell")]
    unsafe {
        sys::gz_deinit();
    }
    info!("Gazell central hub stopped");
}

/// Channel-based split driver for one Gazell pipe.
///
/// Contains NO Gazell-specific FFI code — fully reusable for any
/// multi-pipe radio protocol (ESB, ESP-NOW, etc.).
pub(crate) struct PipeDriver {
    pipe_index: usize,
    poisoned: bool,
}

impl PipeDriver {
    pub(crate) fn new(pipe_index: usize) -> Self {
        Self {
            pipe_index,
            poisoned: false,
        }
    }
}

impl SplitReader for PipeDriver {
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
        // Once poisoned, always return Disconnected (prevents infinite retry)
        if self.poisoned {
            // Block forever — prevents busy-spin in callers that retry on error.
            // `pending()` never resolves, so the task sleeps until dropped.
            return core::future::pending().await;
        }
        // Event-driven: awaits channel (zero wakeups until data arrives).
        // Hub sends None (poison pill) on shutdown to unblock all pipes.
        match PIPE_RX[self.pipe_index].receive().await {
            Some(msg) => Ok(msg),
            None => {
                self.poisoned = true;
                Err(SplitDriverError::Disconnected)
            }
        }
    }
}

impl SplitWriter for PipeDriver {
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        PIPE_TX[self.pipe_index].send(*message).await;
        Ok(SPLIT_MESSAGE_MAX_SIZE)
    }
}

// ---------------------------------------------------------------------------
// Entry points (called from split/central.rs and split/peripheral.rs)
// ---------------------------------------------------------------------------

/// Static cancel signal for the peripheral.
/// Signalling this causes `run_gazell_split_peripheral` to shut down gracefully.
pub static GAZELL_PERIPHERAL_CANCEL: Signal<RawMutex, ()> = Signal::new();

/// Initialize Gazell and run the split peripheral loop.
///
/// Returns when `GAZELL_PERIPHERAL_CANCEL` is signalled, after calling `gz_deinit()`.
///
/// IMPORTANT: Caller must ensure HFCLK is started before calling this function.
/// On nRF52840 without USB, HFCLK must be explicitly started:
/// ```ignore
/// embassy_nrf::pac::CLOCK.tasks_hfclkstart().write_value(1);
/// while embassy_nrf::pac::CLOCK.events_hfclkstarted().read() != 1 {}
/// ```
pub(crate) async fn run_gazell_split_peripheral(config: GazellConfig) {
    // NOTE: Caller (codegen or user) must call gz_init_default(0) + HFCLK + IRQ priorities
    // BEFORE this function. Gazell init is NOT done here to avoid double-init issues.

    info!("Gazell peripheral started (pipe={})", config.pipe);

    let driver = GazellPeripheralDriver::new(&config);
    let mut peripheral = SplitPeripheral::new(driver);

    // When GAZELL_PERIPHERAL_CANCEL is signalled, driver.read() returns
    // Disconnected, causing run() to break out. The outer loop retries on
    // transient errors but exits on Disconnected too.
    loop {
        peripheral.run().await;
        // run() only returns on Disconnected — check if it was a cancel
        if GAZELL_PERIPHERAL_CANCEL.signaled() {
            break;
        }
        // Otherwise it was a transient error; retry
        warn!("Gazell peripheral: run() exited, retrying...");
    }

    // Cleanup: deinit Gazell radio
    #[cfg(feature = "wireless_gazell")]
    unsafe {
        sys::gz_deinit();
    }
    info!("Gazell peripheral stopped");
}

/// Run the central-side peripheral manager for one Gazell pipe.
///
/// Creates a `PipeDriver` connected to the hub's static channels and wraps it
/// in a `PeripheralManager`. The hub must be running concurrently.
pub async fn run_gazell_pipe_manager<
    const ROW: usize,
    const COL: usize,
    const ROW_OFFSET: usize,
    const COL_OFFSET: usize,
>(
    id: usize,
    pipe_index: usize,
) {
    info!("Gazell pipe manager started (id={}, pipe={})", id, pipe_index);
    let driver = PipeDriver::new(pipe_index);
    let manager = PeripheralManager::<ROW, COL, ROW_OFFSET, COL_OFFSET, _>::new(driver, id);
    manager.run().await;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use postcard::experimental::max_size::MaxSize;

    #[test]
    fn split_message_fits_in_gazell_payload() {
        assert!(
            SPLIT_MESSAGE_MAX_SIZE <= GAZELL_MAX_PAYLOAD,
            "SplitMessage ({} bytes) exceeds Gazell {}-byte max payload",
            SPLIT_MESSAGE_MAX_SIZE,
            GAZELL_MAX_PAYLOAD,
        );
    }

    #[test]
    fn heartbeat_marker_does_not_collide_with_split_message() {
        // Verify no SplitMessage variant serializes to the heartbeat marker [0xFE, 0xFE]
        let mut buf = [0u8; SPLIT_MESSAGE_MAX_SIZE];

        // Test all no-payload and small-payload variants
        let test_messages: &[SplitMessage] = &[
            SplitMessage::ClearPeer,
            SplitMessage::LedState(false),
            SplitMessage::LedState(true),
            SplitMessage::ConnectionState(false),
            SplitMessage::ConnectionState(true),
            SplitMessage::KeyboardIndicator(0),
            SplitMessage::KeyboardIndicator(0xFF),
            SplitMessage::Layer(0),
            SplitMessage::Layer(0xFF),
        ];

        for msg in test_messages {
            let bytes = postcard::to_slice(msg, &mut buf).unwrap();
            assert!(
                !(bytes.len() == 2 && bytes[0] == 0xFE && bytes[1] == 0xFE),
                "SplitMessage {:?} serializes to heartbeat marker!",
                msg
            );
        }
    }

    #[test]
    fn postcard_max_size_is_expected() {
        // Guard against unexpected SplitMessage growth
        assert!(
            SplitMessage::POSTCARD_MAX_SIZE <= 28,
            "SplitMessage::POSTCARD_MAX_SIZE ({}) unexpectedly large — check new variants",
            SplitMessage::POSTCARD_MAX_SIZE,
        );
    }
}
