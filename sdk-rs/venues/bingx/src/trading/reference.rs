//! One unsigned read under the initiating command, before destructive admission.
use super::*;
use crate::wire::{Daily, Premium};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::plan::{CallTerms, RetryWindow};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};

pub struct ReferenceRead {
    entry: Entry,
    context: Context,
    command: CommandId,
    job: Job,
    symbol: String,
    source: StopTriggerType,
    quote: Option<(f64, f64)>,
    invalid: bool,
}
impl ReferenceRead {
    pub(crate) fn item(&mut self, cx: &Cx<'_, Pending>, item: &str) {
        let Entry::Preflight(owner, _) = &self.entry else {
            return;
        };
        if cx
            .validate_preflight_delivery(owner, &self.context)
            .is_err()
        {
            return;
        }
        let quote = match self.source {
            StopTriggerType::LastPrice => serde_json::from_str::<Daily<'_>>(item)
                .map(|row| (row.symbol.into_owned(), row.last_price)),
            StopTriggerType::MarkPrice => serde_json::from_str::<Premium<'_>>(item)
                .map(|row| (row.symbol.into_owned(), row.mark_price)),
            StopTriggerType::IndexPrice => unreachable!("unsupported protective source"),
        };
        match quote {
            Ok((symbol, value)) if symbol == self.symbol => {
                if self.quote.is_some() || !value.is_finite() || value <= 0.0 {
                    self.invalid = true;
                } else {
                    self.quote = Some((value, cx.now));
                }
            }
            Ok(_) => {}
            Err(_) => self.invalid = true,
        }
    }

    pub(crate) fn reject(&self, cx: &mut Cx<'_, Pending>, cause: &str) {
        let mut error = not_sent(format!(
            "Your {} stop was not changed: reference stale ({cause})",
            self.job
                .guard
                .as_ref()
                .expect("protective read")
                .order
                .symbol,
        ))
        .kind(ErrorKind::InvalidInput);
        error.presents = true;
        self.entry.reject(cx, error);
    }
}

impl Trading {
    pub(super) fn read_reference(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        entry: Entry,
        command: CommandId,
        job: Job,
    ) {
        let guard = job.guard.as_ref().expect("protective read");
        let Some(market) = cx.store.market(&guard.order.symbol) else {
            entry.reject(cx, not_sent("Protective reference market is unavailable"));
            return;
        };
        let symbol = market.id.clone();
        let source = guard.source.expect("protective source");
        let path = match source {
            StopTriggerType::LastPrice => crate::paths::DAILY,
            StopTriggerType::MarkPrice => crate::paths::PREMIUM,
            StopTriggerType::IndexPrice => {
                entry.reject(cx, not_sent("BingX Index triggers are unsupported"));
                return;
            }
        };
        let Entry::Preflight(owner, _) = &entry else {
            return;
        };
        let context = match cx.preflight_read_context(
            owner,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            CallTerms {
                wait_ms: 0,
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::FreshAttempt,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                entry.reject(cx, error.venue_error().outcome(Outcome::NotSent));
                return;
            }
        };
        // Omit symbol to retain the existing streamed array shape; symbol-filtered
        // BingX public endpoints return an object. Only the target row is retained.
        let (url, _, body) = crate::sign::dispatch::unsigned(Method::Get, path, &Params::new());
        let read = ReferenceRead {
            entry,
            context: context.clone(),
            command,
            job,
            symbol,
            source,
            quote: None,
            invalid: false,
        };
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Get,
            url,
            crate::sign::unsigned_headers(Method::Get),
            body,
            Pending::ProtectiveReference(Box::new(read)),
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::EnvelopeField("data")),
            Err((Pending::ProtectiveReference(read), error)) => {
                read.reject(cx, &error.venue_error().message)
            }
            Err(_) => unreachable!("protective pending"),
        }
    }

    pub(crate) fn reference_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        read: ReferenceRead,
        accepted: Result<(), String>,
    ) {
        let Entry::Preflight(owner, _) = &read.entry else {
            return;
        };
        if cx
            .validate_preflight_delivery(owner, &read.context)
            .is_err()
        {
            return;
        }
        if let Err(error) = accepted {
            read.reject(cx, &error);
            return;
        }
        let Some((value, at)) = read.quote.filter(|(_, at)| {
            !read.invalid && cx.now.is_finite() && (0.0..=5_000.0).contains(&(cx.now - at))
        }) else {
            read.reject(cx, "missing, invalid or delayed public reference");
            return;
        };
        let guard = read.job.guard.as_ref().expect("protective read");
        // A Store mismatch cannot authorize an older row over a newer receipt.
        if !self
            .references
            .get(&guard.order.symbol)
            .is_some_and(|r| r.received_after(read.source, at))
        {
            cx.store
                .update_ticker(&guard.order.symbol, |ticker| match read.source {
                    StopTriggerType::LastPrice => ticker.last = value,
                    StopTriggerType::MarkPrice => ticker.mark = value,
                    StopTriggerType::IndexPrice => unreachable!("unsupported protective source"),
                });
            self.observe_reference(&guard.order.symbol, read.source, value, at);
        }
        if let Err(error) = guard.check(cx.store, &self.references, cx.now, false) {
            read.entry.reject(cx, error);
            return;
        }
        if let Err(error) = cx.finish_preflight_call(owner, &read.context) {
            read.entry
                .reject(cx, error.venue_error().outcome(Outcome::NotSent));
            return;
        }
        self.admit_job(cx, credentials, read.entry, read.command, read.job, |_| {});
    }
}
