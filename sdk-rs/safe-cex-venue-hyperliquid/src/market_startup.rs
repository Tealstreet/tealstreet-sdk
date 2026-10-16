//! Primary/fallback and HIP-3 retry request bodies over the mapped tuple parser.
use crate::{
    api_errors::{ApiErrorClass, ApiErrorHost},
    income::IncomeFuture,
    json_boundaries::JsPayloadHost,
    market_helpers::{MetaTupleHost, parse_meta_and_asset_ctxs_response},
    spot_json::JsString,
};
pub trait StartupHost: MetaTupleHost {
    fn object(&mut self) -> Result<Self::Value, Self::Error>;
    fn null(&self) -> Self::Value;
    fn tuple(&mut self, values: &[Self::Value; 2]) -> Result<Self::Value, Self::Error>;
    fn string_value(&self, text: &JsString) -> Self::Value;
    fn call(
        &mut self,
        function: &Self::Value,
        receiver: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn await_value(&mut self, value: Self::Value) -> IncomeFuture<'_, Self::Value, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    /// Native Promise returned by the local async fetch helper, with adoption.
    fn helper_promise(&mut self, result: Result<Self::Value, Self::Error>) -> Self::Value;
    fn info_endpoint(&mut self) -> Result<Self::Value, Self::Error>;
    fn meta_info_type(&mut self) -> Result<Self::Value, Self::Error>;
    fn request_config(&self) -> Self::Value;
    fn warning_severity(&mut self) -> Result<Self::Value, Self::Error>;
}
fn text(v: &str) -> JsString {
    JsString(v.encode_utf16().collect())
}
fn join(parts: &[&[u16]]) -> JsString {
    JsString(parts.iter().flat_map(|p| p.iter().copied()).collect())
}
fn log<H: StartupHost>(h: &mut H, logger: &H::Value, msg: &JsString) -> Result<(), H::Error> {
    let msg = h.string_value(msg);
    let severity = h.warning_severity()?;
    h.call(logger, &h.undefined(), &[msg, severity])?;
    Ok(())
}
struct Logged<'a, H: StartupHost> {
    inner: &'a mut H,
    logger: H::Value,
}
impl<H: StartupHost> JsPayloadHost for Logged<'_, H> {
    type Value = H::Value;
    type Error = H::Error;
    fn string_units<'a>(&self, v: &'a H::Value) -> Option<&'a [u16]> {
        self.inner.string_units(v)
    }
    fn is_object(&self, v: &H::Value) -> bool {
        self.inner.is_object(v)
    }
    fn is_array(&mut self, v: &H::Value) -> Result<bool, H::Error> {
        self.inner.is_array(v)
    }
    fn is_nullish(&self, v: &H::Value) -> bool {
        self.inner.is_nullish(v)
    }
    fn undefined(&self) -> H::Value {
        self.inner.undefined()
    }
    fn parse_json(&mut self, v: &[u16]) -> Result<H::Value, H::Error> {
        self.inner.parse_json(v)
    }
    fn get(&mut self, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
        self.inner.get(v, k)
    }
    fn json_stringify(&mut self, v: &H::Value) -> Result<Option<JsString>, H::Error> {
        self.inner.json_stringify(v)
    }
    fn to_string(&mut self, v: &H::Value) -> Result<JsString, H::Error> {
        self.inner.to_string(v)
    }
}
impl<H: StartupHost> ApiErrorHost for Logged<'_, H> {
    fn new_error(&mut self, c: ApiErrorClass, v: &H::Value) -> Result<H::Value, H::Error> {
        self.inner.new_error(c, v)
    }
    fn define_field(&mut self, o: &H::Value, k: &str, v: &H::Value) -> Result<(), H::Error> {
        self.inner.define_field(o, k, v)
    }
    fn set_property(&mut self, o: &H::Value, k: &str, v: &H::Value) -> Result<(), H::Error> {
        self.inner.set_property(o, k, v)
    }
    fn literal(&self, v: &str) -> H::Value {
        self.inner.literal(v)
    }
    fn truthy(&self, v: &H::Value) -> bool {
        self.inner.truthy(v)
    }
    fn exception_from_value(&self, v: H::Value) -> H::Error {
        self.inner.exception_from_value(v)
    }
}
impl<H: StartupHost> MetaTupleHost for Logged<'_, H> {
    fn first_two(&mut self, v: &H::Value) -> Result<[H::Value; 2], H::Error> {
        self.inner.first_two(v)
    }
    fn type_of(&self, v: &H::Value) -> &'static str {
        self.inner.type_of(v)
    }
    fn implicit_string(&mut self, v: &H::Value) -> Result<JsString, H::Error> {
        self.inner.implicit_string(v)
    }
    fn log_warning(&mut self, msg: &JsString) -> Result<(), H::Error> {
        log(self.inner, &self.logger, msg)
    }
}
async fn fetch_source<H: StartupHost>(
    h: &mut H,
    client: &H::Value,
    body: &H::Value,
    logger: &H::Value,
    source: &JsString,
    context: &JsString,
    retry_hint: Option<&JsString>,
    failure_prefix: &JsString,
) -> Result<Option<H::Value>, H::Error> {
    let attempt = async {
        let post = h.get(client, "post")?;
        let endpoint = h.info_endpoint()?;
        let config = h.request_config();
        let promise = h.call(&post, client, &[endpoint, body.clone(), config])?;
        let response = h.await_value(promise).await?;
        let data = h.get(&response, "data")?;
        let mut parser = Logged {
            inner: h,
            logger: logger.clone(),
        };
        let parsed = parse_meta_and_asset_ctxs_response(&mut parser, &data, source, context)?;
        if let Some([meta, assets]) = parsed {
            return parser.inner.tuple(&[meta, assets]).map(Some);
        }
        if let Some(message) = retry_hint {
            log(parser.inner, logger, message)?;
        }
        Ok(None)
    }
    .await;
    match attempt {
        Ok(Some(tuple)) => Ok(Some(tuple)),
        Ok(None) => Ok(None),
        Err(error) => {
            let error = h.error_value(&error);
            let error = h.implicit_string(&error)?;
            log(h, logger, &join(&[&failure_prefix.0, &error.0]))?;
            Ok(None)
        }
    }
}
async fn fetch_core_body<H: StartupHost>(
    h: &mut H,
    input: &H::Value,
) -> Result<H::Value, H::Error> {
    let primary = h.get(input, "infoXhr")?;
    let fallback = h.get(input, "xhr")?;
    let logger = h.get(input, "log")?;
    let body = h.object()?;
    let kind = h.meta_info_type()?;
    h.define_field(&body, "type", &kind)?;
    let context = text("core markets");
    let hint = text("Retrying core markets fetch via fallback due to malformed response");
    for (client, label, hint) in [
        (&primary, "primary", Some(&hint)),
        (&fallback, "fallback", None),
    ] {
        let prefix = text(&format!("Failed to fetch core markets via {label}: "));
        let fetched = fetch_source(
            h,
            client,
            &body,
            &logger,
            &text(label),
            &context,
            hint,
            &prefix,
        )
        .await;
        let fetched = fetched.map(|value| value.unwrap_or_else(|| h.null()));
        let promise = h.helper_promise(fetched);
        let fetched = h.await_value(promise).await?;
        if h.truthy(&fetched) {
            return Ok(fetched);
        }
    }
    let msg =
        h.literal("Failed to fetch valid core markets response from primary and fallback sources");
    let error = h.new_error(ApiErrorClass::Intrinsic, &msg)?;
    Err(h.exception_from_value(error))
}
async fn fetch_hip3_body<H: StartupHost>(
    h: &mut H,
    input: &H::Value,
) -> Result<H::Value, H::Error> {
    let xhr = h.get(input, "xhr")?;
    let dex = h.get(input, "dexName")?;
    let logger = h.get(input, "log")?;
    let body = h.object()?;
    let kind = h.meta_info_type()?;
    h.define_field(&body, "type", &kind)?;
    h.define_field(&body, "dex", &dex)?;
    // Template coercion remains per attempt and inside its original try/catch.
    for attempt in 1..=2 {
        let fetched = async {
            let post = h.get(&xhr, "post")?;
            let endpoint = h.info_endpoint()?;
            let config = h.request_config();
            let promise = h.call(&post, &xhr, &[endpoint, body.clone(), config])?;
            let response = h.await_value(promise).await?;
            let data = h.get(&response, "data")?;
            let source = text(&format!("official API (attempt {attempt})"));
            let dex_text = h.implicit_string(&dex)?;
            let context = join(&[&text("HIP-3 markets (").0, &dex_text.0, &text(")").0]);
            let mut parser = Logged {
                inner: h,
                logger: logger.clone(),
            };
            let parsed = parse_meta_and_asset_ctxs_response(&mut parser, &data, &source, &context)?;
            if let Some(pair) = parsed {
                return parser.inner.tuple(&pair).map(Some);
            }
            if attempt == 1 {
                let dex = parser.inner.implicit_string(&dex)?;
                log(
                    parser.inner,
                    &logger,
                    &join(&[
                        &text("Retrying HIP-3 markets fetch for ").0,
                        &dex.0,
                        &text(" due to malformed response").0,
                    ]),
                )?;
            }
            Ok(None)
        }
        .await;
        let fetched = match fetched {
            Ok(value) => Ok(value.unwrap_or_else(|| h.null())),
            Err(error) => (|| {
                let dex = h.implicit_string(&dex)?;
                let label = if attempt == 1 {
                    "initial attempt"
                } else {
                    "retry"
                };
                let prefix = text(&format!(" ({label}): "));
                let error = h.error_value(&error);
                let error = h.implicit_string(&error)?;
                log(
                    h,
                    &logger,
                    &join(&[
                        &text("Failed to fetch HIP-3 markets for ").0,
                        &dex.0,
                        &prefix.0,
                        &error.0,
                    ]),
                )?;
                Ok(h.null())
            })(),
        };
        let promise = h.helper_promise(fetched);
        let fetched = h.await_value(promise).await?;
        if h.truthy(&fetched) {
            return Ok(fetched);
        }
    }
    let dex = h.implicit_string(&dex)?;
    log(
        h,
        &logger,
        &join(&[
            &text("Failed to fetch valid HIP-3 markets for ").0,
            &dex.0,
            &text(" after retry").0,
        ]),
    )?;
    Ok(h.null())
}

pub fn fetch_core_meta_and_asset_ctxs_with_fallback<H>(
    host: H,
    input: H::Value,
) -> crate::helper_tasks::HelperObserver<H::Value, H::Error>
where
    H: StartupHost + crate::helper_tasks::HelperTaskHost<H::Value, H::Error>,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut worker = host.clone();
    crate::helper_tasks::start(
        &host,
        async move { fetch_core_body(&mut worker, &input).await },
    )
}
pub fn fetch_hip3_meta_and_asset_ctxs_with_retry<H>(
    host: H,
    input: H::Value,
) -> crate::helper_tasks::HelperObserver<H::Value, H::Error>
where
    H: StartupHost + crate::helper_tasks::HelperTaskHost<H::Value, H::Error>,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut worker = host.clone();
    crate::helper_tasks::start(
        &host,
        async move { fetch_hip3_body(&mut worker, &input).await },
    )
}
