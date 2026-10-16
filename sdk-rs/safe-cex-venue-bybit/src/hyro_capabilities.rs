//! HyroTrader metadata and execution capability method bodies.
use crate::hyro_private_session::{SessionFuture, SessionHost};

macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}

pub fn fail<H: SessionHost>(
    h: &H,
    owner: H::Value,
    message: H::Value,
) -> Result<H::Value, H::Error> {
    call!(h, owner, "emitAndLogError", vec![message.clone()]);
    Err(h.error(message)?)
}

pub fn fetch_and_cache_api_key_info<H: SessionHost>(
    h: &H,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = Ok(h.undefined());
    Box::pin(async move { result })
}

pub fn fetch_api_key_info<H: SessionHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = (|| {
        Ok(call!(
            h,
            owner,
            "fail",
            vec![h.literal("HyroTrader does not provide API-key metadata")]
        ))
    })();
    Box::pin(async move { h.wait(result?).await })
}

pub fn get_account<H: SessionHost>(
    h: &H,
    owner: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = (|| {
        Ok(call!(
            h,
            owner,
            "fail",
            vec![h.literal("HyroTrader does not provide account identity metadata")]
        ))
    })();
    Box::pin(async move { h.wait(result?).await })
}

pub fn fetch_fees<H: SessionHost>(
    h: &H,
    _category: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = Ok(h.array(vec![]));
    Box::pin(async move { result })
}

pub fn fetch_position_mode<H: SessionHost>(
    h: &H,
    _symbol: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = Ok(h.literal("oneway"));
    Box::pin(async move { result })
}

pub fn change_position_mode<H: SessionHost>(
    h: &H,
    owner: H::Value,
    hedged: H::Value,
    _symbol: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = (|| {
        if h.truthy(&hedged) {
            call!(
                h,
                owner,
                "fail",
                vec![h.literal("HyroTrader supports one-way position mode only")]
            );
        }
        Ok(h.undefined())
    })();
    Box::pin(async move { result })
}

pub fn change_margin_type<H: SessionHost>(
    h: &H,
    owner: H::Value,
    isolated: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let result = (|| {
        if h.truthy(&isolated) {
            call!(
                h,
                owner,
                "fail",
                vec![h.literal("HyroTrader supports cross margin only")]
            );
        }
        Ok(h.undefined())
    })();
    Box::pin(async move { result })
}

pub fn assert_private_ready<H: SessionHost>(h: &H, owner: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&h.get(&h.get(&owner, "privateWebsocket")?, "started")?)
        && !h.truthy(&h.get(&h.get(&owner, "privateWebsocket")?, "ready")?)
    {
        call!(
            h,
            owner,
            "fail",
            vec![h.literal(
                "HyroTrader account is reconnecting; wait for private state reconciliation before placing orders"
            )]
        );
    }
    Ok(h.undefined())
}

pub fn validate_order_capabilities<H: SessionHost>(
    h: &H,
    owner: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    call!(h, owner.clone(), "assertPrivateReady", vec![]);
    let markets = h.get(&h.get(&owner, "store")?, "marketsBySymbol")?;
    let symbol = h.get(&opts, "symbol")?;
    let market = h.intrinsic("computed property", vec![markets, symbol])?;
    if !h.equal(&h.optional(&market, "quote")?, &h.literal("USDT")) {
        call!(
            h,
            owner.clone(),
            "fail",
            vec![h.literal("HyroTrader supports USDT perpetual instruments only")]
        );
    }
    if !h.equal(&h.get(&opts, "maxSlippagePct")?, &h.undefined()) {
        call!(
            h,
            owner.clone(),
            "fail",
            vec![h.literal(
                "HyroTrader does not support client-defined slippage limits; protection uses the simulator-configured gate"
            )]
        );
    }
    if h.truthy(&h.intrinsic("property in", vec![h.literal("bybitBbo"), opts.clone()])?)
        || h.truthy(&h.intrinsic(
            "property in",
            vec![h.literal("bybitSlippageToleranceTicks"), opts],
        )?)
    {
        call!(
            h,
            owner,
            "fail",
            vec![h.literal("HyroTrader does not support Bybit BBO or TickSize slippage controls")]
        );
    }
    Ok(h.undefined())
}
