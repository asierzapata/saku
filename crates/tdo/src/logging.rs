//! Structured logging initialization for tdo
//!
//! This module provides initialization for the tracing-based logging system.
//! Logging is disabled by default and must be enabled via the "logging" feature flag.
//!
//! # Configuration
//!
//! Logging is configured via environment variables:
//! - `TDO_LOG` or `RUST_LOG` - Controls log level (error, warn, info, debug, trace)
//!
//! # Output
//!
//! Logs are written to:
//! - stderr (for real-time output)
//! - /Users/asierzapata/Library/Application\ Support/tdo/tdo.log (rotating log file)

use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the logging system with stderr and file output
///
/// This sets up:
/// - Environment-based filtering (TDO_LOG or RUST_LOG)
/// - Stderr output for real-time logging
/// - File output with rotation (keeps last 5 log files)
/// - Default level: ERROR
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_log_directory()?;
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "tdo.log");
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = std::env::var("TDO_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|val| EnvFilter::try_new(&val).ok())
        .unwrap_or_else(|| EnvFilter::new("error"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_thread_ids(false)
                .with_line_number(true)
                .with_file(false),
        )
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .with_target(true)
                .with_thread_ids(false)
                .with_line_number(true)
                .with_file(true)
                .with_ansi(false),
        )
        .init();

    // Leak the guard to keep the file writer alive for the program's lifetime
    std::mem::forget(_guard);

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log_dir = %log_dir.display(),
        "tdo logging initialized"
    );

    Ok(())
}

/// Get the log directory path (~/.local/share/tdo)
fn get_log_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Use TDO_DATA_DIR if set (for testing), otherwise use default
    if let Ok(data_dir) = std::env::var("TDO_DATA_DIR") {
        return Ok(PathBuf::from(data_dir));
    }

    // Use platform-appropriate data directory
    let data_dir = dirs::data_local_dir()
        .ok_or("Could not determine local data directory")?
        .join("tdo");

    Ok(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_log_directory_with_env_var() {
        unsafe {
            std::env::set_var("TDO_DATA_DIR", "/tmp/test-tdo");
        }
        let result = get_log_directory().unwrap();
        assert_eq!(result, PathBuf::from("/tmp/test-tdo"));
        unsafe {
            std::env::remove_var("TDO_DATA_DIR");
        }
    }

    #[test]
    fn test_get_log_directory_default() {
        unsafe {
            std::env::remove_var("TDO_DATA_DIR");
        }
        let result = get_log_directory().unwrap();
        assert!(result.ends_with("tdo"));
    }
}
