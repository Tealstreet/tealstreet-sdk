//! Bind source-generated COIN-M constants to the shared native JavaScript graph.
use super::types::*;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_typed};

fn table<T: serde::Serialize>(rows: &[(&str, T)]) -> Result<Js, Thrown> {
    let value = Js::object(vec![]);
    for (key, item) in rows {
        value.define(key, from_typed(item)?)?;
    }
    Ok(value)
}

pub(crate) fn install(globals: &Js) -> Result<(), Thrown> {
    globals.define(
        "OrderRejectReason",
        Js::object(vec![(
            "PostOnly",
            from_typed(&safe_cex_types::OrderRejectReason::PostOnly)?,
        )]),
    )?;
    globals.define("RECV_WINDOW", Js::Number(RECV_WINDOW))?;
    globals.define("BASE_URL", table(BASE_URL)?)?;
    globals.define("BASE_WS_URL", table(BASE_WS_URL)?)?;
    globals.define("ENDPOINTS", table(ENDPOINTS)?)?;
    globals.define("ORDER_TYPE", table(ORDER_TYPE)?)?;
    globals.define("ORDER_SIDE", table(ORDER_SIDE)?)?;
    globals.define("POSITION_SIDE", table(POSITION_SIDE)?)?;
    globals.define("TIME_IN_FORCE", table(TIME_IN_FORCE)?)?;
    globals.define("ORDER_TRIGGER_TYPE", table(ORDER_TRIGGER_TYPE)?)?;
    for (name, values) in [
        ("PUBLIC_ENDPOINTS", PUBLIC_ENDPOINTS),
        ("GRAFANA_ENDPOINTS_EVERY", GRAFANA_ENDPOINTS_EVERY),
        ("GRAFANA_ENDPOINTS_PERIODIC", GRAFANA_ENDPOINTS_PERIODIC),
    ] {
        globals.define(
            name,
            Js::array(values.iter().map(|value| Js::str(value)).collect()),
        )?;
    }
    Ok(())
}
