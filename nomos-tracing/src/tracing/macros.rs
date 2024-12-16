#[macro_export]
macro_rules! trace_with_parent {
    ($idbytes:expr, $msg:expr $(, $key:ident = $value:expr)*) => {{
        use opentelemetry::trace::{SpanContext, SpanId, TraceId, TraceState};
        use opentelemetry::Context;
        use tracing::Span;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        // Derive trace_id from bytes.
        let trace_id = TraceId::from_bytes($idbytes);

        // Create a SpanContext with the derived trace_id.
        let span_id = SpanId::new();
        let span_context = SpanContext::new(
            trace_id,
            span_id,
            0, // Smapled
            false, // Not a remote span
            TraceState::default(),
        );

        let otel_context = Context::current_with_span_context(span_context);

        let current_span = Span::current();
        current_span.set_parent(otel_context);

        // Call tracing with the updated parent span id.
        tracing::info!($msg $(, $key = $value)*);
    }};
}
