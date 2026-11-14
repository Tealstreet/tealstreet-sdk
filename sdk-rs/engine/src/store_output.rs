//! Final public Store settings. Zero is a value; an unknown rail omits the row.
use crate::store::MarketSetting;

pub(crate) fn rails(setting: &MarketSetting) -> Option<[f64; 2]> {
    let scalar = setting.leverage.is_finite().then_some(setting.leverage);
    let side = |value: Option<f64>| value.filter(|v| v.is_finite()).or(scalar);
    Some([side(setting.buy_leverage)?, side(setting.sell_leverage)?])
}
