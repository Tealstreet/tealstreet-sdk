//! History receiver adapters retain native values until the public model projection.
use crate::binding::{
    rt::{Rt, venue},
    value::{Js, Thrown, arg},
};
use safe_cex_types::ExchangeName;
use std::rc::Rc;

pub(crate) fn query_windows(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let optional = |v: Js| {
        if v.is_undefined() {
            Ok(None)
        } else {
            v.to_number().map(Some)
        }
    };
    let windows =
        venue(rt)?.get_execution_query_windows(optional(arg(&args, 0))?, optional(arg(&args, 1))?);
    Ok(Js::array(
        windows
            .into_iter()
            .map(|(start, end)| {
                Js::object(vec![
                    ("startTime", start.map_or(Js::Undefined, Js::Number)),
                    ("endTime", end.map_or(Js::Undefined, Js::Number)),
                ])
            })
            .collect(),
    ))
}

pub(crate) fn refuse_option_symbol(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    crate::ledger_history::refuse_option_symbol(rt, &arg(&args, 0), &arg(&args, 1))
}

pub(crate) fn fetch_closed_pnl(rt: &Rt, opts: Js) -> Result<Js, Thrown> {
    Ok(match venue(rt)?.base.name {
        ExchangeName::Bybitspot => Js::array(vec![]),
        ExchangeName::Bybitinverse => crate::ledger_history::fetch_inverse_closed_pnl(
            Rc::new(rt.clone()),
            rt.this.clone(),
            opts,
        ),
        _ => crate::ledger_history::fetch_closed_pnl(Rc::new(rt.clone()), rt.this.clone(), opts),
    })
}
