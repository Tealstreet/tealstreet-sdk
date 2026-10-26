//! Host telemetry contracts, with opaque values rather than JSON coercion.
use std::{any::Any, rc::Rc};

#[derive(Clone, Default)]
pub struct TelemetryContext {
    pub tags: Option<Rc<dyn Any>>,
    pub extra: Option<Rc<dyn Any>>,
}

/// Missing optional source callbacks are the default no-op methods.
/// An Err retains the original opaque thrown value; callers decide its policy.
pub trait TelemetrySink {
    fn event(&self, _name: &str, _data: Rc<dyn Any>) -> Result<(), Rc<dyn Any>> {
        Ok(())
    }

    fn capture_exception(
        &self,
        _error: Rc<dyn Any>,
        _context: Rc<TelemetryContext>,
    ) -> Result<(), Rc<dyn Any>> {
        Ok(())
    }
}
