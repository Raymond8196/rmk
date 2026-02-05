//! ELink protocol split driver for RMK
//!
//! This module provides split keyboard communication using ELink protocol.
//! It can be enabled via the `elink` feature flag.
//!
//! # Usage
//!
//! Enable the `elink` feature in your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! rmk = { path = "...", features = ["elink"] }
//! ```
//!
//! When the `elink` feature is disabled, this module is completely excluded from compilation,
//! saving firmware size and allowing comparison of firmware sizes with/without ELink.

use embedded_io_async::{Read, Write};

use super::driver::SplitDriverError;
use crate::split::driver::{PeripheralManager, SplitReader, SplitWriter};
use crate::split::{SPLIT_MESSAGE_MAX_SIZE, SplitMessage};

/// Run ELink-based peripheral manager
///
/// This function creates an ELink split driver and runs the peripheral manager.
/// It's similar to `run_serial_peripheral_manager` but uses ELink protocol instead.
///
/// # Arguments
/// * `id` - Peripheral ID
/// * `receiver` - Transport (serial, BLE, etc.) that implements `Read + Write`
/// * `device_class` - 4-bit device class (0x1 = Keyboard, etc.)
/// * `device_address` - 8-bit device address (0-255)
/// * `sub_type` - 4-bit sub-type (0x0 = Central, 0x1 = Peripheral, etc.)
pub(crate) async fn run_elink_peripheral_manager<
    const ROW: usize,
    const COL: usize,
    const ROW_OFFSET: usize,
    const COL_OFFSET: usize,
    S: Read + Write,
>(
    id: usize,
    receiver: S,
    device_class: u8,
    device_address: u8,
    sub_type: u8,
) {
    let split_elink_driver = ElinkSplitDriver::new(receiver, device_class, device_address, sub_type);
    let peripheral_manager = PeripheralManager::<ROW, COL, ROW_OFFSET, COL_OFFSET, _>::new(split_elink_driver, id);
    info!("Running ELink peripheral manager {}", id);

    peripheral_manager.run().await;
}

/// ELink-based split driver for RMK
///
/// This driver implements SplitReader and SplitWriter using ELink protocol.
/// It can be used as a drop-in replacement for SerialSplitDriver or BleSplitDriver.
pub(crate) struct ElinkSplitDriver<T: Read + Write> {
    /// Underlying transport (serial, BLE, etc.)
    transport: T,
    /// ELink adapter for encoding/decoding messages
    adapter: elink_rmk_adapter::ElinkAdapter,
    /// Buffer for serialized SplitMessage
    message_buffer: heapless::Vec<u8, SPLIT_MESSAGE_MAX_SIZE>,
}

impl<T: Read + Write> ElinkSplitDriver<T> {
    /// Create a new ELink split driver
    ///
    /// # Arguments
    /// * `transport` - The underlying transport (serial port, BLE, etc.)
    /// * `device_class` - 4-bit device class (0x1 = Keyboard, etc.)
    /// * `device_address` - 8-bit device address (0-255)
    /// * `sub_type` - 4-bit sub-type (0x0 = Central, 0x1 = Peripheral, etc.)
    pub(crate) fn new(transport: T, device_class: u8, device_address: u8, sub_type: u8) -> Self {
        Self {
            transport,
            adapter: elink_rmk_adapter::ElinkAdapter::new(device_class, device_address, sub_type),
            message_buffer: heapless::Vec::new(),
        }
    }
}

impl<T: Read + Write> SplitReader for ElinkSplitDriver<T> {
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
        // Read bytes from transport and feed to adapter
        let mut temp_buffer = [0u8; 256];

        loop {
            // Try to read available data from transport
            match self.transport.read(&mut temp_buffer).await {
                Ok(bytes_read) => {
                    if bytes_read == 0 {
                        // No data available, but check if adapter has a complete message
                        // from previous reads
                        match self.adapter.process_incoming_bytes(&[]) {
                            Ok(Some(message_bytes)) => match postcard::from_bytes::<SplitMessage>(message_bytes) {
                                Ok(message) => return Ok(message),
                                Err(e) => {
                                    error!("Postcard deserialize error: {}", e);
                                    return Err(SplitDriverError::DeserializeError);
                                }
                            },
                            Ok(None) | Err(_) => {
                                // No message available, wait for more data
                                return Err(SplitDriverError::EmptyMessage);
                            }
                        }
                    }
                    // Feed bytes to adapter
                    match self.adapter.process_incoming_bytes(&temp_buffer[..bytes_read]) {
                        Ok(Some(message_bytes)) => {
                            // Successfully decoded a message, deserialize it
                            match postcard::from_bytes::<SplitMessage>(message_bytes) {
                                Ok(message) => return Ok(message),
                                Err(e) => {
                                    error!("Postcard deserialize split message error: {}", e);
                                    return Err(SplitDriverError::DeserializeError);
                                }
                            }
                        }
                        Ok(None) => {
                            // No complete message yet, continue reading
                            continue;
                        }
                        Err(e) => {
                            match e {
                                elink_rmk_adapter::Error::BufferTooSmall => {
                                    error!("ELink buffer too small");
                                    return Err(SplitDriverError::SerializeError);
                                }
                                elink_rmk_adapter::Error::InvalidCrc => {
                                    error!("ELink CRC error");
                                    // Continue reading, try to recover
                                    continue;
                                }
                                elink_rmk_adapter::Error::InvalidFrame => {
                                    error!("ELink invalid frame");
                                    // Continue reading, try to recover
                                    continue;
                                }
                                elink_rmk_adapter::Error::InvalidPriority => {
                                    error!("ELink invalid priority");
                                    continue;
                                }
                                elink_rmk_adapter::Error::InvalidData => {
                                    error!("ELink invalid data");
                                    continue;
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    return Err(SplitDriverError::SerialError);
                }
            }
        }
    }
}

impl<T: Read + Write> SplitWriter for ElinkSplitDriver<T> {
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
        // Serialize SplitMessage to bytes
        self.message_buffer.clear();
        let serialized = postcard::to_slice(message, &mut self.message_buffer).map_err(|e| {
            error!("Postcard serialize split message error: {}", e);
            SplitDriverError::SerializeError
        })?;

        // Encode message to ELink frame
        let frame_bytes = self.adapter.encode_message(serialized).map_err(|_e| {
            error!("ELink encode error");
            SplitDriverError::SerializeError
        })?;

        // Write frame bytes to transport
        let mut remaining_bytes = frame_bytes.len();
        while remaining_bytes > 0 {
            let sent_bytes = self
                .transport
                .write(&frame_bytes[frame_bytes.len() - remaining_bytes..])
                .await
                .map_err(|_e| SplitDriverError::SerialError)?;
            remaining_bytes -= sent_bytes;
        }

        Ok(frame_bytes.len())
    }
}
