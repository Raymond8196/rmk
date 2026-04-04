#ifndef GAZELL_SHIM_H
#define GAZELL_SHIM_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Error codes (maps 1:1 to Rust WirelessError)
typedef enum {
    GZ_OK = 0,
    GZ_ERR_SEND_FAILED = -1,
    GZ_ERR_RECEIVE_FAILED = -2,
    GZ_ERR_FRAME_TOO_LARGE = -3,
    GZ_ERR_NOT_INITIALIZED = -4,
    GZ_ERR_BUSY = -5,
    GZ_ERR_INVALID_CONFIG = -6,
    GZ_ERR_HARDWARE = -7,
} gz_error_t;

// Configuration (matches GazellConfig in Rust)
typedef struct {
    uint8_t channel;              // RF channel: 0-100
    uint8_t data_rate;            // Data rate: 1=1Mbps, 2=2Mbps
    int8_t tx_power;              // TX power in dBm: -40, -20, -16, -12, -8, -4, 0, +3, +4
    uint8_t max_retries;          // Max TX retries: 0-15
    uint16_t ack_timeout_us;      // ACK timeout in microseconds: 250-4000
    uint8_t base_address[4];      // Base address (4 bytes)
    uint8_t address_prefix;       // Address prefix for pipe 0
} gz_config_t;

// Mode selection
typedef enum {
    GZ_MODE_DEVICE = 0,  // Transmitter mode (keyboard)
    GZ_MODE_HOST = 1,    // Receiver mode (dongle)
} gz_mode_t;

/**
 * @brief Initialize Gazell with the given configuration
 *
 * Must be called before any other gz_* functions.
 *
 * @param config Pointer to configuration structure
 * @return GZ_OK on success, error code otherwise
 */
gz_error_t gz_init(const gz_config_t* config);

/**
 * @brief Set Gazell operating mode
 *
 * @param mode GZ_MODE_DEVICE (transmitter) or GZ_MODE_HOST (receiver)
 * @return GZ_OK on success, error code otherwise
 */
gz_error_t gz_set_mode(gz_mode_t mode);

/**
 * @brief Send a frame (blocking with timeout)
 *
 * This function blocks until:
 * - ACK is received (success)
 * - Max retries exceeded (failure)
 * - Timeout occurs (failure)
 *
 * @param data Pointer to data buffer
 * @param len Length of data (max 32 bytes)
 * @param pipe Pipe number to send on (0-7)
 * @return GZ_OK on successful transmission and ACK, error code otherwise
 */
gz_error_t gz_send(const uint8_t* data, uint8_t len, uint8_t pipe);

/**
 * @brief Receive a frame (non-blocking, host mode)
 *
 * Checks if a frame is available and copies it to the output buffer.
 * Returns immediately if no data is available.
 *
 * @param out_buf Buffer to store received data
 * @param out_len Pointer to store actual received length (set to 0 if no data)
 * @param out_pipe Pointer to store the pipe number the data was received on
 * @param max_len Maximum buffer size
 * @return GZ_OK on success (even if no data), error code on failure
 */
gz_error_t gz_recv(uint8_t* out_buf, uint8_t* out_len, uint8_t* out_pipe, uint8_t max_len);

/**
 * @brief Set ACK payload for a pipe (host mode)
 *
 * Attaches data to the next ACK sent on the specified pipe.
 * Used for central→peripheral communication via Gazell ACK.
 *
 * @param pipe Pipe number (0-7)
 * @param data Pointer to data buffer
 * @param len Length of data (max 32 bytes)
 * @return GZ_OK on success, error code otherwise
 */
gz_error_t gz_set_ack_payload(uint8_t pipe, const uint8_t* data, uint8_t len);

/**
 * @brief Get ACK payload received after last successful send (device mode)
 *
 * After gz_send() succeeds, call this to check if the host attached
 * payload data to the ACK.
 *
 * @param out_buf Buffer to store ACK payload data
 * @param out_len Pointer to store actual received length (set to 0 if no data)
 * @param max_len Maximum buffer size
 * @return GZ_OK on success (even if no data), error code on failure
 */
gz_error_t gz_get_ack_payload(uint8_t* out_buf, uint8_t* out_len, uint8_t max_len);

/**
 * @brief Check if Gazell is ready to transmit on a given pipe
 *
 * @param pipe Pipe number (0-7)
 * @return true if TX FIFO has space, false otherwise
 */
bool gz_is_ready(uint8_t pipe);

/**
 * @brief Flush all TX and RX FIFOs
 *
 * @return GZ_OK on success, error code otherwise
 */
gz_error_t gz_flush(void);

/**
 * @brief Initialize Gazell with Nordic defaults (no custom config)
 *
 * Skips custom configuration and uses Nordic SDK defaults.
 * Useful for quick startup when default settings are sufficient.
 *
 * @param mode  GZ_MODE_DEVICE (0) or GZ_MODE_HOST (1)
 * @return GZ_OK on success, error code otherwise
 */
gz_error_t gz_init_default(gz_mode_t mode);

/**
 * @brief Deinitialize Gazell and disable radio
 *
 * Should be called before entering low-power modes.
 */
void gz_deinit(void);

// ---------------------------------------------------------------------------
// Non-blocking TX API (for async / RTOS integration)
// ---------------------------------------------------------------------------

/// TX completion status returned by gz_poll_tx_status()
typedef enum {
    GZ_TX_PENDING = 1,   ///< TX in progress, poll again later
    GZ_TX_SUCCESS = 0,   ///< TX completed, ACK received
    GZ_TX_FAILED = -1,   ///< TX failed (max retries exceeded or timeout)
} gz_tx_status_t;

/**
 * @brief Enqueue a frame for transmission (non-blocking)
 *
 * Validates parameters, adds the packet to the TX FIFO, and returns
 * immediately.  Call gz_poll_tx_status() to check completion.
 *
 * Only one TX operation can be in flight at a time.  Calling this
 * while a previous TX is still pending returns GZ_ERR_BUSY.
 *
 * @param data Pointer to data buffer
 * @param len  Length of data (1-32 bytes)
 * @param pipe Pipe number (0-7)
 * @return GZ_OK if enqueued, GZ_ERR_BUSY if FIFO full or TX pending
 */
gz_error_t gz_send_start(const uint8_t* data, uint8_t len, uint8_t pipe);

/**
 * @brief Poll the status of the last gz_send_start() operation
 *
 * After GZ_TX_SUCCESS, the ACK payload (if any) is available via
 * gz_get_ack_payload().
 *
 * @return GZ_TX_PENDING, GZ_TX_SUCCESS, or GZ_TX_FAILED
 */
gz_tx_status_t gz_poll_tx_status(void);

#ifdef __cplusplus
}
#endif

#endif // GAZELL_SHIM_H
