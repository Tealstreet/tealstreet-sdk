//! TS prototype getters of the Bitget classes, as accessors on the receiver.
use super::{
    rt::{Rt, Variant},
    value::{Getter, Js},
};
use std::rc::Rc;

fn testnet(this: &Js) -> Result<bool, super::value::Thrown> {
    Ok(this.get("options")?.get("testnet")?.truthy())
}

// ts: BitgetExchange.apiProductType / apiMarginCoin / apiUtaCategory / usesUtaRouting
// ts: BitgetUSDCExchange / BitgetInverseExchange override apiProductType and apiMarginCoin
pub(crate) fn install(rt: &Rt) -> Result<(), super::value::Thrown> {
    let this = &rt.this;
    let variant = rt.shared.extra.variant;
    let product: Getter = Rc::new(move |this| {
        let t = testnet(this)?;
        Ok(Js::str(match variant {
            Variant::Classic => crate::profile::api_product_type(t),
            Variant::Usdc => crate::profile::usdc_api_product_type(t),
            Variant::Inverse => crate::profile::inverse_api_product_type(t),
        }))
    });
    let margin: Getter = Rc::new(move |this| {
        let t = testnet(this)?;
        Ok(Js::str(match variant {
            Variant::Classic => crate::profile::api_margin_coin(t),
            Variant::Usdc => crate::profile::usdc_api_margin_coin(t),
            Variant::Inverse => crate::profile::inverse_api_margin_coin(),
        }))
    });
    let category: Getter = Rc::new(|this| {
        this.get("apiProductType")?
            .call_method("toUpperCase", vec![])
    });
    let uta: Getter = Rc::new(|this| {
        Ok(Js::Bool(crate::profile::uses_uta_routing(
            this.get("isUta")?.truthy(),
            this.get("supportsUtaRouting")?.truthy(),
            testnet(this)?,
        )))
    });
    let suppress: Getter = Rc::new(|this| this.get("_suppressBitgetValidationErrorEmission"));
    this.define_accessor("apiProductType", Some(product), None)?;
    this.define_accessor("apiMarginCoin", Some(margin), None)?;
    this.define_accessor("apiUtaCategory", Some(category), None)?;
    this.define_accessor("usesUtaRouting", Some(uta), None)?;
    this.define_accessor(
        "suppressBitgetValidationErrorEmission",
        Some(suppress),
        None,
    )?;
    Ok(())
}
