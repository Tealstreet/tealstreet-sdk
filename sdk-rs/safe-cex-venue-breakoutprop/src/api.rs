//! DXtrade API initialization and the request/one-shot session retry interceptors.
//! The host binds Axios creation, retry installation, and optional tunnel transport.
use crate::{
    lifecycle::{BreakoutpropExchange, DxClient},
    types::{BASE_URL, RECV_WINDOW},
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_transport::{
    provider::RestRequest,
    query::{QueryArrayFormat, QueryProfile, render_query},
    retry::{ReadClassification, reads_and_posted},
};
use safe_cex_types::{ExchangeOptions, QueryValue, model_utils::replace_url_params};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub struct ApiDefaults {
    pub base_url: &'static str,
    pub timeout: f64,
    pub content_type: &'static str,
    pub serialize_params: fn(&QueryValue) -> String,
}
pub struct DxConfig {
    pub request: RestRequest,
    pub session_retried: bool,
}
pub type Config = Rc<RefCell<DxConfig>>;
pub struct ApiFailure {
    pub error: ExchangeError,
    pub status: Option<f64>,
    pub config: Option<Config>,
}
pub trait ApiWire {
    fn install_request_interceptor(&self, api: Weak<DxApi>) -> Result<(), ExchangeError>;
    fn rate_limit(&self, max_rps: f64) -> Result<(), ExchangeError>;
    fn request(
        &self,
        config: Config,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<JsNumberInput, ApiFailure>>>>;
}
pub trait ApiHost {
    fn create(
        &self,
        defaults: ApiDefaults,
        options: &ExchangeOptions,
        exchange: &Rc<BreakoutpropExchange>,
    ) -> Result<Rc<dyn ApiWire>, ExchangeError>;
    fn install_network_retry(
        &self,
        wire: &Rc<dyn ApiWire>,
        retries: usize,
        reads: ReadClassification,
    ) -> Result<(), ExchangeError>;
    fn install_transport_adapter(
        &self,
        wire: &Rc<dyn ApiWire>,
        options: &ExchangeOptions,
        exchange: &Rc<BreakoutpropExchange>,
    ) -> Result<(), ExchangeError>;
}
pub struct DxApi {
    owner: Weak<BreakoutpropExchange>,
    wire: Rc<dyn ApiWire>,
    options: ExchangeOptions,
    this: Weak<Self>,
}
fn serialize_params(params: &QueryValue) -> String {
    render_query(
        params,
        QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: QueryArrayFormat::Indexed,
            encode: true,
        },
    )
    .expect("Preserve query order does not initialize collation")
}
// ts: createAPI, initialization order matches create -> retry -> optional adapter.
pub fn create_api(
    host: &dyn ApiHost,
    options: ExchangeOptions,
    exchange: &Rc<BreakoutpropExchange>,
) -> Result<Rc<DxApi>, ExchangeError> {
    let defaults = ApiDefaults {
        base_url: BASE_URL,
        timeout: options
            .extra
            .as_ref()
            .and_then(|e| e.recv_window)
            .unwrap_or(RECV_WINDOW),
        content_type: "application/json",
        serialize_params,
    };
    let wire = host.create(defaults, &options, exchange)?;
    host.install_network_retry(
        &wire,
        3,
        reads_and_posted(vec!["/login".into(), "marketdata".into()]),
    )?;
    if options.tealstreet_tunnel == Some(true) {
        host.install_transport_adapter(&wire, &options, exchange)?;
    }
    let api = Rc::new_cyclic(|this| DxApi {
        owner: Rc::downgrade(exchange),
        wire: wire.clone(),
        options,
        this: this.clone(),
    });
    wire.install_request_interceptor(Rc::downgrade(&api))?;
    Ok(api)
}
impl DxApi {
    pub fn intercept_request(&self, config: &mut DxConfig) -> Result<(), ExchangeError> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("DXtrade exchange dropped"))?;
        let token = owner.session_token.borrow().clone();
        if config.request.url != "/login" && token.as_ref().is_none_or(String::is_empty) {
            return Err(ExchangeError::new("No session token"));
        }
        let params = config
            .request
            .params
            .as_ref()
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default();
        let (url, params) =
            replace_url_params(Some(&config.request.url), &params).map_err(|e| {
                let mut error = ExchangeError::new(e.message);
                error.name = e.name.into();
                error
            })?;
        config.request.url = url.unwrap_or_default();
        config.request.params = Some(params.into());
        let key = if self.options.tealstreet_tunnel == Some(true) {
            "Authorization"
        } else {
            "Coinbase-Authorization"
        };
        // AxiosHeaders keeps first spelling and last case-insensitive value.
        config.request.headers.insert(
            key.into(),
            format!("DXAPI {}", token.as_deref().unwrap_or("null")),
        );
        config.request.merge_axios_headers(&Default::default());
        Ok(())
    }
    pub fn request_config(
        self: &Rc<Self>,
        config: Config,
    ) -> ExchangeFuture<'static, JsNumberInput> {
        let api = self.clone();
        Box::pin(async move {
            let mut current = config;
            loop {
                match api.wire.request(current.clone()).await {
                    Ok(data) => return Ok(data),
                    Err(failure) => {
                        let eligible = failure.status == Some(401.)
                            && failure.config.as_ref().is_some_and(|c| {
                                let c = c.borrow();
                                c.request.url != "/login" && !c.session_retried
                            });
                        if !eligible {
                            return Err(failure.error);
                        }
                        let retry = failure.config.unwrap();
                        retry.borrow_mut().session_retried = true;
                        let owner = api
                            .owner
                            .upgrade()
                            .ok_or_else(|| ExchangeError::new("DXtrade exchange dropped"))?;
                        owner.invalidate_session();
                        owner.get_session_token().await?;
                        current = retry;
                    }
                }
            }
        })
    }
}
impl DxClient for DxApi {
    fn rate_limit(&self, max_rps: f64) -> Result<(), ExchangeError> {
        self.wire.rate_limit(max_rps)
    }
    fn request(&self, request: RestRequest) -> ExchangeFuture<'static, JsNumberInput> {
        self.this
            .upgrade()
            .expect("live API")
            .request_config(Rc::new(RefCell::new(DxConfig {
                request,
                session_retried: false,
            })))
    }
}
