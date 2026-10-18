//! Module-scope values the bodies import: BaseExchange, the four socket classes,
//! enums and JS builtins.
use super::{
    rt::{Rt, Shared},
    value::{Js, Thrown, arg},
};
use crate::classes::{self, Kind};
use safe_cex_exchange::js_runtime::{
    base_exchange::{self, BaseExchangeParts},
    globals::{install_builtins, install_enums},
    scope::scope_of,
};
use std::rc::{Rc, Weak};

fn scope(weak: &Weak<Shared>) -> Result<Rt, Thrown> {
    scope_of(weak)
}

/// `new BaseExchange(name, options, store)`; the Rust core is kept for `Exchange::base()`.
fn base(rt: &Rt, name: Js, options: Js, store: Js) -> Result<Js, Thrown> {
    let extra = &rt.shared.extra;
    let parts = BaseExchangeParts {
        name: extra.name.clone(),
        options: extra.options.clone(),
        store: rt.shared.store.clone(),
        settings: super::exchange::settings(),
        external_public_data: extra.external_public_data.clone(),
    };
    let spot = extra.name == safe_cex_types::ExchangeName::Krakenspot;
    let (this, core) =
        base_exchange::create(&rt.shared.sockets, parts, name, options, store, |this| {
            // krakenspot.exchange.ts: isSpot = true; Futures inherits false.
            if spot {
                this.define("isSpot", Js::Bool(true))?;
            }
            Ok(())
        })?;
    let _ = rt.shared.base.set(core);
    Ok(this)
}

pub(crate) fn build(weak: Weak<Shared>) -> Js {
    let g = Js::object(vec![]);
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    let w = weak.clone();
    def(
        "BaseExchange",
        Js::constructor("BaseExchange", None, move |_, a| {
            base(&scope(&w)?, arg(&a, 0), arg(&a, 1), arg(&a, 2))
        }),
    );
    for (class, kind) in [
        ("KrakenPublicWebsocket", Kind::FuturesPublic),
        ("KrakenPrivateWebsocket", Kind::FuturesPrivate),
        ("KrakenSpotPublicWebsocket", Kind::SpotPublic),
        ("KrakenSpotPrivateWebsocket", Kind::SpotPrivate),
    ] {
        let w = weak.clone();
        def(
            class,
            Js::constructor(class, None, move |_, a| {
                let rt = scope(&w)?;
                let parent = Rc::new(arg(&a, 0));
                let socket = match kind {
                    Kind::FuturesPublic => classes::futures_public(&rt, parent),
                    Kind::FuturesPrivate => classes::futures_private(&rt, parent),
                    Kind::SpotPublic => classes::spot_public(&rt, parent),
                    _ => classes::spot_private(&rt, parent),
                }?;
                Ok((*socket).clone())
            }),
        );
    }
    install_enums(&g);
    install_builtins(&g, weak);
    g
}
