//! GRVT account bootstrapping, authorization and REST snapshot bodies.
use crate::{
    builder_workflow::{JsPromise, JsResult},
    session_host::{SessionHost, optional, session_template},
};
use safe_cex_exchange::runtime::LogSeverity;
use std::rc::Rc;
pub type Iteration<'a, V> = dyn FnMut(V) -> JsResult<V, bool> + 'a;

/// for_of binds live iteration and IteratorClose on break/error; true breaks the loop.
/// loop_after retains callbacks; start owns eager bodies and adopts returns after local catches.
/// Helpers bind their existing source implementations without narrowing opaque values.
pub trait AccountHost: SessionHost {
    fn binding(&self) -> Rc<Self>;
    fn set_indexed(
        &self,
        target: &Self::Value,
        key: Self::Value,
        value: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn number_value(&self, value: Self::Value) -> JsResult<Self::Value, f64>;
    fn greater_than_zero(&self, value: Self::Value) -> JsResult<Self::Value, bool>;
    fn symbol_set(&self, values: Self::Value) -> JsResult<Self::Value>;
    fn has_symbol(&self, set: &Self::Value, value: Self::Value) -> JsResult<Self::Value, bool>;
    fn append_iterable(
        &self,
        target: &Self::Value,
        values: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn exchange(&self) -> Self::Value;
    fn endpoint(&self, name: &str) -> Self::Value;
    fn severity(&self, severity: LogSeverity) -> Self::Value;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn indexed(&self, value: &Self::Value, key: &Self::Value) -> JsResult<Self::Value>;
    fn parse_float(&self, value: Self::Value) -> JsResult<Self::Value, f64>;
    fn parse_int(&self, value: Self::Value) -> JsResult<Self::Value, f64>;
    fn js_string(&self, value: Self::Value) -> JsResult<Self::Value>;
    fn add(&self, a: Self::Value, b: Self::Value) -> JsResult<Self::Value>;
    fn now(&self) -> JsResult<Self::Value, f64>;
    fn virtual_now(&self) -> JsResult<Self::Value, f64>;
    fn for_of(
        &self,
        value: Self::Value,
        visitor: &mut Iteration<'_, Self::Value>,
    ) -> JsResult<Self::Value, ()>;
    fn loop_after(
        &self,
        callback: Rc<dyn Fn() -> JsResult<Self::Value>>,
        delay: f64,
    ) -> JsResult<Self::Value, ()>;
    fn sleep(&self, delay: f64) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn builder_address(&self, testnet: Self::Value) -> JsResult<Self::Value>;
    fn fee_to_raw(&self, fee: Self::Value) -> JsResult<Self::Value, f64>;
    fn builder_info(&self, testnet: Self::Value) -> JsResult<Self::Value>;
    fn builder_error(&self, opts: Self::Value) -> JsResult<Self::Value>;
    fn position_id(&self, symbol: Self::Value) -> JsResult<Self::Value>;
    fn trigger_metadata(&self, order: Self::Value) -> JsResult<Self::Value>;
    fn time_in_force(&self, order: Self::Value) -> JsResult<Self::Value>;
    fn keys_join(&self, value: Self::Value) -> JsResult<Self::Value>;
}
fn field<H: AccountHost>(h: &H, key: &str) -> JsResult<H::Value> {
    h.get(&h.exchange(), key)
}
fn store_field<H: AccountHost>(h: &H, key: &str) -> JsResult<H::Value> {
    h.get(&field(h, "store")?, key)
}
fn opt_field<H: AccountHost>(h: &H, key: &str) -> JsResult<H::Value> {
    h.get(&field(h, "options")?, key)
}
fn invoke<H: AccountHost>(
    h: &H,
    receiver: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> JsResult<H::Value> {
    let method = h.get(&receiver, key)?;
    h.call(method, receiver, args)
}
fn own_call<H: AccountHost>(h: &H, key: &str, args: Vec<H::Value>) -> JsResult<H::Value> {
    invoke(h, h.exchange(), key, args)
}
async fn own_await<H: AccountHost>(h: &H, key: &str, args: Vec<H::Value>) -> JsResult<H::Value> {
    h.await_value(own_call(h, key, args)?).await
}
fn log_dynamic<H: AccountHost>(
    h: &H,
    make: impl FnOnce() -> JsResult<H::Value>,
    severity: impl FnOnce() -> JsResult<H::Value>,
) -> JsResult<H::Value, ()> {
    let parent = h.exchange();
    let logger = h.get(&parent, "log")?;
    h.call(logger, parent, vec![make()?, severity()?])?;
    Ok(())
}
fn log<H: AccountHost>(
    h: &H,
    make: impl FnOnce() -> JsResult<H::Value>,
    severity: LogSeverity,
) -> JsResult<H::Value, ()> {
    log_dynamic(h, make, || Ok(h.severity(severity)))
}
fn or<H: AccountHost>(h: &H, v: &H::Value, keys: &[&str]) -> JsResult<H::Value> {
    for (i, key) in keys.iter().enumerate() {
        let value = h.get(v, key)?;
        if h.truthy(&value) || i == keys.len() - 1 {
            return Ok(value);
        }
    }
    Ok(h.undefined())
}
fn fallback<H: AccountHost>(h: &H, v: H::Value, text: &str) -> H::Value {
    if h.truthy(&v) { v } else { h.string(text) }
}
fn null_default<H: AccountHost>(
    h: &H,
    v: H::Value,
    default: impl FnOnce() -> JsResult<H::Value>,
) -> JsResult<H::Value> {
    if h.nullish(&v) { default() } else { Ok(v) }
}
fn loaded_update<H: AccountHost>(
    h: &H,
    fields: Vec<(&str, H::Value)>,
    flags: &[&str],
) -> JsResult<H::Value, ()> {
    let store = field(h, "store")?;
    let update = h.get(&store, "update")?;
    let loaded = h.object(vec![]);
    h.spread(&loaded, store_field(h, "loaded")?)?;
    for key in flags {
        h.define(&loaded, key, h.boolean(true))?;
    }
    let changes = h.object(fields);
    h.define(&changes, "loaded", loaded)?;
    h.call(update, store, vec![changes])?;
    Ok(())
}
fn catch_log<H: AccountHost>(
    h: &H,
    error: H::Value,
    prefix: &str,
    severity: LogSeverity,
) -> JsResult<H::Value> {
    log(
        h,
        || session_template!(h;h.string(prefix),error.clone()),
        severity,
    )?;
    Err(error)
}
/// ts: GrvtExchange.tick.
pub fn tick<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 if !h.truthy(&field(h.as_ref(),"isDisposed")?){
  if h.truthy(&field(h.as_ref(),"isPublicDataOnly")?){return Ok(h.undefined());}
  let result:JsResult<H::Value,bool>=async{
   let balance=own_await(h.as_ref(),"fetchBalance",vec![]).await?;
   if h.truthy(&field(h.as_ref(),"isDisposed")?){return Ok(true);}
   loaded_update(h.as_ref(),vec![("balance",balance)],&["balance"])?;
   own_call(h.as_ref(),"syncBalanceUpnlFromPositions",vec![])?;
   log(h.as_ref(),||Ok(h.string("Balance refreshed via tick()")),LogSeverity::Debug)?;Ok(false)
  }.await;
  match result{Ok(true)=>return Ok(h.undefined()),Ok(false)=>{},Err(error)=>log(h.as_ref(),||session_template!(h;h.string("Error in tick: "),optional(h.as_ref(),error,"message")?),LogSeverity::Warning)?}
  let next=h.clone();h.loop_after(Rc::new(move||own_call(next.as_ref(),"tick",vec![])),60000.)?;
 }
 Ok(h.undefined())
}))
}
/// ts: GrvtExchange.authenticate.
pub fn authenticate<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let result:JsResult<H::Value>=async{
  let edge=field(h.as_ref(),"edgeXhr")?;let post=h.get(&edge,"post")?;
  let response=h.call(post,edge,vec![h.endpoint("AUTH"),h.object(vec![("api_key",opt_field(h.as_ref(),"key")?)]),h.object(vec![("headers",h.object(vec![("X-Grvt-Remember-Me",h.string("true"))]))])])?;
  h.await_value(response).await?;
  if h.truthy(&optional(h.as_ref(),field(h.as_ref(),"apiExtras")?,"sessionCookie")?)&&h.truthy(&optional(h.as_ref(),field(h.as_ref(),"apiExtras")?,"accountId")?){
   log(h.as_ref(),||Ok(h.string("Successfully authenticated with GRVT")),LogSeverity::Info)?;
   if h.truthy(&field(h.as_ref(),"grvtClient")?){
    let client=field(h.as_ref(),"grvtClient")?;
    let cookie=h.object(vec![("gravity",h.get(&field(h.as_ref(),"apiExtras")?,"sessionCookie")?),("expires",h.number(h.now()?/1000.+3600.)),("XGrvtAccountId",h.get(&field(h.as_ref(),"apiExtras")?,"accountId")?)]);
    h.set(&client,"cookie",cookie)?;
   }
   Ok(h.boolean(true))
  }else{log(h.as_ref(),||Ok(h.string("Authentication response did not include required cookies/headers")),LogSeverity::Error)?;Ok(h.boolean(false))}
 }.await;
 match result{Ok(v)=>Ok(v),Err(error)=>{log(h.as_ref(),||session_template!(h;h.string("Authentication failed: "),h.get(&error,"message")?),LogSeverity::Error)?;Err(error)}}
}))
}
/// ts: GrvtExchange.validateAccount.
pub fn validate_account<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let result: JsResult<H::Value> = async {
            own_await(h.as_ref(), "authenticate", vec![]).await?;
            if !h.truthy(&field(h.as_ref(), "grvtClient")?) {
                return Err(h.error(h.string("GRVT SDK client not initialized"))?);
            }
            let client = field(h.as_ref(), "grvtClient")?;
            let response = h
                .await_value(invoke(
                    h.as_ref(),
                    client,
                    "getFundingAccountSummary",
                    vec![],
                )?)
                .await?;
            if h.truthy(&response) {
                log(
                    h.as_ref(),
                    || Ok(h.string("Account validated successfully")),
                    LogSeverity::Info,
                )?;
                Ok(h.object(vec![("error", h.string(""))]))
            } else {
                Ok(h.object(vec![("error", h.string("Failed to validate account"))]))
            }
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(error) => {
                let message = fallback(
                    h.as_ref(),
                    h.get(&error, "message")?,
                    "Invalid API credentials",
                );
                own_call(h.as_ref(), "emitAndLogError", vec![message.clone()])?;
                Ok(h.object(vec![("error", message)]))
            }
        }
    }))
}
/// ts: GrvtExchange#fetchBuilderAuthorization.
pub fn fetch_builder_authorization<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 if !h.truthy(&opt_field(h.as_ref(),"key")?){h.set(&h.exchange(),"_builderAuthState",h.object(vec![("status",h.string("unauthorized"))]))?;return Ok(h.undefined());}
 if !h.truthy(&optional(h.as_ref(),field(h.as_ref(),"apiExtras")?,"sessionCookie")?){
  log(h.as_ref(),||Ok(h.string("No session cookie found, authenticating before builder authorization check...")),LogSeverity::Debug)?;
  let authenticated=own_await(h.as_ref(),"authenticate",vec![]).await?;
  if !h.truthy(&authenticated){log(h.as_ref(),||Ok(h.string("Authentication failed, cannot check builder authorization")),LogSeverity::Warning)?;h.set(&h.exchange(),"_builderAuthState",h.object(vec![("status",h.string("unknown"))]))?;return Ok(h.undefined());}
 }
 let testnet=null_default(h.as_ref(),opt_field(h.as_ref(),"testnet")?,||Ok(h.boolean(false)))?;
 let builder_address=h.builder_address(testnet)?;
 for attempt in 1..=3{
  let result:JsResult<H::Value,()>=async{
   log(h.as_ref(),||session_template!(h;h.string("Checking builder authorization for "),builder_address.clone(),h.string(" (attempt "),h.number(attempt as f64),h.string("/3)")),LogSeverity::Debug)?;
   let xhr=field(h.as_ref(),"tradingXhr")?;let post=h.get(&xhr,"post")?;let response=h.await_value(h.call(post,xhr,vec![h.endpoint("GET_AUTHORIZED_BUILDERS"),h.object(vec![])])?).await?;
   let data=h.get(&response,"data")?;let full=optional(h.as_ref(),data.clone(),"results")?;
   let builders=if h.truthy(&full){full}else{fallback_array(h.as_ref(),optional(h.as_ref(),data,"r")?)};
   log(h.as_ref(),||session_template!(h;h.string("get_authorized_builders returned "),h.get(&builders,"length")?,h.string(" builder(s)")),LogSeverity::Debug)?;
   let required=h.fee_to_raw(h.string(crate::builder::DEFAULT_GRVT_BUILDER_FEE_RATE))?;
   let mut next=h.object(vec![("status",h.string("unauthorized"))]);
   h.for_of(builders,&mut |builder|{
    let addr=fallback(h.as_ref(),or(h.as_ref(),&builder,&["builder_account_id","ba"] )?,"");
    let addr=invoke(h.as_ref(),addr,"toLowerCase",vec![])?;
    let expected=invoke(h.as_ref(),builder_address.clone(),"toLowerCase",vec![])?;
    if !h.equal(&addr,&expected){return Ok(false);}
    let raw=null_default(h.as_ref(),h.get(&builder,"max_futures_fee_rate")?,||null_default(h.as_ref(),h.get(&builder,"max_fee_rate")?,||h.get(&builder,"mfr")))?;
    let authorized=if h.nullish(&raw){None}else{(||{let text=h.js_string(raw)?;h.fee_to_raw(text)})().ok()};
    if let Some(raw)=authorized.filter(|raw|*raw<required){
     log(h.as_ref(),||session_template!(h;h.string("Builder "),builder_address.clone(),h.string(" authorized cap ("),h.number(raw),h.string(") is below required ("),h.number(required),h.string("). Re-authorization is required before orders can be placed.")),LogSeverity::Warning)?;
     next=h.object(vec![("status",h.string("cap-too-low")),("authorizedRaw",h.number(raw))]);
    }else{next=h.object(vec![("status",h.string("authorized"))]);}
    Ok(true)
   })?;
   h.set(&h.exchange(),"_builderAuthState",next.clone())?;
   log_dynamic(h.as_ref(),||session_template!(h;h.string("Builder authorization status: "),h.get(&next,"status")?),||Ok(h.severity(if h.equal(&h.get(&next,"status")?,&h.string("authorized")){LogSeverity::Info}else{LogSeverity::Warning})))?;
   Ok(())
  }.await;
  match result{Ok(())=>return Ok(h.undefined()),Err(error)=>{
   log(h.as_ref(),||session_template!(h;h.string("Failed to check builder authorization (attempt "),h.number(attempt as f64),h.string("/3): "),error),LogSeverity::Warning)?;
   if attempt<3{h.sleep(2000.*attempt as f64)?.await?;}else{h.set(&h.exchange(),"_builderAuthState",h.object(vec![("status",h.string("unknown"))]))?;}
  }}
 }
 Ok(h.undefined())
}))
}
fn fallback_array<H: AccountHost>(h: &H, value: H::Value) -> H::Value {
    if h.truthy(&value) {
        value
    } else {
        h.array(vec![])
    }
}
/// ts: GrvtExchange#getBuilderInfo.
pub fn get_builder_info<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        if h.equal(
            &h.get(&field(h.as_ref(), "_builderAuthState")?, "status")?,
            &h.string("unknown"),
        ) {
            own_await(h.as_ref(), "fetchBuilderAuthorization", vec![]).await?;
        }
        let required = h.fee_to_raw(h.string(crate::builder::DEFAULT_GRVT_BUILDER_FEE_RATE))?;
        if h.equal(
            &h.get(&field(h.as_ref(), "_builderAuthState")?, "status")?,
            &h.string("authorized"),
        ) {
            return h.builder_info(null_default(
                h.as_ref(),
                opt_field(h.as_ref(), "testnet")?,
                || Ok(h.boolean(false)),
            )?);
        }
        let opts = if h.equal(
            &h.get(&field(h.as_ref(), "_builderAuthState")?, "status")?,
            &h.string("cap-too-low"),
        ) {
            h.object(vec![
                ("reason", h.string("cap-too-low")),
                ("requiredRaw", h.number(required)),
                (
                    "authorizedRaw",
                    h.get(&field(h.as_ref(), "_builderAuthState")?, "authorizedRaw")?,
                ),
            ])
        } else {
            h.object(vec![
                ("reason", h.string("unauthorized")),
                ("requiredRaw", h.number(required)),
            ])
        };
        Err(h.builder_error(opts)?)
    }))
}
/// ts: GrvtExchange.start.
pub fn start<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let result:JsResult<H::Value>=async{
  log(h.as_ref(),||Ok(h.string("Starting GRVT exchange...")),LogSeverity::Info)?;
  let external=own_await(h.as_ref(),"initExternalPublicData",vec![]).await?;
  let public_only=field(h.as_ref(),"isPublicDataOnly")?;
  if !h.truthy(&public_only)&&!h.truthy(&optional(h.as_ref(),field(h.as_ref(),"apiExtras")?,"sessionCookie")?){own_await(h.as_ref(),"authenticate",vec![]).await?;}
  if !h.truthy(&external){
   let markets=own_await(h.as_ref(),"fetchMarkets",vec![]).await?;loaded_update(h.as_ref(),vec![("markets",markets)],&["markets"])?;
   invoke(h.as_ref(),field(h.as_ref(),"publicWebsocket")?,"connectAndSubscribe",vec![])?;
   let tickers=own_await(h.as_ref(),"fetchTickers",vec![]).await?;loaded_update(h.as_ref(),vec![("tickers",tickers)],&["tickers"])?;
  }
  if h.truthy(&public_only){loaded_update(h.as_ref(),vec![],&["balance","positions","orders"])?;log(h.as_ref(),||Ok(h.string("GRVT exchange started in public-only mode")),LogSeverity::Info)?;return Ok(h.undefined());}
  let balance:JsResult<H::Value,()>=async{let value=own_await(h.as_ref(),"fetchBalance",vec![]).await?;loaded_update(h.as_ref(),vec![("balance",value)],&["balance"])}.await;
  if let Err(error)=balance{log(h.as_ref(),||session_template!(h;h.string("Error fetching balance: "),error),LogSeverity::Warning)?;loaded_update(h.as_ref(),vec![],&["balance"])?;}
  if !h.truthy(&external)&&h.truthy(&field(h.as_ref(),"grvtTradingAccountId")?){
   let parent=h.exchange();let method=h.get(&parent,"updateMarketLeverageLimits")?;let markets=store_field(h.as_ref(),"markets")?;let markets=h.await_value(h.call(method,parent,vec![markets])?).await?;loaded_update(h.as_ref(),vec![("markets",markets)],&["markets"])?;
  }
  if let Err(error)=own_await(h.as_ref(),"fetchBuilderAuthorization",vec![]).await{log(h.as_ref(),||session_template!(h;h.string("Builder authorization check failed: "),error),LogSeverity::Debug)?;}
  if h.truthy(&field(h.as_ref(),"privateWebsocket")?)&&!h.truthy(&h.get(&field(h.as_ref(),"privateWebsocket")?,"isConnected")?){
   log(h.as_ref(),||session_template!(h;h.string("Connecting private WebSocket with trading account ID: "),field(h.as_ref(),"grvtTradingAccountId")?),LogSeverity::Info)?;
   invoke(h.as_ref(),field(h.as_ref(),"privateWebsocket")?,"connectAndSubscribe",vec![])?;
  }
  let positions:JsResult<H::Value,()>=async{let value=own_await(h.as_ref(),"fetchPositions",vec![]).await?;loaded_update(h.as_ref(),vec![("positions",value.clone())],&["positions"])?;own_call(h.as_ref(),"syncBalanceUpnlFromPositions",vec![value])?;Ok(())}.await;
  if let Err(error)=positions{log(h.as_ref(),||session_template!(h;h.string("Error fetching positions: "),error),LogSeverity::Warning)?;loaded_update(h.as_ref(),vec![],&["positions"])?;}
  let orders:JsResult<H::Value,()>=async{let value=own_await(h.as_ref(),"fetchOpenOrders",vec![]).await?;loaded_update(h.as_ref(),vec![("orders",value)],&["orders"])}.await;
  if let Err(error)=orders{log(h.as_ref(),||session_template!(h;h.string("Error fetching orders: "),error),LogSeverity::Warning)?;loaded_update(h.as_ref(),vec![],&["orders"])?;}
  own_call(h.as_ref(),"tick",vec![])?;
  log(h.as_ref(),||Ok(h.string("GRVT exchange started successfully")),LogSeverity::Info)?;Ok(h.undefined())
 }.await;
 match result{Ok(v)=>Ok(v),Err(error)=>catch_log(h.as_ref(),error,"Error starting exchange: ",LogSeverity::Error)}
}))
}
/// ts: GrvtExchange.fetchBalance.
pub fn fetch_balance<H: AccountHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let result:JsResult<H::Value>=async{
  log(h.as_ref(),||Ok(h.string("fetchBalance - START")),LogSeverity::Debug)?;
  let currencies=h.object(vec![]);let mut equity=0.;let mut available=0.;let mut upnl=h.number(0.);
  let aggregate:JsResult<H::Value,()>=async{
   log(h.as_ref(),||Ok(h.string("fetchBalance - fetching aggregated account summary")),LogSeverity::Debug)?;
   let xhr=field(h.as_ref(),"tradingXhr")?;let post=h.get(&xhr,"post")?;
   let response=h.await_value(h.call(post,xhr,vec![h.string("/lite/v1/aggregated_account_summary"),h.object(vec![])])?).await?;
   let lite=optional(h.as_ref(),h.get(&response,"data")?,"r")?;
   let data=if h.truthy(&lite){lite}else{optional(h.as_ref(),h.get(&response,"data")?,"result")?};
   if h.truthy(&data){
    let spots=fallback_array(h.as_ref(),or(h.as_ref(),&data,&["sb","spot_balances"])?);
    h.for_of(spots,&mut |spot|{
     let balance=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&spot,&["b","balance"] )?,"0"))?;
     let currency=fallback(h.as_ref(),or(h.as_ref(),&spot,&["c","currency"] )?,"UNKNOWN");
     if !h.truthy(&h.indexed(&currencies,&currency)?){
      h.set_indexed(&currencies,currency.clone(),h.object(vec![("free",h.number(0.)),("used",h.number(0.)),("total",h.number(0.)),("upnl",h.number(0.))]))?;
     }
     let coin=h.indexed(&currencies,&currency)?;let free=h.add(h.get(&coin,"free")?,h.number(balance))?;h.set(&coin,"free",free)?;
     let coin=h.indexed(&currencies,&currency)?;let total=h.add(h.get(&coin,"total")?,h.number(balance))?;h.set(&coin,"total",total)?;
     Ok(false)
    })?;
    equity=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&data,&["te","total_equity"] )?,"0"))?;
    available=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&data,&["ts2","total_sub_account_available_balance","te","total_equity"] )?,"0"))?;
    let raw=null_default(h.as_ref(),h.get(&data,"tu")?,||h.get(&data,"total_upnl"))?;
    if !h.nullish(&raw){let parsed=h.parse_float(raw)?;if parsed.is_finite(){upnl=h.number(parsed);}}
    else{let parent=h.exchange();let calculate=h.get(&parent,"calculateUpnlFromPositions")?;let positions=store_field(h.as_ref(),"positions")?;let result=h.call(calculate,parent,vec![positions])?;upnl=result;}
    if !h.truthy(&field(h.as_ref(),"grvtTradingAccountId")?){
     let summaries=fallback_array(h.as_ref(),or(h.as_ref(),&data,&["sas","sub_account_summaries"] )?);
     if h.greater_than_zero(h.get(&summaries,"length")?)?{
      let first=h.get(&summaries,"0")?;let account=or(h.as_ref(),&first,&["si","sub_account_id"])?;
      if h.truthy(&account){let account=h.js_string(account)?;h.set(&h.exchange(),"grvtTradingAccountId",account)?;
       log(h.as_ref(),||session_template!(h;h.string("Auto-detected trading account ID: "),field(h.as_ref(),"grvtTradingAccountId")?),LogSeverity::Info)?;
      }
     }
    }
   }
   log(h.as_ref(),||session_template!(h;h.string("fetchBalance - received: equity="),h.number(equity),h.string(", available="),h.number(available)),LogSeverity::Debug)?;Ok(())
  }.await;
  if let Err(error)=aggregate{log(h.as_ref(),||session_template!(h;h.string("Error fetching aggregated account summary: "),h.get(&error,"message")?),LogSeverity::Error)?;return Err(error);}
  Ok(h.object(vec![("total",h.number(equity)),("free",h.number(if available>0.{available}else{equity})),("used",h.number(equity-available)),("upnl",upnl),("currencies",currencies)]))
 }.await;
 match result{Ok(value)=>Ok(value),Err(error)=>catch_log(h.as_ref(),error,"Error fetching balance: ",LogSeverity::Error)}
}))
}
/// ts: GrvtExchange.fetchPositions.
pub fn fetch_positions<H: AccountHost>(host: Rc<H>, symbols: H::Value) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let result:JsResult<H::Value>=async{
  if !h.truthy(&field(h.as_ref(),"grvtClient")?){return Err(h.error(h.string("GRVT SDK client not initialized"))?);}
  if !h.truthy(&field(h.as_ref(),"grvtTradingAccountId")?){log(h.as_ref(),||Ok(h.string("Skipping positions fetch - no trading account ID provided")),LogSeverity::Debug)?;return own_call(h.as_ref(),"getFakePositions",vec![]);}
  let instruments=if h.nullish(&symbols){h.undefined()}else{
   let map=h.get(&symbols,"map")?;let next=h.clone();let callback=h.function(Rc::new(move |symbol|own_call(next.as_ref(),"symbolToInstrumentId",vec![symbol])));h.call(map,symbols,vec![callback])?
  };
  let client=field(h.as_ref(),"grvtClient")?;let tdg=h.get(&client,"tdgClient")?;let method=h.get(&tdg,"positions")?;
  let request=h.object(vec![("sub_account_id",field(h.as_ref(),"grvtTradingAccountId")?)]);
  if h.truthy(&instruments)&&h.greater_than_zero(h.get(&instruments,"length")?)?{h.define(&request,"instrument_ids",instruments)?;}
  let endpoint=invoke(h.as_ref(),field(h.as_ref(),"grvtClient")?,"authenticatedEndpoint",vec![])?;let endpoint=h.await_value(endpoint).await?;
  let response=h.await_value(h.call(method,tdg,vec![request,endpoint])?).await?;
  let data=or(h.as_ref(),&response,&["r","result"])?;
  log(h.as_ref(),||session_template!(h;h.string("GRVT positions response: "),fallback(h.as_ref(),optional(h.as_ref(),data.clone(),"length")?,"0"),h.string(" rows")),LogSeverity::Debug)?;
  if !h.truthy(&data)||h.equal(&h.get(&data,"length")?,&h.number(0.)){return own_call(h.as_ref(),"getFakePositions",vec![]);}
  let mode=own_await(h.as_ref(),"getPositionMode",vec![]).await?;let positions=h.array(vec![]);
  h.for_of(data,&mut |position|{
   let instrument=or(h.as_ref(),&position,&["i","instrument"])?;let size=h.parse_float(or(h.as_ref(),&position,&["s","size"] )?)?;
   if size==0.{return Ok(false);}
   let market=h.indexed(&store_field(h.as_ref(),"marketsById")?,&instrument)?;
   if !h.truthy(&market){log(h.as_ref(),||session_template!(h;h.string("Market not found for instrument "),instrument),LogSeverity::Warning)?;return Ok(false);}
   let side=if size>0.{"long"}else{"short"};let contracts=size.abs();
   let entry=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&position,&["ep","entry_price","ep1"] )?,"0"))?;
   let upnl=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&position,&["up","unrealized_pnl"] )?,"0"))?;
   let rpnl=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&position,&["rp","realized_pnl"] )?,"0"))?;
   let parsed=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&position,&["l","leverage"] )?,"0"))?;let mut leverage=h.number(parsed);
   if parsed==0.||!parsed.is_finite(){
    let map=store_field(h.as_ref(),"fetchedLeverage")?;let symbol=h.get(&market,"symbol")?;let fetched=h.indexed(&map,&symbol)?;
    leverage=if h.truthy(&fetched)&&!h.truthy(&h.get(&fetched,"isFetching")?){h.get(&fetched,if side=="long"{"buyLeverage"}else{"sellLeverage"})?}else{field(h.as_ref(),"defaultLeverage")?};
   }
   let liquidation=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&position,&["el","est_liquidation_price","liquidation_price"] )?,"0"))?;
   let notional=h.parse_float(fallback(h.as_ref(),or(h.as_ref(),&position,&["n","notional"] )?,"0"))?.abs();
   let push=h.get(&positions,"push")?;
   let mapped=h.object(vec![("id",h.position_id(h.get(&market,"symbol")?)?),("symbol",h.get(&market,"symbol")?),("side",h.string(side)),("contracts",h.number(contracts)),("entryPrice",h.number(entry)),("notional",h.number(notional)),("leverage",leverage),("unrealizedPnl",h.number(upnl)),("realizedPnl",h.number(rpnl)),("liquidationPrice",h.number(liquidation)),("marginType",h.string("isolated")),("positionMode",mode.clone()),("lastUpdate",h.number(h.virtual_now()?)),("info",position)]);
   h.call(push,positions.clone(),vec![mapped])?;Ok(false)
  })?;
  let next=h.clone();
  let callback=h.function(Rc::new(move |position|next.get(&position,"symbol")));
  let real=invoke(h.as_ref(),positions.clone(),"map",vec![callback])?;
  // SameValueZero membership is bound separately from property-key indexing.
  let real_symbols=h.symbol_set(real)?;
  let fake=own_call(h.as_ref(),"getFakePositions",vec![])?;let filter=h.get(&fake,"filter")?;
  let next=h.clone();let callback=h.function(Rc::new(move |position|Ok(next.boolean(!next.has_symbol(&real_symbols,next.get(&position,"symbol")?)?))));
  let fake=h.call(filter,fake,vec![callback])?;
  let all=h.array(vec![]);h.append_iterable(&all,positions.clone())?;h.append_iterable(&all,fake)?;
  log(h.as_ref(),||session_template!(h;h.string("Fetched "),h.get(&positions,"length")?,h.string(" GRVT positions ("),h.get(&all,"length")?,h.string(" including fake)")),LogSeverity::Debug)?;Ok(all)
 }.await;
 match result{Ok(value)=>Ok(value),Err(error)=>catch_log(h.as_ref(),error,"Error fetching positions: ",LogSeverity::Error)}
}))
}
/// ts: GrvtExchange.fetchOpenOrders.
pub fn fetch_open_orders<H: AccountHost>(host: Rc<H>, symbol: H::Value) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let result:JsResult<H::Value>=async{
  if !h.truthy(&field(h.as_ref(),"grvtClient")?){return Err(h.error(h.string("GRVT SDK client not initialized"))?);}
  if !h.truthy(&field(h.as_ref(),"grvtTradingAccountId")?){log(h.as_ref(),||Ok(h.string("Skipping orders fetch - no trading account ID provided")),LogSeverity::Debug)?;return Ok(h.array(vec![]));}
  let instrument=if h.truthy(&symbol){own_call(h.as_ref(),"symbolToInstrumentId",vec![symbol])?}else{h.undefined()};
  let tdg=h.get(&field(h.as_ref(),"grvtClient")?,"tdgClient")?;let method=h.get(&tdg,"openOrders")?;
  let request=h.object(vec![("sub_account_id",field(h.as_ref(),"grvtTradingAccountId")?)]);if h.truthy(&instrument){h.define(&request,"instrument_id",instrument)?;}
  let endpoint=invoke(h.as_ref(),field(h.as_ref(),"grvtClient")?,"authenticatedEndpoint",vec![])?;let endpoint=h.await_value(endpoint).await?;
  let response=h.await_value(h.call(method,tdg,vec![request,endpoint])?).await?;let data=or(h.as_ref(),&response,&["r","result"])?;
  log(h.as_ref(),||session_template!(h;h.string("GRVT open-orders response: "),fallback(h.as_ref(),optional(h.as_ref(),data.clone(),"length")?,"0"),h.string(" rows")),LogSeverity::Debug)?;
  log(h.as_ref(),||session_template!(h;h.string("fetchOpenOrders: response keys="),h.keys_join(response)?,h.string(", ordersData="),h.string(if h.truthy(&data){"array"}else{"undefined"})),LogSeverity::Info)?;
  if !h.truthy(&data){log(h.as_ref(),||Ok(h.string("fetchOpenOrders: no orders data in response")),LogSeverity::Warning)?;return Ok(h.array(vec![]));}
  if h.equal(&h.get(&data,"length")?,&h.number(0.)){log(h.as_ref(),||Ok(h.string("fetchOpenOrders: API returned empty orders array")),LogSeverity::Info)?;}
  let map=h.get(&data,"map")?;let next=h.clone();let callback=h.function(Rc::new(move |order|own_call(next.as_ref(),"mapOrder",vec![order])));let orders=h.call(map,data,vec![callback])?;
  log(h.as_ref(),||session_template!(h;h.string("Fetched "),h.get(&orders,"length")?,h.string(" open orders")),LogSeverity::Info)?;Ok(orders)
 }.await;
 match result{Ok(value)=>Ok(value),Err(error)=>catch_log(h.as_ref(),error,"Error fetching open orders: ",LogSeverity::Error)}
}))
}
/// ts: GrvtExchange#mapOrder.
pub fn map_order<H: AccountHost>(h: &H, order: H::Value) -> JsResult<H::Value> {
    let full = optional(h, h.get(&order, "legs")?, "0")?;
    let leg = if h.truthy(&full) {
        full
    } else {
        let lite = optional(h, h.get(&order, "l")?, "0")?;
        if h.truthy(&lite) {
            lite
        } else {
            h.object(vec![])
        }
    };
    let state = or(h, &order, &["state", "s1"])?;
    let state = if h.truthy(&state) {
        state
    } else {
        h.object(vec![])
    };
    let metadata = or(h, &order, &["metadata", "m"])?;
    let metadata = if h.truthy(&metadata) {
        metadata
    } else {
        h.object(vec![])
    };
    let client = or(h, &metadata, &["client_order_id", "co"])?;
    let raw = or(h, &order, &["order_id", "oi"])?;
    let id = if h.truthy(&client) {
        h.js_string(client.clone())?
    } else {
        raw.clone()
    };
    if h.truthy(&client) {
        let parent = h.exchange();
        let record = h.get(&parent, "recordNonce")?;
        let client = h.js_string(client)?;
        let time = own_call(h, "getOrderCreationTimeMs", vec![metadata])?;
        h.call(record, parent, vec![client, raw, time])?;
    }
    let instrument = or(h, &leg, &["instrument", "i"])?;
    if !h.truthy(&instrument) {
        log(
            h,
            || Ok(h.string("mapOrder: no instrument found in order")),
            LogSeverity::Warning,
        )?;
    }
    let market = if h.truthy(&instrument) {
        let first = h.indexed(&store_field(h, "marketsById")?, &instrument)?;
        if h.truthy(&first) {
            first
        } else {
            let table = store_field(h, "marketsById")?;
            let upper = invoke(h, instrument.clone(), "toUpperCase", vec![])?;
            let second = h.indexed(&table, &upper)?;
            if h.truthy(&second) {
                second
            } else {
                let markets = store_field(h, "markets")?;
                let find = h.get(&markets, "find")?;
                let needle = instrument.clone();
                let host = h.binding();
                let predicate = h.function(Rc::new(move |market| {
                    let left = invoke(
                        host.as_ref(),
                        host.get(&market, "id")?,
                        "toUpperCase",
                        vec![],
                    )?;
                    let right = invoke(host.as_ref(), needle.clone(), "toUpperCase", vec![])?;
                    Ok(host.boolean(host.equal(&left, &right)))
                }));
                h.call(find, markets, vec![predicate])?
            }
        }
    } else {
        h.undefined()
    };
    let symbol = if h.truthy(&market) {
        h.get(&market, "symbol")?
    } else {
        let upper = if h.nullish(&instrument) {
            h.undefined()
        } else {
            invoke(h, instrument, "toUpperCase", vec![])?
        };
        fallback(h, upper, "UNKNOWN")
    };
    let book = optional(h, h.get(&state, "book_size")?, "0")?;
    let book = if h.truthy(&book) {
        book
    } else {
        optional(h, h.get(&state, "bs")?, "0")?
    };
    let book = h.parse_float(fallback(h, book, "0"))?;
    let traded = optional(h, h.get(&state, "traded_size")?, "0")?;
    let traded = if h.truthy(&traded) {
        traded
    } else {
        optional(h, h.get(&state, "ts")?, "0")?
    };
    let traded = h.parse_float(fallback(h, traded, "0"))?;
    let market_value = h.get(&order, "is_market")?;
    let is_market = if !h.is_undefined(&market_value) {
        h.get(&order, "is_market")?
    } else {
        h.get(&order, "im")?
    };
    let reduce = h.get(&order, "reduce_only")?;
    let reduce = if !h.is_undefined(&reduce) {
        h.get(&order, "reduce_only")?
    } else {
        h.get(&order, "ro")?
    };
    let trigger = h.trigger_metadata(order.clone())?;
    let kind = if h.equal(
        &optional(h, trigger.clone(), "type")?,
        &h.string("STOP_LOSS"),
    ) {
        "stop_market"
    } else if h.equal(
        &optional(h, trigger.clone(), "type")?,
        &h.string("TAKE_PROFIT"),
    ) {
        "take_profit_market"
    } else if h.truthy(&is_market) {
        "market"
    } else {
        "limit"
    };
    let buying = h.get(&leg, "is_buying_asset")?;
    let buying = if !h.is_undefined(&buying) {
        h.get(&leg, "is_buying_asset")?
    } else {
        h.get(&leg, "ib")?
    };
    let limit = h.parse_float(fallback(h, or(h, &leg, &["limit_price", "lp"])?, "0"))?;
    let price = null_default(h, optional(h, trigger.clone(), "triggerPrice")?, || {
        Ok(h.number(limit))
    })?;
    let amount = h.parse_float(fallback(h, or(h, &leg, &["size", "s"])?, "0"))?;
    let status = or(h, &state, &["status", "s"])?;
    let update = or(h, &state, &["update_time", "ut"])?;
    let status = own_call(h, "mapOrderStatus", vec![status])?;
    let result = h.object(vec![
        ("id", id),
        ("symbol", symbol),
        ("type", h.string(kind)),
        (
            "side",
            h.string(if h.truthy(&buying) { "buy" } else { "sell" }),
        ),
        ("price", price),
        ("amount", h.number(amount)),
        ("filled", h.number(traded)),
        ("remaining", h.number(book)),
        ("status", status),
        (
            "reduceOnly",
            if h.truthy(&reduce) {
                reduce
            } else {
                h.boolean(false)
            },
        ),
        ("timeInForce", h.time_in_force(order.clone())?),
    ]);
    let close = optional(h, trigger.clone(), "close")?;
    if !h.is_undefined(&close) {
        h.define(&result, "close", h.get(&trigger, "close")?)?;
    }
    if kind == "stop_market" && h.truthy(&optional(h, trigger.clone(), "triggerType")?) {
        h.define(&result, "slTriggerType", h.get(&trigger, "triggerType")?)?;
    }
    if kind == "take_profit_market" && h.truthy(&optional(h, trigger.clone(), "triggerType")?) {
        h.define(&result, "tpTriggerType", h.get(&trigger, "triggerType")?)?;
    }
    let last = if h.truthy(&update) {
        h.parse_int(update)? / 1000000.
    } else {
        h.now()?
    };
    h.define(&result, "lastUpdate", h.number(last))?;
    h.define(&result, "info", order)?;
    Ok(result)
}
