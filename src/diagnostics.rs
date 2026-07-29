//! Logging/diagnostics: the single tracing subscriber the binary owns. Libraries only
//! ever emit; this module is the one place a subscriber is installed.

use tracing_subscriber::prelude::*;

/// Guard that flushes the non-blocking file writer on drop. Held by `main` for the
/// process lifetime so buffered log lines are not lost on shutdown. `None` when file
/// logging is disabled (unwritable log dir) and only the console layer is active.
pub struct LogGuard {
    _worker: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Install the one subscriber: console (stderr, ANSI) + a rolling daily file in the
/// OS-standard log dir, gated by a single `EnvFilter` (`RUST_LOG` overrides the default).
///
/// Default filter: our crate at `info`, the GPU/windowing stack at `warn` so wgpu/winit
/// chatter doesn't drown the landmarks. `tracing_log::LogTracer` bridges any `log`-crate
/// records from dependencies into the same sink.
pub fn init_tracing() -> LogGuard {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "lumagen=info,wgpu=warn,wgpu_core=warn,wgpu_hal=warn,naga=warn,winit=warn"
            .parse()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    });

    // Rolling daily file sink in the OS log dir (…/lumagen/logs/lumagen.log.YYYY-MM-DD).
    let log_dir = directories::ProjectDirs::from("dev", "lumagen", "lumagen")
        .map(|d| d.data_dir().join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    // An unwritable log dir (read-only install, locked-down profile) must degrade to
    // console-only logging, not abort startup — the builder surfaces the error the
    // `rolling::daily` helper would panic on.
    let (file, guard) = match tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("lumagen.log")
        .build(&log_dir)
    {
        Ok(appender) => {
            let (file_writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer().with_writer(file_writer).with_ansi(false);
            (Some(layer), Some(guard))
        }
        Err(e) => {
            // The subscriber isn't installed yet at this point in startup, so a tracing
            // event would vanish — stderr is the only channel that reaches the user here.
            #[allow(clippy::print_stderr)]
            {
                eprintln!("lumagen: file logging disabled — cannot write {}: {e}", log_dir.display());
            }
            (None, None)
        }
    };

    let console = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

    tracing_subscriber::registry().with(filter).with(console).with(file).init();

    // Bridge `log`-crate records (from deps that still use the log facade) into tracing.
    let _ = tracing_log::LogTracer::init();

    LogGuard { _worker: guard }
}

/// Log-dir resolution is pure; the fallible piece (creating/opening the file) is covered
/// by the graceful-degradation branch above, which tests can't reach portably without
/// permission fixtures — so only the resolution contract is pinned here.
#[cfg(test)]
mod tests {
    #[test]
    fn log_dir_resolves_without_panicking() {
        let dir = directories::ProjectDirs::from("dev", "lumagen", "lumagen")
            .map(|d| d.data_dir().join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        assert!(!dir.as_os_str().is_empty());
    }
}
