//! GRVT order placement and bounded fill reconciliation over bound runtime operations.
use crate::{
    account_runtime::AccountHost,
    builder_workflow::{JsPromise, JsResult},
    session_host::{optional, session_template},
};
use safe_cex_exchange::runtime::LogSeverity;
use std::rc::Rc;
pub trait TradingHost: AccountHost {
    fn parse_int_decimal(&self, value: Self::Value) -> JsResult<Self::Value, f64>;
    fn random(&self) -> JsResult<Self::Value, f64>;
    fn adjust(&self, value: Self::Value, step: Self::Value) -> JsResult<Self::Value, f64>;
    fn pow(&self, base: f64, exponent: Self::Value) -> JsResult<Self::Value, f64>;
    fn sdk_domain(&self) -> Self::Value;
    fn sdk_order_types(&self) -> Self::Value;
    fn sdk_sign(&self, key: Self::Value, typed: Self::Value) -> JsResult<Self::Value>;
    fn sdk_decode(&self, signature: Self::Value) -> JsResult<Self::Value>;
    fn is_builder_error(&self, error: &Self::Value) -> bool;
    fn new_map(&self) -> JsResult<Self::Value>;
    fn new_set(&self, values: Self::Value) -> JsResult<Self::Value>;
    fn has_own(&self, value: &Self::Value, key: &str) -> JsResult<Self::Value, bool>;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn is_number(&self, value: &Self::Value) -> bool;
    fn decimal_pattern(&self, value: &Self::Value) -> JsResult<Self::Value, bool>;
    fn first_set_value(&self, values: Self::Value) -> JsResult<Self::Value>;
    fn minimum(&self, values: Self::Value) -> JsResult<Self::Value, f64>;
}
fn field<H: AccountHost>(h: &H, key: &str) -> JsResult<H::Value> {
    h.get(&h.exchange(), key)
}
fn method<H: AccountHost>(
    h: &H,
    receiver: H::Value,
    key: &str,
    args: Vec<H::Value>,
) -> JsResult<H::Value> {
    let f = h.get(&receiver, key)?;
    h.call(f, receiver, args)
}
fn own<H: AccountHost>(h: &H, key: &str, args: Vec<H::Value>) -> JsResult<H::Value> {
    method(h, h.exchange(), key, args)
}
fn or<H: AccountHost>(h: &H, value: &H::Value, keys: &[&str]) -> JsResult<H::Value> {
    for (i, key) in keys.iter().enumerate() {
        let v = h.get(value, key)?;
        if h.truthy(&v) || i == keys.len() - 1 {
            return Ok(v);
        }
    }
    Ok(h.undefined())
}
fn optional_or<H: AccountHost>(
    h: &H,
    value: H::Value,
    key: &str,
    next: impl FnOnce() -> JsResult<H::Value>,
) -> JsResult<H::Value> {
    let v = optional(h, value, key)?;
    if h.truthy(&v) { Ok(v) } else { next() }
}
fn null_default<H: AccountHost>(
    h: &H,
    value: H::Value,
    next: impl FnOnce() -> JsResult<H::Value>,
) -> JsResult<H::Value> {
    if h.nullish(&value) { next() } else { Ok(value) }
}
fn log<H: AccountHost>(
    h: &H,
    make: impl FnOnce() -> JsResult<H::Value>,
    severity: LogSeverity,
) -> JsResult<H::Value, ()> {
    let parent = h.exchange();
    let f = h.get(&parent, "log")?;
    h.call(f, parent, vec![make()?, h.severity(severity)])?;
    Ok(())
}
fn trigger_by<H: AccountHost>(h: &H, value: H::Value) -> H::Value {
    h.string(if h.equal(&value, &h.string("mark_price")) {
        "MARK"
    } else if h.equal(&value, &h.string("index_price")) {
        "INDEX"
    } else {
        "LAST"
    })
}
/// ts: GrvtExchange.placeOrder.
pub fn place_order<H: TradingHost>(host: Rc<H>, opts: H::Value) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let caller_id=own(h.as_ref(),"resolveCallerClientOrderId",vec![opts.clone()])?;
 let result:JsResult<H::Value>=async{
  if !h.truthy(&field(h.as_ref(),"wallet")?){return Err(h.error(h.string("Private key required for order placement - wallet not initialized"))?);}
  if !h.truthy(&field(h.as_ref(),"grvtTradingAccountId")?){log(h.as_ref(),||Ok(h.string("No trading account ID configured, using 0 (default sub-account)")),LogSeverity::Warning)?;h.set(&h.exchange(),"grvtTradingAccountId",h.string("0"))?;}
  let store=field(h.as_ref(),"store")?;let markets=h.get(&store,"marketsBySymbol")?;let market=h.indexed(&markets,&h.get(&opts,"symbol")?)?;
  if !h.truthy(&market){return Err(h.error(session_template!(h;h.string("Market "),h.get(&opts,"symbol")?,h.string(" not found"))?)?);}
  let metadata=own(h.as_ref(),"getInstrumentSigningMetadata",vec![market.clone()])?;
  let hash=h.get(&metadata,"ih")?;let base=h.get(&metadata,"bd")?;let quote=h.get(&metadata,"qd")?;
  log(h.as_ref(),||session_template!(h;h.string("Instrument details: hash="),hash.clone(),h.string(", baseDecimals="),base.clone(),h.string(", quoteDecimals="),quote),LogSeverity::Debug)?;
  let side=if h.equal(&h.get(&opts,"side")?,&h.string("buy")){"BUY"}else{"SELL"};
  let kind=if h.equal(&h.get(&opts,"type")?,&h.string("market")){"MARKET"}else{"LIMIT"};
  let tif=if h.equal(&h.get(&opts,"timeInForce")?,&h.string("ImmediateOrCancel")){"IMMEDIATE_OR_CANCEL"}else if h.equal(&h.get(&opts,"timeInForce")?,&h.string("FillOrKill")){"FILL_OR_KILL"}else{"GOOD_TILL_TIME"};
  let price=if h.truthy(&h.get(&opts,"price")?){h.adjust(h.get(&opts,"price")?,h.get(&h.get(&market,"precision")?,"price")?)?}else{0.};
  let size=h.adjust(h.get(&opts,"amount")?,h.get(&h.get(&market,"precision")?,"amount")?)?;
  log(h.as_ref(),||session_template!(h;h.string("Precision - price: "),h.get(&h.get(&market,"precision")?,"price")?,h.string(", amount: "),h.get(&h.get(&market,"precision")?,"amount")?),LogSeverity::Debug)?;
  log(h.as_ref(),||session_template!(h;h.string("Adjusted - price: "),h.number(price),h.string(", size: "),h.number(size),h.string(" (from "),h.get(&opts,"amount")?,h.string(")")),LogSeverity::Debug)?;
  if size<=0.{return Err(h.error(session_template!(h;h.string("Invalid order size: "),h.number(size),h.string(" (original: "),h.get(&opts,"amount")?,h.string(", precision: "),h.get(&h.get(&market,"precision")?,"amount")?,h.string(")"))?)?);}
  let nonce=(h.random()?*1e9).floor();let expiration=method(h.as_ref(),h.number((h.now()?+3600.*1000.)*1e6),"toString",vec![])?;
  let options=field(h.as_ref(),"options")?;let secret=h.get(&options,"secret")?;
  let starts=if h.nullish(&secret){h.undefined()}else{method(h.as_ref(),secret,"startsWith",vec![h.string("0x")])?};
  let key=if h.truthy(&starts){h.get(&field(h.as_ref(),"options")?,"secret")?}else{session_template!(h;h.string("0x"),h.get(&field(h.as_ref(),"options")?,"secret")?)?};
  let chain=if h.truthy(&h.get(&field(h.as_ref(),"options")?,"testnet")?){326.}else{325.};
  let domain=h.object(vec![]);h.spread(&domain,h.sdk_domain())?;h.define(&domain,"chainId",h.number(chain))?;
  let builder=h.await_value(own(h.as_ref(),"getBuilderInfo",vec![])?).await?;
  log(h.as_ref(),||session_template!(h;h.string("Including builder info: "),h.get(&builder,"builder")?,h.string(" (fee: "),h.get(&builder,"builderFee")?,h.string(")")),LogSeverity::Debug)?;
  let account=if h.truthy(&field(h.as_ref(),"grvtTradingAccountId")?){h.parse_int_decimal(field(h.as_ref(),"grvtTradingAccountId")?)?}else{0.};
  let message=h.object(vec![
   ("subAccountID",h.number(account)),("isMarket",h.boolean(kind=="MARKET")),("timeInForce",own(h.as_ref(),"mapTimeInForceToEnum",vec![h.string(tif)])?),
   ("postOnly",h.boolean(h.equal(&h.get(&opts,"timeInForce")?,&h.string("PostOnly")))),("reduceOnly",{let v=h.get(&opts,"reduceOnly")?;if h.truthy(&v){v}else{h.boolean(false)}}),
   ("legs",h.array(vec![h.object(vec![("assetID",hash),("contractSize",h.number(safe_cex_num::math_round(size*h.pow(10.,base)?))),("limitPrice",h.number(if kind=="LIMIT"&&price>0.{safe_cex_num::math_round(price*1e9)}else{0.})),("isBuyingContract",h.boolean(side=="BUY"))])])),
   ("builder",h.get(&builder,"builder")?),("builderFee",h.get(&builder,"builderFeeInt")?),("nonce",h.number(nonce)),("expiration",expiration.clone())]);
  let template=h.sdk_order_types();let typed=h.object(vec![]);h.spread(&typed,template)?;h.define(&typed,"domain",domain)?;h.define(&typed,"message",message)?;
  let signed=h.sdk_sign(key,typed)?;let decoded=h.sdk_decode(signed)?;let r=h.get(&decoded,"r")?;let s=h.get(&decoded,"s")?;let v=h.get(&decoded,"v")?;
  log(h.as_ref(),||session_template!(h;h.string("Placing order: "),h.get(&opts,"symbol")?,h.string(" "),h.string(side),h.string(" "),h.number(size),h.string(" @ "),if price!=0.&&!price.is_nan(){h.number(price)}else{h.string("market")}),LogSeverity::Info)?;
  log(h.as_ref(),||session_template!(h;h.string("Order opts: stopLoss="),h.get(&opts,"stopLoss")?,h.string(", takeProfit="),h.get(&opts,"takeProfit")?,h.string(", reduceOnly="),h.get(&opts,"reduceOnly")?),LogSeverity::Debug)?;
  if !h.truthy(&field(h.as_ref(),"grvtClient")?){return Err(h.error(h.string("GRVT SDK client not initialized"))?);}
  let id=null_default(h.as_ref(),caller_id,||method(h.as_ref(),h.number(nonce),"toString",vec![]))?;
  let create_time=method(h.as_ref(),h.number(h.now()?*1000000.),"toString",vec![])?;
  let mut trigger=h.undefined();let stop=h.equal(&h.get(&opts,"type")?,&h.string("stop_market"));let take=h.equal(&h.get(&opts,"type")?,&h.string("take_profit_market"));
  if stop||take{
   let trigger_price=or(h.as_ref(),&opts,&["triggerPrice","price"])?;
   if !h.truthy(&trigger_price){return Err(h.error(session_template!(h;h.string("Trigger price is required for "),h.get(&opts,"type")?,h.string(" orders"))?)?);}
   let trigger_type=if stop{"STOP_LOSS"}else{"TAKE_PROFIT"};let by=trigger_by(h.as_ref(),h.get(&opts,if stop{"slTriggerType"}else{"tpTriggerType"})?);
   log(h.as_ref(),||session_template!(h;h.string("Creating "),h.string(trigger_type),h.string(" order with trigger price "),trigger_price.clone(),h.string(", trigger by "),by.clone()),LogSeverity::Debug)?;
   trigger=h.object(vec![("trigger_type",h.string(trigger_type)),("tpsl",h.object(vec![("trigger_by",by),("trigger_price",method(h.as_ref(),trigger_price,"toString",vec![])?),("close_position",{let close=h.get(&opts,"close")?;if h.truthy(&close){close}else{h.boolean(false)}})]))]);
  }else if h.truthy(&h.get(&opts,"takeProfit")?)||h.truthy(&h.get(&opts,"stopLoss")?){
   let price=or(h.as_ref(),&opts,&["takeProfit","stopLoss"])?;let trigger_type=if h.truthy(&h.get(&opts,"takeProfit")?){"TAKE_PROFIT"}else{"STOP_LOSS"};
   let by=trigger_by(h.as_ref(),h.get(&opts,if h.truthy(&h.get(&opts,"takeProfit")?){"tpTriggerType"}else{"slTriggerType"})?);
   trigger=h.object(vec![("trigger_type",h.string(trigger_type)),("tpsl",h.object(vec![("trigger_by",by),("trigger_price",method(h.as_ref(),price,"toString",vec![])?),("close_position",{let close=h.get(&opts,"close")?;if h.truthy(&close){close}else{h.boolean(false)}})]))]);
  }
  let full=h.object(vec![
   ("sub_account_id",field(h.as_ref(),"grvtTradingAccountId")?),("is_market",h.boolean(kind=="MARKET")),("time_in_force",h.string(tif)),("post_only",h.boolean(h.equal(&h.get(&opts,"timeInForce")?,&h.string("PostOnly")))),
   ("reduce_only",{let v=h.get(&opts,"reduceOnly")?;if h.truthy(&v){v}else{h.boolean(false)}}),
   ("legs",h.array(vec![h.object(vec![("instrument",h.get(&market,"id")?),("size",method(h.as_ref(),h.number(size),"toString",vec![])?),("limit_price",if kind=="MARKET"||price==0.{h.undefined()}else{method(h.as_ref(),h.number(price),"toString",vec![])?}),("is_buying_asset",h.boolean(side=="BUY"))])])),
   ("signature",h.object(vec![("signer",h.get(&field(h.as_ref(),"wallet")?,"address")?),("r",r),("s",s),("v",v),("expiration",expiration),("nonce",h.number(nonce)),("chain_id",method(h.as_ref(),h.number(chain),"toString",vec![])?)])),
   ("metadata",{let metadata=h.object(vec![("client_order_id",id.clone()),("create_time",create_time)]);if h.truthy(&trigger){h.define(&metadata,"trigger",trigger)?;}metadata}),
   ("builder",h.get(&builder,"builder")?),("builder_fee",h.get(&builder,"builderFee")?)]);
  let tdg=h.get(&field(h.as_ref(),"grvtClient")?,"tdgClient")?;let create=h.get(&tdg,"createOrder")?;let request=h.object(vec![("order",full)]);
  let endpoint=method(h.as_ref(),field(h.as_ref(),"grvtClient")?,"authenticatedEndpoint",vec![])?;let endpoint=h.await_value(endpoint).await?;
  let response=h.await_value(h.call(create,tdg,vec![request,endpoint])?).await?;
  let message=h.get(&response,"m")?;let error_message=if h.truthy(&message){message}else{optional_or(h.as_ref(),h.get(&response,"error")?,"message",||optional_or(h.as_ref(),h.get(&response,"data")?,"m",||optional_or(h.as_ref(),h.get(&response,"data")?,"message",||{
   if h.number_value(h.get(&response,"status")?)?>=400.{session_template!(h;h.string("HTTP "),h.get(&response,"status")?)}else{Ok(h.null())}
  })))?};
  if h.truthy(&error_message){
   let code=h.get(&response,"c")?;let code=if h.truthy(&code){code}else{optional_or(h.as_ref(),h.get(&response,"error")?,"code",||optional_or(h.as_ref(),h.get(&response,"data")?,"c",||h.get(&response,"status")))?};
   let full=if h.truthy(&code){session_template!(h;error_message,h.string(" (code: "),code,h.string(")"))?}else{error_message};
   log(h.as_ref(),||session_template!(h;h.string("Order creation failed: "),full.clone()),LogSeverity::Error)?;return Err(h.error(full)?);
  }
  if h.truthy(&h.get(&response,"c")?)&&h.truthy(&h.get(&response,"m")?){
   let full=session_template!(h;h.get(&response,"m")?,h.string(" (code: "),h.get(&response,"c")?,h.string(")"))?;
   log(h.as_ref(),||session_template!(h;h.string("Order creation failed: "),full.clone()),LogSeverity::Error)?;return Err(h.error(full)?);
  }
  log(h.as_ref(),||session_template!(h;h.string("Order created successfully: client_order_id="),id.clone()),LogSeverity::Info)?;
  let real=optional_or(h.as_ref(),h.get(&response,"result")?,"order_id",||optional_or(h.as_ref(),h.get(&response,"r")?,"oi",||{
   let own=h.get(&response,"order_id")?;if h.truthy(&own){Ok(own)}else{optional(h.as_ref(),optional(h.as_ref(),h.get(&response,"data")?,"result")?,"order_id")}
  }))?;
  let parent=h.exchange();let record=h.get(&parent,"recordNonce")?;h.call(record,parent,vec![id.clone(),real,h.number(h.now()?)])?;
  let nested=optional(h.as_ref(),h.get(&response,"result")?,"order")?;let response_order=if h.truthy(&nested){nested}else{or(h.as_ref(),&response,&["result","r"])?};
  let fee=optional_or(h.as_ref(),response_order.clone(),"builder_fee",||optional_or(h.as_ref(),response_order,"bf",||h.get(&builder,"builderFee")))?;
  let emitter=field(h.as_ref(),"emitter")?;let emit=h.get(&emitter,"emit")?;h.call(emit,emitter,vec![h.string("builderFee"),h.object(vec![("orderId",id.clone()),("builderFee",fee),("builder",h.get(&builder,"builder")?)])])?;
  Ok(h.array(vec![id]))
 }.await;
 match result{Ok(value)=>Ok(value),Err(error)=>{
  if h.is_builder_error(&error){let parent=h.exchange();let emit=h.get(&parent,"emitAndLogError")?;h.call(emit,parent,vec![session_template!(h;h.string("Order failed: "),h.get(&error,"message")?)?])?;return Err(error);}
  let data=optional(h.as_ref(),h.get(&error,"response")?,"data")?;
  let message=optional_or(h.as_ref(),data.clone(),"message",||optional_or(h.as_ref(),data.clone(),"m",||h.get(&error,"message")))?;
  let code=optional_or(h.as_ref(),data.clone(),"code",||optional(h.as_ref(),data,"c"))?;
  let full=if h.truthy(&code){session_template!(h;message,h.string(" (code: "),code,h.string(")"))?}else{message};
  let parent=h.exchange();let emit=h.get(&parent,"emitAndLogError")?;h.call(emit,parent,vec![session_template!(h;h.string("Order failed: "),full.clone())?])?;
  Err(h.error(full)?)
 }}
}))
}
/// ts: GrvtExchange.fetchFillsForOrderIds.
pub fn fetch_fills_for_order_ids<H: TradingHost>(
    host: Rc<H>,
    ids: H::Value,
) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move{
 let totals=h.new_map()?;
 if h.equal(&h.get(&ids,"length")?,&h.number(0.)){return Ok(totals);}
 let result:JsResult<H::Value>=async{
  let account=field(h.as_ref(),"grvtTradingAccountId")?;let account=if h.truthy(&account){account}else{optional(h.as_ref(),field(h.as_ref(),"apiExtras")?,"subAccountId")?};
  if !h.truthy(&account){return Ok(totals.clone());}
  let map=h.get(&ids,"map")?;let next=h.clone();let normalize=h.function(Rc::new(move|id|next.js_string(id)));let requested=h.new_set(h.call(map,ids,vec![normalize])?)?;
  let keys=h.array(vec![]);h.append_iterable(&keys,requested.clone())?;
  let map=h.get(&keys,"map")?;let next=h.clone();let creation=h.function(Rc::new(move|id|method(next.as_ref(),field(next.as_ref(),"_nonceCreationTimes")?,"get",vec![id])));let times=h.call(map,keys,vec![creation])?;
  let every=h.get(&times,"every")?;let next=h.clone();let known=h.function(Rc::new(move|time|Ok(next.boolean(!next.is_undefined(&time)))));let all=h.call(every,times.clone(),vec![known])?;
  let oldest=if h.truthy(&all){Some(h.minimum(times)?)}else{None};
  let start=oldest.map(|oldest|safe_cex_num::math_max(0.,oldest-300000.)).unwrap_or(0.);
  let now=h.js_string(h.number(h.now()?*1000000.))?;let start=h.js_string(h.number(start*1000000.))?;
  let nonce_by_real=h.new_map()?;
  h.for_of(requested.clone(),&mut |id|{
   let real=method(h.as_ref(),field(h.as_ref(),"_nonceToOrderId")?,"get",vec![id.clone()])?;
   if h.truthy(&real){let set=h.get(&nonce_by_real,"set")?;let real=h.js_string(real)?;h.call(set,nonce_by_real.clone(),vec![real,id])?;}Ok(false)
  })?;
  let mut cursor=h.undefined();let seen=h.new_set(h.array(vec![]))?;
  for page in 0..20{
   let xhr=field(h.as_ref(),"tradingXhr")?;let post=h.get(&xhr,"post")?;
   let request=h.object(vec![("sa",account.clone()),("st",start.clone()),("et",now.clone()),("l",h.number(1000.))]);if h.truthy(&cursor){h.define(&request,"c",cursor.clone())?;}
   let response=h.await_value(h.call(post,xhr,vec![h.endpoint("FILL_HISTORY"),request])?).await?;let data=h.get(&response,"data")?;
   if !h.truthy(&data)||!h.is_object(&data){return Err(h.error(session_template!(h;h.string("GRVT fill history page "),h.number((page+1) as f64),h.string(" has an invalid response shape"))?)?);}
   let fills=null_default(h.as_ref(),h.get(&data,"r")?,||h.get(&data,"result"))?;
   if !h.is_array(&fills){return Err(h.error(session_template!(h;h.string("GRVT fill history page "),h.number((page+1) as f64),h.string(" has no fill array"))?)?);}
   h.for_of(fills,&mut |fill|{
    let metadata=null_default(h.as_ref(),h.get(&fill,"metadata")?,||h.get(&fill,"m"))?;
    let candidates=h.array(vec![h.get(&fill,"co")?,h.get(&fill,"client_order_id")?,optional(h.as_ref(),metadata.clone(),"co")?,optional(h.as_ref(),metadata,"client_order_id")?]);
    let filter=h.get(&candidates,"filter")?;let next=h.clone();let valid=h.function(Rc::new(move|id|Ok(next.boolean(!next.is_undefined(&id)&&!next.equal(&id,&next.null())&&!next.equal(&id,&next.string(""))))));
    let candidates=h.call(filter,candidates,vec![valid])?;let map=h.get(&candidates,"map")?;let next=h.clone();let normalize=h.function(Rc::new(move|id|next.js_string(id)));let clients=h.new_set(h.call(map,candidates,vec![normalize])?)?;
    let mut selected=h.undefined();
    if h.equal(&h.get(&clients,"size")?,&h.number(1.)){
     let first=h.first_set_value(clients)?;if h.truthy(&method(h.as_ref(),requested.clone(),"has",vec![first.clone()])?){selected=first;}
    }else if h.equal(&h.get(&clients,"size")?,&h.number(0.)){
     let matching=h.new_set(h.array(vec![]))?;
     for real in [h.get(&fill,"oi")?,h.get(&fill,"order_id")?]{
      if h.is_undefined(&real)||h.equal(&real,&h.null())||h.equal(&real,&h.string("")){continue;}
      let real=h.js_string(real)?;if h.truthy(&method(h.as_ref(),requested.clone(),"has",vec![real.clone()])?){method(h.as_ref(),matching.clone(),"add",vec![real.clone()])?;}
      let nonce=method(h.as_ref(),nonce_by_real.clone(),"get",vec![real])?;if h.truthy(&nonce){method(h.as_ref(),matching.clone(),"add",vec![nonce])?;}
     }
     if h.equal(&h.get(&matching,"size")?,&h.number(1.)){let values=h.array(vec![]);h.append_iterable(&values,matching)?;selected=h.get(&values,"0")?;}
    }
    if !h.truthy(&selected){return Ok(false);}
    let raw=null_default(h.as_ref(),h.get(&fill,"s")?,||h.get(&fill,"size"))?;
    // The TypeScript arms are NOT identical - one passes the value through and
    // the other wraps it in Number(). They coincide here only because
    // h.number_value subsumes both, so the branch is kept to preserve the
    // source's two separate decision points rather than because the source
    // duplicates a body.
    #[allow(clippy::if_same_then_else)]
    let amount=if h.is_number(&raw){h.number_value(raw)?}else if h.is_string(&raw)&&h.decimal_pattern(&raw)?{h.number_value(raw)?}else{f64::NAN};
    if !amount.is_finite()||amount<=0.{return Ok(false);}
    let set=h.get(&totals,"set")?;let previous=method(h.as_ref(),totals.clone(),"get",vec![selected.clone()])?;let previous=if h.truthy(&previous){previous}else{h.number(0.)};let sum=h.add(previous,h.number(amount))?;h.call(set,totals.clone(),vec![selected,sum])?;Ok(false)
   })?;
   let has_n=h.has_own(&data,"n")?;let has_next=h.has_own(&data,"next")?;
   if !has_n&&!has_next{return Err(h.error(session_template!(h;h.string("GRVT fill history page "),h.number((page+1) as f64),h.string(" is missing its cursor"))?)?);}
   let next=h.get(&data,if has_n{"n"}else{"next"})?;
   if !h.is_string(&next){return Err(h.error(session_template!(h;h.string("GRVT fill history page "),h.number((page+1) as f64),h.string(" has an invalid cursor"))?)?);}
   if !h.truthy(&next){return Ok(totals.clone());}
   if h.truthy(&method(h.as_ref(),seen.clone(),"has",vec![next.clone()])?){return Err(h.error(session_template!(h;h.string("GRVT fill history pagination repeated cursor on page "),h.number((page+1) as f64))?)?);}
   method(h.as_ref(),seen.clone(),"add",vec![next.clone()])?;cursor=next;
  }
  Err(h.error(h.string("GRVT fill history exceeded 20 pages"))?)
 }.await;
 match result{Ok(value)=>Ok(value),Err(error)=>{log(h.as_ref(),||session_template!(h;h.string("fetchFillsForOrderIds error: "),error.clone()),LogSeverity::Warning)?;Err(error)}}
}))
}
