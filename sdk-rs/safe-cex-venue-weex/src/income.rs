//! Income record normalization only; paging is owned by the shared exchange crate.
use crate::utils::{
    WeexUtilityError, WeexUtilityInput, normalize_timestamp_to_ms, to_integer_string, to_number,
};
use safe_cex_num::{js_string, js_truthy};
use safe_cex_types::IncomeRecord;

pub fn normalize_income(
    raw: &WeexUtilityInput,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<IncomeRecord, WeexUtilityError> {
    let value = raw.as_value();
    if value.is_null() {
        return Err(WeexUtilityError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'symbol')".to_owned(),
        });
    }
    let truthy_string = |keys: &[&str], fallback: &str| -> Result<String, WeexUtilityError> {
        match keys
            .iter()
            .find_map(|key| value.get(*key).filter(|v| js_truthy(Some(v))))
        {
            Some(v) => Ok(js_string(Some(v))?),
            None => Ok(fallback.to_owned()),
        }
    };
    Ok(IncomeRecord {
        symbol: truthy_string(&["symbol"], "")?,
        income_type: truthy_string(&["incomeType"], "")?,
        income: to_number(raw.pick_retained(&["income"]).as_ref(), None)?,
        asset: truthy_string(&["asset", "coin"], "USDT")?,
        timestamp: normalize_timestamp_to_ms(
            raw.pick_retained(&["time", "createdTime"]).as_ref(),
            local_to_utc,
        )?,
        tran_id: to_integer_string(raw.pick_retained(&["billId", "tranId", "id"]).as_ref())?,
        trade_id: if js_truthy(value.get("tradeId")) {
            Some(to_integer_string(raw.pick_retained(&["tradeId"]).as_ref())?)
        } else {
            None
        },
        category: Some(safe_cex_types::model_utils::income_category_from_type(
            value.get("incomeType"),
        )?),
    })
}
