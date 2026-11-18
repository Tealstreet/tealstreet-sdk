//! A native 513 grants one clock sample and exact GET replay, never a write.
use crate::{MexcFutures, Pending, wire::Envelope};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{Cx, ErrorKind, Method, Response, VenueError};

pub struct Request {
    context: Context,
    root: Context,
    pending: Box<Pending>,
}

impl MexcFutures {
    pub(crate) fn recover_clock(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: &Response<'_>,
    ) -> Option<Pending> {
        if response.status != 200
            || !serde_json::from_str::<Envelope<'_>>(response.body)
                .is_ok_and(|e| e.success == Some(false) && e.code == 513. && e.data.is_none())
        {
            return Some(pending);
        }
        let root = match &pending {
            Pending::Private { owner, replay, .. } if !replay.retried && owner.current(cx) => {
                Some(owner.root().unwrap_or_else(|| owner.context()).clone())
            }
            Pending::History(recovery) => recovery.clock_root(cx),
            _ => None,
        };
        let Some(root) = root else {
            return Some(pending);
        };
        let context = match cx.http_leg_context(
            &root,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 5_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                self.clock_pending_failed(cx, pending, error.venue_error());
                return None;
            }
        };
        let recovery = Request {
            context: context.clone(),
            root,
            pending: Box::new(pending),
        };
        if let Err((Pending::Clock(recovery), error)) = cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}/api/v1/contract/ping", crate::REST),
            None,
            Pending::Clock(Box::new(recovery)),
        ) {
            self.clock_failed(cx, *recovery, error.venue_error());
        }
        None
    }

    pub(crate) fn clock_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        recovery: Request,
        response: Response<'_>,
    ) {
        let valid = if matches!(
            recovery.root.owner(),
            safe_cex_engine::request_context::Owner::Background { .. }
        ) {
            cx.validate_http_context(&recovery.root)
                .and_then(|()| cx.validate_background_read_http_delivery(&recovery.context))
        } else {
            cx.validate_read_http_delivery(&recovery.root, &recovery.context)
        };
        if let Err(error) = valid {
            self.clock_failed(cx, recovery, error.venue_error());
            return;
        }
        let sample = (response.status == 200)
            .then(|| Envelope::checked(response.body).ok())
            .flatten()
            .and_then(|e| e.data?.get().parse::<u64>().ok())
            .filter(|sample| *sample > 0 && *sample <= 9_007_199_254_740_991);
        let Some(sample) = sample else {
            self.clock_failed(
                cx,
                recovery,
                Envelope::refusal(response.body).unwrap_or_else(|| {
                    VenueError::new("Invalid MEXC server-time response").kind(ErrorKind::Internal)
                }),
            );
            return;
        };
        if !cx.observe_protocol_time(sample as f64) {
            self.clock_failed(
                cx,
                recovery,
                VenueError::new("Invalid MEXC server time").kind(ErrorKind::Internal),
            );
            return;
        }
        let _ = cx.finish_http_context(&recovery.context);
        match *recovery.pending {
            Pending::Private {
                owner,
                pending,
                mut replay,
            } => {
                replay.retried = true;
                self.replay_private(cx, owner, replay, pending);
            }
            Pending::History(recovery) => self.replay_history(cx, recovery),
            _ => unreachable!("only current signed GETs own clock recovery"),
        }
    }

    pub(crate) fn clock_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        recovery: Request,
        error: VenueError,
    ) {
        let _ = cx.finish_http_context(&recovery.context);
        self.clock_pending_failed(cx, *recovery.pending, error);
    }

    fn clock_pending_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
    ) {
        match pending {
            pending @ Pending::Private { .. } => self.private_failed(cx, pending, error),
            Pending::History(recovery) => self.query_failed(cx, recovery, error),
            _ => unreachable!("only signed reads own clock recovery"),
        }
    }
}
