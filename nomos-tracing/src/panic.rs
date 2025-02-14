use std::{
    backtrace::{Backtrace, BacktraceStatus},
    panic::PanicHookInfo,
};

pub fn panic_hook(panic_info: &PanicHookInfo) {
    let payload = panic_info.payload();

    #[allow(clippy::manual_map)]
    let payload = if let Some(s) = payload.downcast_ref::<&str>() {
        Some(&**s)
    } else if let Some(s) = payload.downcast_ref::<String>() {
        Some(s.as_str())
    } else {
        None
    };

    let location = panic_info.location().map(|l| l.to_string());
    let backtrace = Backtrace::capture();
    let note = (backtrace.status() == BacktraceStatus::Disabled)
        .then_some("run with RUST_BACKTRACE=1 environment variable to display a backtrace");

    tracing::error!(
        panic.payload = payload,
        panic.location = location,
        panic.backtrace = backtrace.to_string(),
        panic.note = note,
        "A panic occurred",
    );
}
