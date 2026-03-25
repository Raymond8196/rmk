#include "gazell_shim.h"

// Include Nordic Gazell SDK headers
#include "nrf_gzll.h"
#include "nrf_gzll_error.h"
#include "nrf.h"

// Maximum payload length from Nordic SDK
#define MAX_PAYLOAD_LENGTH 32

// Internal state management
static struct {
    bool initialized;
    gz_mode_t mode;

    // Saved configuration — gz_set_mode() calls nrf_gzll_init() which
    // resets all Gazell settings to defaults.  We save the config from
    // gz_init() so gz_set_mode() can re-apply it after reinit.
    gz_config_t saved_config;

    // RX state (host mode)
    uint8_t rx_buffer[MAX_PAYLOAD_LENGTH];
    uint32_t rx_length;
    uint8_t rx_pipe;           // Pipe the last RX packet came from
    volatile bool rx_ready;

    // TX state (device mode)
    volatile bool tx_success;
    volatile bool tx_failed;

    // ACK payload state (device mode: payload received in ACK from host)
    uint8_t ack_payload_buffer[MAX_PAYLOAD_LENGTH];
    uint8_t ack_payload_length;
    volatile bool ack_payload_ready;
} gz_state = {0};

// Forward declarations of callback functions
void nrf_gzll_device_tx_success(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info);
void nrf_gzll_device_tx_failed(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info);
void nrf_gzll_host_rx_data_ready(uint32_t pipe, nrf_gzll_host_rx_info_t rx_info);
void nrf_gzll_disabled(void);

//-----------------------------------------------------------------------------
// Gazell Callbacks (called from interrupt context)
//-----------------------------------------------------------------------------

/**
 * @brief Callback for successful device transmission
 * Called when ACK is received from host.
 * If the host attached ACK payload, capture it here.
 */
void nrf_gzll_device_tx_success(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info) {

    // Check if ACK carried payload data from host
    if (tx_info.payload_received_in_ack) {
        // SAFETY: nrf_gzll_fetch_packet_from_rx_fifo expects uint32_t* for length,
        // but gz_state.ack_payload_length is uint8_t (max 32 bytes). Use temporary.
        uint32_t temp_len = MAX_PAYLOAD_LENGTH;
        if (nrf_gzll_fetch_packet_from_rx_fifo(pipe,
                                                gz_state.ack_payload_buffer,
                                                &temp_len)) {
            gz_state.ack_payload_length = (uint8_t)temp_len;
            gz_state.ack_payload_ready = true;
        }
    }

    gz_state.tx_success = true;
}

/**
 * @brief Callback for failed device transmission
 * Called when max retries exceeded without ACK
 */
void nrf_gzll_device_tx_failed(uint32_t pipe, nrf_gzll_device_tx_info_t tx_info) {
    (void)pipe;
    (void)tx_info;
    gz_state.tx_failed = true;
}

/**
 * @brief Callback for host receiving data
 * Called when host receives a packet from device
 */
void nrf_gzll_host_rx_data_ready(uint32_t pipe, nrf_gzll_host_rx_info_t rx_info) {
    (void)rx_info;

    // Fetch the packet from RX FIFO and record which pipe it came from
    gz_state.rx_length = MAX_PAYLOAD_LENGTH;
    if (nrf_gzll_fetch_packet_from_rx_fifo(pipe,
                                            gz_state.rx_buffer,
                                            &gz_state.rx_length)) {
        gz_state.rx_pipe = (uint8_t)pipe;
        gz_state.rx_ready = true;
    }
}

/**
 * @brief Callback for Gazell disabled event
 */
void nrf_gzll_disabled(void) {
    // Optional: handle disable event
}

//-----------------------------------------------------------------------------
// API Implementation
//-----------------------------------------------------------------------------

/**
 * @brief Apply saved config to Gazell (internal helper)
 *
 * Called after nrf_gzll_init() to (re-)apply all user settings.
 * nrf_gzll_init() resets everything to defaults, so this must be
 * called every time after init.
 */
static gz_error_t gz_apply_config(const gz_config_t* config) {
    // Configure base address
    uint32_t base_addr = ((uint32_t)config->base_address[3] << 24) |
                         ((uint32_t)config->base_address[2] << 16) |
                         ((uint32_t)config->base_address[1] << 8)  |
                         ((uint32_t)config->base_address[0]);
    nrf_gzll_set_base_address_0(base_addr);

    // Configure address prefix for pipe 0
    nrf_gzll_set_address_prefix_byte(0, config->address_prefix);

    // Configure TX power
    nrf_gzll_set_tx_power((nrf_gzll_tx_power_t)config->tx_power);

    // Configure data rate
    nrf_gzll_datarate_t rate;
    switch (config->data_rate) {
        case 1:
            rate = NRF_GZLL_DATARATE_1MBIT;
            break;
        case 2:
            rate = NRF_GZLL_DATARATE_2MBIT;
            break;
        default:
            return GZ_ERR_INVALID_CONFIG;
    }
    nrf_gzll_set_datarate(rate);

    // Configure channel (single channel, no hopping)
    uint8_t channels[] = {config->channel};
    nrf_gzll_set_channel_table(channels, 1);

    // Configure max retries
    nrf_gzll_set_max_tx_attempts(config->max_retries);

    // Configure timeslot period (affects ACK timeout)
    uint32_t timeslot = config->ack_timeout_us / 500;
    if (timeslot < 1) timeslot = 1;
    nrf_gzll_set_timeslot_period(timeslot);

    return GZ_OK;
}

gz_error_t gz_init(const gz_config_t* config) {
    if (config == NULL) {
        return GZ_ERR_INVALID_CONFIG;
    }

    // Validate configuration parameters
    if (config->channel > 100) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (config->data_rate < 1 || config->data_rate > 2) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (config->max_retries > 15) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (config->ack_timeout_us < 250 || config->ack_timeout_us > 4000) {
        return GZ_ERR_INVALID_CONFIG;
    }

    // Save config for later re-application in gz_set_mode()
    gz_state.saved_config = *config;

    // Clear state
    gz_state.initialized = false;
    gz_state.rx_ready = false;
    gz_state.rx_pipe = 0;
    gz_state.tx_success = false;
    gz_state.tx_failed = false;
    gz_state.ack_payload_ready = false;
    gz_state.ack_payload_length = 0;

    // Initialize Gazell in device mode (will switch mode later if needed)
    if (!nrf_gzll_init(NRF_GZLL_MODE_DEVICE)) {
        return GZ_ERR_HARDWARE;
    }

    // Apply user configuration
    gz_error_t err = gz_apply_config(config);
    if (err != GZ_OK) {
        return err;
    }

    gz_state.initialized = true;

    return GZ_OK;
}

gz_error_t gz_set_mode(gz_mode_t mode) {
    if (!gz_state.initialized) {
        return GZ_ERR_NOT_INITIALIZED;
    }

    // Disable Gazell before mode change
    nrf_gzll_disable();
    while (nrf_gzll_is_enabled()) {
        __WFE();
    }

    // Set new mode
    nrf_gzll_mode_t nrf_mode;
    if (mode == GZ_MODE_DEVICE) {
        nrf_mode = NRF_GZLL_MODE_DEVICE;
    } else if (mode == GZ_MODE_HOST) {
        nrf_mode = NRF_GZLL_MODE_HOST;
    } else {
        return GZ_ERR_INVALID_CONFIG;
    }

    // Reinitialize with new mode
    // IMPORTANT: nrf_gzll_init() resets ALL settings to defaults.
    // We must re-apply saved config afterwards.
    if (!nrf_gzll_init(nrf_mode)) {
        return GZ_ERR_HARDWARE;
    }

    // Re-apply user configuration (lost by nrf_gzll_init)
    gz_error_t err = gz_apply_config(&gz_state.saved_config);
    if (err != GZ_OK) {
        return err;
    }

    // Enable Gazell
    if (!nrf_gzll_enable()) {
        return GZ_ERR_HARDWARE;
    }

    gz_state.mode = mode;

    return GZ_OK;
}

gz_error_t gz_send(const uint8_t* data, uint8_t len, uint8_t pipe) {
    if (!gz_state.initialized) {
        return GZ_ERR_NOT_INITIALIZED;
    }

    if (data == NULL) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (len == 0 || len > MAX_PAYLOAD_LENGTH) {
        return GZ_ERR_FRAME_TOO_LARGE;
    }

    if (pipe > 7) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (gz_state.mode != GZ_MODE_DEVICE) {
        return GZ_ERR_INVALID_CONFIG;
    }

    // Clear TX flags and ACK payload state
    gz_state.tx_success = false;
    gz_state.tx_failed = false;
    gz_state.ack_payload_ready = false;

    // Add packet to TX FIFO on specified pipe
    // Cast away const — Nordic SDK API doesn't take const but won't modify the buffer
    if (!nrf_gzll_add_packet_to_tx_fifo(pipe, (uint8_t*)data, len)) {
        return GZ_ERR_BUSY;
    }

    // Wait for TX complete with timeout
    // Timeout calculation: max_retries * timeslot_period + margin
    // Conservative estimate: 10ms should be sufficient for most cases
    volatile uint32_t timeout = 100000; // ~10ms at 10 cycles per loop

    while (timeout-- > 0) {
        if (gz_state.tx_success) {
            return GZ_OK;
        }
        if (gz_state.tx_failed) {
            return GZ_ERR_SEND_FAILED;
        }
        // Sleep until next interrupt (radio ISR will wake us)
        __WFE();
    }

    // Timeout occurred
    return GZ_ERR_SEND_FAILED;
}

gz_error_t gz_recv(uint8_t* out_buf, uint8_t* out_len, uint8_t* out_pipe, uint8_t max_len) {
    if (!gz_state.initialized) {
        return GZ_ERR_NOT_INITIALIZED;
    }

    if (out_buf == NULL || out_len == NULL || out_pipe == NULL) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (gz_state.mode != GZ_MODE_HOST) {
        return GZ_ERR_INVALID_CONFIG;
    }

    // Check if data is available
    if (!gz_state.rx_ready) {
        *out_len = 0;
        *out_pipe = 0;
        return GZ_OK; // No data available, not an error
    }

    // Check buffer size
    if (gz_state.rx_length > max_len) {
        return GZ_ERR_FRAME_TOO_LARGE;
    }

    // Copy data to output buffer
    for (uint8_t i = 0; i < gz_state.rx_length; i++) {
        out_buf[i] = gz_state.rx_buffer[i];
    }

    *out_len = (uint8_t)gz_state.rx_length;
    *out_pipe = gz_state.rx_pipe;
    gz_state.rx_ready = false;

    return GZ_OK;
}

gz_error_t gz_set_ack_payload(uint8_t pipe, const uint8_t* data, uint8_t len) {
    if (!gz_state.initialized) {
        return GZ_ERR_NOT_INITIALIZED;
    }

    if (data == NULL) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (len == 0 || len > MAX_PAYLOAD_LENGTH) {
        return GZ_ERR_FRAME_TOO_LARGE;
    }

    if (pipe > 7) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (gz_state.mode != GZ_MODE_HOST) {
        return GZ_ERR_INVALID_CONFIG;
    }

    // In host mode, nrf_gzll_add_packet_to_tx_fifo adds ACK payload
    // for the specified pipe. The data will be sent in the next ACK
    // when a device transmits on that pipe.
    // Cast away const — Nordic SDK API doesn't take const but won't modify the buffer
    if (!nrf_gzll_add_packet_to_tx_fifo(pipe, (uint8_t*)data, len)) {
        return GZ_ERR_BUSY;
    }

    return GZ_OK;
}

gz_error_t gz_get_ack_payload(uint8_t* out_buf, uint8_t* out_len, uint8_t max_len) {
    if (!gz_state.initialized) {
        return GZ_ERR_NOT_INITIALIZED;
    }

    if (out_buf == NULL || out_len == NULL) {
        return GZ_ERR_INVALID_CONFIG;
    }

    if (gz_state.mode != GZ_MODE_DEVICE) {
        return GZ_ERR_INVALID_CONFIG;
    }

    // Check if ACK payload was received
    if (!gz_state.ack_payload_ready) {
        *out_len = 0;
        return GZ_OK; // No ACK payload, not an error
    }

    // Check buffer size
    if (gz_state.ack_payload_length > max_len) {
        return GZ_ERR_FRAME_TOO_LARGE;
    }

    // Copy ACK payload to output buffer
    for (uint8_t i = 0; i < gz_state.ack_payload_length; i++) {
        out_buf[i] = gz_state.ack_payload_buffer[i];
    }

    *out_len = gz_state.ack_payload_length;
    gz_state.ack_payload_ready = false;

    return GZ_OK;
}

bool gz_is_ready(uint8_t pipe) {
    if (!gz_state.initialized) {
        return false;
    }

    if (pipe > 7) {
        return false;
    }

    // Check if TX FIFO has space on specified pipe
    return nrf_gzll_get_tx_fifo_packet_count(pipe) < NRF_GZLL_CONST_FIFO_LENGTH;
}

gz_error_t gz_flush(void) {
    if (!gz_state.initialized) {
        return GZ_ERR_NOT_INITIALIZED;
    }

    // Flush TX and RX FIFOs for all pipes (0-7)
    for (uint8_t p = 0; p < 8; p++) {
        nrf_gzll_flush_tx_fifo(p);
        if (gz_state.mode == GZ_MODE_HOST) {
            nrf_gzll_flush_rx_fifo(p);
        }
    }

    // Clear state flags
    gz_state.rx_ready = false;
    gz_state.rx_pipe = 0;
    gz_state.tx_success = false;
    gz_state.tx_failed = false;
    gz_state.ack_payload_ready = false;
    gz_state.ack_payload_length = 0;

    return GZ_OK;
}

void gz_deinit(void) {
    if (gz_state.initialized) {
        nrf_gzll_disable();

        // Wait for disable to complete
        while (nrf_gzll_is_enabled()) {
            __WFE();
        }

        gz_state.initialized = false;
        gz_state.rx_ready = false;
        gz_state.rx_pipe = 0;
        gz_state.tx_success = false;
        gz_state.tx_failed = false;
        gz_state.ack_payload_ready = false;
        gz_state.ack_payload_length = 0;
    }
}

// Minimal init: pure defaults, no custom config.
// For debugging: eliminates any config mismatch possibility.
gz_error_t gz_init_default(gz_mode_t mode) {
    gz_state.initialized = false;
    gz_state.rx_ready = false;
    gz_state.rx_pipe = 0;
    gz_state.tx_success = false;
    gz_state.tx_failed = false;
    gz_state.ack_payload_ready = false;
    gz_state.ack_payload_length = 0;
    nrf_gzll_mode_t nrf_mode = (mode == GZ_MODE_HOST)
        ? NRF_GZLL_MODE_HOST
        : NRF_GZLL_MODE_DEVICE;

    if (!nrf_gzll_init(nrf_mode)) {
        return GZ_ERR_HARDWARE;
    }
    if (!nrf_gzll_enable()) {
        return GZ_ERR_HARDWARE;
    }

    gz_state.initialized = true;
    gz_state.mode = mode;
    return GZ_OK;
}
