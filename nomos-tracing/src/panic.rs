use std::{
    backtrace::{Backtrace, BacktraceStatus},
    panic::PanicHookInfo,
};

pub fn panic_hook(panic_info: &PanicHookInfo) {
    let payload = panic_info.payload();

    #[allow(clippy::manual_map)]
    let payload = payload.downcast_ref::<&str>().map_or_else(
        || payload.downcast_ref::<String>().map(|s| s.as_str()),
        |s| Some(&**s),
    );

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
