//! One unsigned native page with an original command Read owner.
use super::*;
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};

pub struct Request {
    query: Query,
    history: Box<ReadCursorHistory<Output>>,
    leg: Context,
    valid: bool,
    raw_rows: usize,
    limit: usize,
}

pub fn start(
    cx: &mut Cx<'_, Pending>,
    account: &Account,
    command: CommandId,
    symbol: String,
    limit: Option<u32>,
) {
    let Ok(root) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    let Ok(completion) = ReadCompletion::from_context(cx, command, root.clone()) else {
        return;
    };
    let Some(market) = cx.store.market(&symbol) else {
        let _ = completion.finish(cx, Ok("[]".into()));
        return;
    };
    let size = limit.unwrap_or(100).clamp(1, 500);
    let url = format!(
        "{}/trades?market={}&page_size={size}",
        account.rest(),
        encode(&market.id)
    );
    let query = Query::PublicTrades { symbol, limit };
    let read = Request {
        history: Box::new(ReadCursorHistory::new(
            completion,
            CursorWalk::new(page_cap(&query)),
            stall_message,
            encode_rows,
        )),
        query,
        leg: root.clone(),
        valid: true,
        raw_rows: 0,
        limit: size as usize,
    };
    match cx.request_managed(root, Method::Get, url, None, Pending::PublicTrades(read)) {
        Ok(id) => cx.stream_response(id, ArrayAt::Field("results")),
        Err((Pending::PublicTrades(read), error)) => failed(cx, read, error.venue_error()),
        Err(_) => {}
    }
}

pub fn item(cx: &Cx<'_, Pending>, read: &mut Request, item: &str) {
    let Ok(mut page) = read.history.admit(cx, &read.leg) else {
        return;
    };
    read.raw_rows += 1;
    let Ok(row) = serde_json::from_str::<Row>(item) else {
        read.valid = false;
        return;
    };
    if !valid_row(&row, &read.query, false) {
        read.valid = false;
        return;
    }
    let Query::PublicTrades { symbol, .. } = &read.query else {
        return;
    };
    if row.market != *symbol || !page.fresh(row.id.clone()) {
        return;
    }
    if let Some(row) = public_trade(row) {
        page.stage(row);
    }
}

pub fn response(cx: &mut Cx<'_, Pending>, mut read: Request, response: Response<'_>) {
    if read.history.validate_delivery(cx, &read.leg).is_err() {
        return;
    }
    if let Err(error) = crate::success(response) {
        failed(cx, read, VenueError::new(error));
        return;
    }
    if !read.valid {
        failed(cx, read, VenueError::new("malformed Paradex history row"));
        return;
    }
    #[derive(Deserialize)]
    struct Envelope<'a> {
        next: Option<String>,
        #[serde(borrow, rename = "results")]
        rows: &'a serde_json::value::RawValue,
    }
    let Ok(envelope) = serde_json::from_str::<Envelope<'_>>(response.body) else {
        failed(
            cx,
            read,
            VenueError::new("invalid Paradex history envelope"),
        );
        return;
    };
    if !envelope.rows.get().starts_with('[') {
        failed(cx, read, VenueError::new("invalid Paradex history list"));
        return;
    }
    // Native PublicTrades validates the cursor's type but always ignores it.
    let _ = envelope.next;
    {
        let Ok(mut page) = read.history.admit(cx, &read.leg) else {
            return;
        };
        page.advance(CursorPage {
            next: None,
            rows: read.raw_rows,
            fresh: read.raw_rows,
            want_more: true,
        });
    }
    let _ = read.history.finish_with(cx, Ok(()), move |mut rows| {
        rows.truncate(read.limit);
        serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
    });
}

pub fn failed(cx: &mut Cx<'_, Pending>, mut read: Request, error: VenueError) {
    let _ = read.history.discard_staged_page();
    let _ = read.history.reject(cx, error);
}
