//! A native timestamp refusal grants one exact GET replay under the same root.
use crate::sign;
use safe_cex_engine::credential_selection::Selection;
use safe_cex_engine::request_context::{Context, Operation, Owner, Policy, RetryPolicy};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::{Cx, Method, RequestId, Response, VenueError};
use serde::Deserialize;

pub struct Request<P> {
    pub(crate) pending: P,
    root: Context,
    leg: Context,
    auth: Option<(Selection, Identity)>,
    origin: &'static str,
    path: String,
    broker: Option<sign::dispatch::Broker>,
    clock: bool,
    pub(crate) retried: bool,
    stream: Option<safe_cex_engine::json::ArrayAt>,
}

pub enum Delivery<P> {
    Page {
        pending: P,
        leg: Context,
        root: Context,
    },
    Failed(P, VenueError),
    Waiting,
}

fn policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn send<P>(
    cx: &mut Cx<'_, P>,
    root: Context,
    auth: Option<(Selection, Identity)>,
    origin: &'static str,
    path: String,
    broker: Option<sign::dispatch::Broker>,
    pending: P,
    stream: Option<safe_cex_engine::json::ArrayAt>,
    wrap: fn(Box<Request<P>>) -> P,
) -> Result<RequestId, (P, VenueError)> {
    let leg = match cx.http_leg_context(
        &root,
        Policy {
            credential: auth
                .as_ref()
                .map(|(selection, identity)| selection.authority(identity)),
            ..policy(20_000)
        },
    ) {
        Ok(leg) => leg,
        Err(error) => {
            if matches!(root.owner(), Owner::Background { .. }) {
                let _ = cx.finish_http_context(&root);
            }
            return Err((pending, error.venue_error()));
        }
    };
    queue(
        cx,
        Request {
            pending,
            root,
            leg,
            auth,
            origin,
            path,
            broker,
            clock: false,
            retried: false,
            stream,
        },
        wrap,
    )
}

fn queue<P>(
    cx: &mut Cx<'_, P>,
    read: Request<P>,
    wrap: fn(Box<Request<P>>) -> P,
) -> Result<RequestId, (P, VenueError)> {
    let leg = read.leg.clone();
    let stream = (!read.clock).then_some(read.stream).flatten();
    let result = if read.clock {
        cx.request_managed(
            leg,
            None,
            Method::Get,
            format!("{}/api/v1/timestamp", read.origin),
            None,
            wrap(Box::new(read)),
        )
    } else if let Some((_, identity)) = &read.auth {
        let recipe = sign::dispatch::Http::new(
            identity.clone(),
            Method::Get,
            read.origin,
            read.path.clone(),
            None,
            read.broker.clone(),
        );
        cx.request_signed_managed(
            leg,
            None,
            Method::Get,
            format!("{}{}", read.origin, crate::encoded_path(&read.path)),
            None,
            Box::new(recipe),
            wrap(Box::new(read)),
        )
    } else {
        cx.request_managed_with_headers(
            leg,
            None,
            Method::Get,
            format!("{}{}", read.origin, crate::encoded_path(&read.path)),
            sign::PUBLIC_HEADERS,
            None,
            wrap(Box::new(read)),
        )
    };
    match result {
        Ok(id) => {
            if let Some(at) = stream {
                cx.stream_response(id, at);
            }
            Ok(id)
        }
        Err((pending, error)) => Err((pending, error.venue_error())),
    }
}

#[derive(Deserialize)]
struct Code<'a> {
    code: &'a str,
    #[serde(borrow)]
    data: Option<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
struct Time<'a> {
    code: &'a str,
    data: u64,
}

pub fn respond<P>(
    cx: &mut Cx<'_, P>,
    mut read: Request<P>,
    response: &Response<'_>,
    wrap: fn(Box<Request<P>>) -> P,
) -> Delivery<P> {
    let valid = match read.root.owner() {
        Owner::Background { .. } => cx.validate_background_read_http_delivery(&read.leg),
        _ => cx.validate_read_http_delivery(&read.root, &read.leg),
    };
    if valid.is_err() {
        return Delivery::Waiting;
    }
    if read.clock {
        let sample = serde_json::from_str::<Time>(response.body)
            .ok()
            .filter(|s| {
                (200..300).contains(&response.status)
                    && s.code == "200000"
                    && s.data > 0
                    && s.data <= 9_007_199_254_740_991
            });
        let Some(sample) = sample else {
            return Delivery::Failed(
                wrap(Box::new(read)),
                VenueError::new("Invalid KuCoin server-time response"),
            );
        };
        if !cx.observe_protocol_time(sample.data as f64) {
            return Delivery::Failed(
                wrap(Box::new(read)),
                VenueError::new("Invalid KuCoin server time"),
            );
        }
        let _ = cx.finish_http_context(&read.leg);
        read.clock = false;
        read.leg = match cx.http_leg_context(
            &read.root,
            Policy {
                credential: read
                    .auth
                    .as_ref()
                    .map(|(selection, identity)| selection.authority(identity)),
                ..policy(20_000)
            },
        ) {
            Ok(leg) => leg,
            Err(error) => return Delivery::Failed(wrap(Box::new(read)), error.venue_error()),
        };
    } else if read.auth.is_some()
        && !read.retried
        && serde_json::from_str::<Code>(response.body)
            .is_ok_and(|e| e.code == "400002" && e.data.is_none())
    {
        let _ = cx.finish_http_context(&read.leg);
        read.retried = true;
        read.clock = true;
        read.leg = match cx.http_leg_context(&read.root, policy(5_000)) {
            Ok(leg) => leg,
            Err(error) => return Delivery::Failed(wrap(Box::new(read)), error.venue_error()),
        };
    } else {
        return Delivery::Page {
            pending: read.pending,
            leg: read.leg,
            root: read.root,
        };
    }
    match queue(cx, read, wrap) {
        Ok(_) => Delivery::Waiting,
        Err((pending, error)) => Delivery::Failed(pending, error),
    }
}

pub fn failed<P>(cx: &mut Cx<'_, P>, read: Request<P>, error: VenueError) -> (P, VenueError) {
    let _ = cx.finish_http_context(&read.leg);
    if matches!(read.root.owner(), Owner::Background { .. }) {
        let _ = cx.finish_http_context(&read.root);
    }
    (read.pending, error)
}
