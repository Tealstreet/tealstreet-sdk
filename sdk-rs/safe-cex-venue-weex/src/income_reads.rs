//! Income REST page body and a consumer of the shared, Bitget-owned typed pager.
use crate::types::ENDPOINTS;
use safe_cex_exchange::{
    income_paging::{IncomePageParams, IncomeRangeOptions, paginate_income_range},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::IncomeRecord;
use std::{future::Future, pin::Pin, rc::Rc};
type F<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
type Mapper<'a, V, E> = Box<dyn FnMut(V) -> Result<V, E> + 'a>;
pub struct WeexIncomeRuntime<H, V, E> {
    pub public_only: fn(&H) -> Result<bool, E>,
    pub request_factory: fn(&H) -> Result<V, E>,
    pub post: for<'a> fn(&'a H, V, &'static str, V) -> Result<F<'a, V, E>, E>,
    pub optional_get: fn(&H, &V, &str) -> Result<V, E>,
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub nullish: fn(&V) -> bool,
    pub undefined: fn() -> V,
    pub null: fn() -> V,
    pub text: fn(&str) -> V,
    pub number: fn(f64) -> V,
    pub numeric: fn(&H, V) -> Result<f64, E>,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub extract_array: fn(&H, V, &[&str]) -> Result<V, E>,
    pub map: for<'a> fn(&'a H, &V, Mapper<'a, V, E>) -> Result<V, E>,
    pub normalize: fn(&H, V) -> Result<V, E>,
}
pub struct WeexIncomeRangeRuntime<H, V> {
    pub fetch_page: for<'a> fn(
        &'a H,
        IncomePageParams,
        V,
    ) -> Result<ExchangeFuture<'a, Vec<IncomeRecord>>, ExchangeError>,
    pub sleep: for<'a> fn(&'a H, f64) -> Result<ExchangeFuture<'a, ()>, ExchangeError>,
    pub warning_sink: fn(&H) -> Result<V, ExchangeError>,
    pub warning: fn(&H, V, Vec<u16>) -> Result<(), ExchangeError>,
    pub iso_date: fn(&H, f64) -> Result<Vec<u16>, ExchangeError>,
}
fn eager<'a, V: 'a, E: 'a>(mut f: F<'a, V, E>) -> F<'a, V, E> {
    let w = std::task::Waker::noop().clone();
    match f.as_mut().poll(&mut std::task::Context::from_waker(&w)) {
        std::task::Poll::Ready(x) => Box::pin(std::future::ready(x)),
        std::task::Poll::Pending => Box::pin(js_await(f)),
    }
}
async fn js_await<T>(f: impl Future<Output = T>) -> T {
    let mut suspended = false;
    std::future::poll_fn(|cx| {
        if suspended {
            std::task::Poll::Ready(())
        } else {
            suspended = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
    f.await
}
pub fn fetch_income_history<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a WeexIncomeRuntime<H, V, E>,
    opts: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        if (r.public_only)(h)? {
            return Ok((r.array)(vec![]));
        }
        let factory = (r.request_factory)(h)?;
        let symbol = (r.optional_get)(h, &opts, "symbol")?;
        let asset = if (r.truthy)(&symbol) {
            (r.undefined)()
        } else {
            let kind = (r.optional_get)(h, &opts, "incomeType")?;
            if (r.truthy)(&kind) {
                (r.undefined)()
            } else {
                (r.text)("")
            }
        };
        let symbol = (r.optional_get)(h, &opts, "symbol")?;
        let symbol = if (r.truthy)(&symbol) {
            symbol
        } else {
            (r.text)("")
        };
        let kind = (r.optional_get)(h, &opts, "incomeType")?;
        let kind = if (r.truthy)(&kind) {
            kind
        } else {
            (r.text)("")
        };
        let start = (r.optional_get)(h, &opts, "startTime")?;
        let start = if (r.nullish)(&start) {
            (r.null)()
        } else {
            start
        };
        let end = (r.optional_get)(h, &opts, "endTime")?;
        let end = if (r.nullish)(&end) { (r.null)() } else { end };
        let limit = (r.optional_get)(h, &opts, "limit")?;
        let limit = if (r.truthy)(&limit) {
            limit
        } else {
            (r.number)(100.0)
        };
        let limit = (r.numeric)(h, limit)?;
        let limit = safe_cex_num::math_min(safe_cex_num::math_max(limit, 1.0), 100.0);
        let payload = (r.object)(vec![
            ("asset", asset),
            ("symbol", symbol),
            ("incomeType", kind),
            ("startTime", start),
            ("endTime", end),
            ("limit", (r.number)(limit)),
        ]);
        let endpoint = ENDPOINTS.iter().find(|(k, _)| *k == "INCOME").unwrap().1;
        let response = js_await((r.post)(h, factory, endpoint, payload)?).await?;
        let data = (r.get)(h, &response, "data")?;
        let rows = (r.extract_array)(h, data, &["items"])?;
        (r.map)(h, &rows, Box::new(move |row| (r.normalize)(h, row)))
    }))
}
pub fn fetch_income_history_range<H: 'static, V: Clone + 'static>(
    h: Rc<H>,
    r: &WeexIncomeRangeRuntime<H, V>,
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
    start: f64,
    end: f64,
    income_type: V,
) -> ExchangeFuture<'static, Vec<IncomeRecord>> {
    let WeexIncomeRangeRuntime {
        fetch_page,
        sleep,
        warning_sink,
        warning,
        iso_date,
    } = *r;
    safe_cex_exchange::eager::start_eager(&runtime.clone(), async move {
        let page_host = h.clone();
        let page = move |params: IncomePageParams| -> ExchangeFuture<'static, Vec<IncomeRecord>> {
            let income_type = income_type.clone();
            let h = page_host.clone();
            eager(Box::pin(async move {
                let records = js_await(fetch_page(&h, params, income_type)?).await?;
                js_await(sleep(&h, 100.0)?).await?;
                Ok(records)
            }))
        };
        let delay_host = h.clone();
        let delay = move |ms| {
            let h = delay_host.clone();
            Box::pin(async move { sleep(&h, ms)?.await }) as ExchangeFuture<'static, ()>
        };
        let options = IncomeRangeOptions {
            start_time: start,
            end_time: end,
            chunk_ms: 8_640_000_000.0,
            page_limit: 100.0,
            chunk_delay_ms: None,
            max_pages_per_chunk: None,
        };
        let pager = paginate_income_range(runtime, Box::new(page), options, Box::new(delay));
        let result = js_await(pager).await?;
        if result.truncated {
            let sink = warning_sink(&h)?;
            let mut message: Vec<u16> = "Income history for ".encode_utf16().collect();
            message.extend(iso_date(&h, start)?);
            message.extend("..".encode_utf16());
            message.extend(iso_date(&h, end)?);
            message.extend(" is INCOMPLETE — the page budget ran out on ".encode_utf16());
            message.extend(safe_cex_num::js_number_string(result.exhausted_chunks).encode_utf16());
            message.extend(" chunk(s). Any total derived from it is a lower bound.".encode_utf16());
            warning(&h, sink, message)?;
        }
        Ok(result.records)
    })
}
