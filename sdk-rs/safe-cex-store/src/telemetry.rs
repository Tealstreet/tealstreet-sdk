//! packages/safe-cex/src/telemetry.ts. Nothing is sent until a host installs a sink.
use safe_cex_types::telemetry::{TelemetryContext, TelemetrySink};
use std::{any::Any, cell::RefCell, rc::Rc};

struct EmptySink;
impl TelemetrySink for EmptySink {}

fn with_sink<T>(operation: impl FnOnce(&RefCell<Rc<dyn TelemetrySink>>) -> T) -> T {
    // Function-local thread state follows the existing service registry pattern;
    // the syntactic parity index need not expand a module-level item macro.
    thread_local! {
        static SINK: RefCell<Rc<dyn TelemetrySink>> = RefCell::new(Rc::new(EmptySink));
    }
    SINK.with(operation)
}

pub fn set_telemetry_sink(next: Rc<dyn TelemetrySink>) {
    with_sink(|sink| *sink.borrow_mut() = next);
}

pub fn report_event(name: &str, data: Rc<dyn Any>) -> Result<(), Rc<dyn Any>> {
    let sink = with_sink(|sink| sink.borrow().clone());
    // Release the registry borrow before a callback can replace the sink.
    sink.event(name, data)
}

pub fn report_exception(
    error: Rc<dyn Any>,
    context: Rc<TelemetryContext>,
) -> Result<(), Rc<dyn Any>> {
    let sink = with_sink(|sink| sink.borrow().clone());
    sink.capture_exception(error, context)
}
