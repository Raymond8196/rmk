use std::env;

fn main() {
    // Only build on ARM Cortex-M4F targets (nRF52 series)
    let target = env::var("TARGET").unwrap();

    // Allow build on non-ARM targets for IDE support and cargo check,
    // but skip actual compilation
    if !target.starts_with("thumbv7em-none-eabi") {
        println!(
            "cargo:warning=rmk-gazell-sys: Skipping compilation for non-ARM target: {}",
            target
        );
        println!("cargo:warning=rmk-gazell-sys: This crate only supports ARM Cortex-M4F targets (nRF52 series)");
        println!("cargo:warning=rmk-gazell-sys: Build will succeed but library will not be functional");
        return;
    }

    // Get Nordic SDK path from environment
    let sdk_path = match env::var("NRF5_SDK_PATH") {
        Ok(path) => {
            println!("cargo:warning=Using Nordic SDK from: {}", path);
            path
        }
        Err(_) => {
            println!("cargo:warning=ERROR: NRF5_SDK_PATH environment variable not set");
            println!("cargo:warning=");
            println!("cargo:warning=Please download Nordic nRF5 SDK v17.1.0 from:");
            println!("cargo:warning=https://www.nordicsemi.com/Products/Development-software/nRF5-SDK");
            println!("cargo:warning=");
            println!("cargo:warning=Then set the environment variable:");
            println!("cargo:warning=  export NRF5_SDK_PATH=/path/to/nRF5_SDK_17.1.0");
            println!("cargo:warning=");
            panic!("NRF5_SDK_PATH not set");
        }
    };

    // Tell cargo to re-run build script if SDK path changes
    println!("cargo:rerun-if-env-changed=NRF5_SDK_PATH");
    println!("cargo:rerun-if-changed=c/gazell_shim.c");
    println!("cargo:rerun-if-changed=c/gazell_shim.h");

    // Determine chip variant from features
    let (chip_define, lib_variant) = if cfg!(feature = "nrf52840") {
        ("NRF52840_XXAA", "nrf52840")
    } else if cfg!(feature = "nrf52833") {
        ("NRF52833_XXAA", "nrf52833")
    } else if cfg!(feature = "nrf52832") {
        ("NRF52832_XXAA", "nrf52832")
    } else {
        println!("cargo:warning=ERROR: Must enable exactly one chip feature");
        println!("cargo:warning=Available features: nrf52840, nrf52833, nrf52832");
        println!("cargo:warning=");
        println!("cargo:warning=Example: cargo build --features nrf52840");
        panic!("No chip feature enabled");
    };

    println!("cargo:warning=Building for chip: {}", chip_define);

    // Build C shim library
    let mut build = cc::Build::new();

    build
        .file("c/gazell_shim.c")
        .include("c")
        // Nordic Gazell SDK headers
        .include(format!("{}/components/proprietary_rf/gzll", sdk_path))
        // nRF MDK (device headers)
        .include(format!("{}/modules/nrfx/mdk", sdk_path))
        // nrfx HAL
        .include(format!("{}/modules/nrfx/hal", sdk_path))
        .include(format!("{}/modules/nrfx", sdk_path))
        // Integration
        .include(format!("{}/integration/nrfx", sdk_path))
        .include(format!("{}/integration/nrfx/legacy", sdk_path))
        // CMSIS headers (core_cm4.h, etc.)
        .include(format!("{}/components/toolchain/cmsis/include", sdk_path))
        // SDK common utilities (sdk_errors.h, app_error.h, nrf_assert.h, etc.)
        .include(format!("{}/components/libraries/util", sdk_path))
        // SoftDevice headers (nrf_error.h for no-SD builds)
        .include(format!("{}/components/drivers_nrf/nrf_soc_nosd", sdk_path))
        // Chip-specific config (sdk_config.h)
        .include(format!("{}/config/{}/config", sdk_path, lib_variant))
        // Define chip variant
        .define(chip_define, None)
        // Standard defines for Nordic SDK
        .define("FLOAT_ABI_HARD", None)
        // Optimization flags
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        // Disable warnings for SDK code
        .flag("-Wno-unused-parameter")
        .flag("-Wno-expansion-to-defined");

    // Compile
    build.compile("gazell_shim");

    // Link Nordic Gazell precompiled library
    // The SDK provides precompiled libraries in components/proprietary_rf/gzll/gcc/
    let gzll_lib_dir = format!("{}/components/proprietary_rf/gzll/gcc", sdk_path);

    println!("cargo:rustc-link-search=native={}", gzll_lib_dir);

    // Library naming convention: gzll_<chip>_gcc_<softdevice>.a
    // For bare metal (no softdevice), use: gzll_nrf52832_gcc.a, etc.
    let lib_name = format!("gzll_{}_gcc", lib_variant);

    // Nordic SDK uses non-standard naming (no "lib" prefix), so use `:` syntax
    println!("cargo:rustc-link-lib=static:+verbatim={}.a", lib_name);
    println!("cargo:warning=Linking Gazell library: {}.a", lib_name);

    // FFI bindings are hand-written in src/lib.rs (no bindgen needed).
    // This avoids dependency on libclang which may not work in all environments.
    println!("cargo:warning=rmk-gazell-sys build complete");
}
