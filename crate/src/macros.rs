//! Convenience macros for quick logging without a Logger instance.
//!
//! Two modes:
//! 1. `teo_info!`, `teo_warn!`, etc. — structured wide events via a global logger.
//! 2. `verbose!` — zero-dep stderr diagnostic for CLI tools (from agentic-cli patterns).

use std::sync::atomic::{AtomicBool, Ordering};

/// Global verbose flag for CLI tools.
pub static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Enable or disable verbose output.
pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::Relaxed);
}

/// Check if verbose is enabled.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// Zero-dep verbose macro. Outputs to stderr with `[module]` prefix.
/// Does nothing unless `teolog::set_verbose(true)` was called.
///
/// ```rust,ignore
/// teolog::set_verbose(true);
/// verbose!("harvest", "step 1/3: capturing canvas");
/// verbose!("resolve", "source={}, count={}", source, count);
/// ```
#[macro_export]
macro_rules! verbose {
    ($module:expr, $($arg:tt)*) => {
        if $crate::macros::VERBOSE.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!("[{}] {}", $module, format!($($arg)*));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbose_flag_toggles() {
        set_verbose(false);
        assert!(!is_verbose());
        set_verbose(true);
        assert!(is_verbose());
        set_verbose(false);
    }
}
