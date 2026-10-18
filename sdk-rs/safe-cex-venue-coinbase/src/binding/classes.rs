use super::{
    error::{exchange_error, from_thrown},
    rt::Host,
    sockets::SocketHost,
    value::{self, to_js},
};
use crate::{
    classes::{
        self, ClassApiFactory, ClassIdentity, ClassMint, ClassRateLimit, CoinbaseConstructionHost,
    },
    portfolios_balances::PortfolioFailure,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{
    client_order_id::{ClientOrderIdRuleOverrides, pattern_client_order_id_rule},
    exchange::ExchangeSettings,
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        store,
        value::{Js, Thrown, arg, from_typed},
    },
};
use std::rc::Rc;

pub(super) fn settings(host: &Host) -> ExchangeSettings {
    let runtime = host.runtime();
    ExchangeSettings {
        tick_interval: Some(1000.0),
        positions_supported: Some(true),
        supports_public_only: host.shared.extra.name == safe_cex_types::ExchangeName::Coinbase,
        client_order_id_rule: Some(pattern_client_order_id_rule(
            "Coinbase".into(),
            "/^\\S+$/".into(),
            Rc::new(|id| {
                !id.is_empty()
                    && !id
                        .chars()
                        .any(|c| safe_cex_num::js_trim(&c.to_string()).is_empty())
            }),
            ClientOrderIdRuleOverrides {
                mint: Some(Rc::new(move || {
                    format!("tealstreet-{}", super::crypto::uuid(runtime.as_ref()))
                })),
                ..Default::default()
            },
        )),
    }
}

impl CoinbaseConstructionHost for Host {
    type Public = SocketHost;
    type Private = SocketHost;
    fn construct_base(
        &self,
        identity: ClassIdentity,
        _name: String,
        options: Frame,
        _store: Frame,
    ) -> Result<(), PortfolioFailure> {
        let (receiver, base) = base_exchange::create(
            &self.shared.sockets,
            BaseExchangeParts {
                name: self.shared.extra.name.clone(),
                options: self.shared.extra.options.clone(),
                store: self.shared.store.clone(),
                settings: settings(self),
                external_public_data: self.shared.extra.external_public_data.clone(),
            },
            from_typed(&self.shared.extra.name).map_err(from_thrown)?,
            to_js(&options),
            store::facade(self.shared.store.clone()).map_err(from_thrown)?,
            |_| Ok(()),
        )
        .map_err(from_thrown)?;
        if self.shared.http.hosted_signer().is_some()
            && let Frame::Object(fields) = &options
        {
            // Native bodies need only this capability's truthiness; executable
            // sign stays on BaseExchange's shared options.remoteSigner handle.
            fields.borrow_mut().insert(
                "remoteSigner".encode_utf16().collect(),
                Frame::Object(Rc::default()),
            );
        }
        if let Frame::Object(fields) = identity {
            fields.borrow_mut().insert(
                "name".encode_utf16().collect(),
                value::text(
                    &serde_json::to_value(&self.shared.extra.name)
                        .expect("name")
                        .as_str()
                        .expect("name literal"),
                ),
            );
        }
        self.shared
            .base
            .set(base)
            .map_err(|_| Host::json_error("Error", "Coinbase base constructed twice"))?;
        *self.shared.exchange.borrow_mut() = receiver;
        Ok(())
    }
    fn pattern_client_order_id_rule(
        &self,
        name: &'static str,
        pattern: &'static str,
        _mint: ClassMint,
    ) -> Result<Frame, PortfolioFailure> {
        Ok(Frame::from_json(
            &serde_json::json!({"label":name,"requirement":format!("match /{pattern}/")}),
        ))
    }
    fn uuid(&self) -> Result<String, PortfolioFailure> {
        Ok(super::crypto::uuid(self.shared.runtime.as_ref()))
    }
    fn create_secondary(
        &self,
        class: Rc<classes::CoinbaseExchange<Self>>,
        options: Frame,
        factory: ClassApiFactory,
        rate_limit: ClassRateLimit,
    ) -> Result<Frame, PortfolioFailure> {
        *self.shared.extra.class.borrow_mut() = Rc::downgrade(&class);
        let identity = class.identity.clone();
        let host = self.clone();
        let factory = Js::function("createAPI", move |_, args| {
            let options = value::from_js(&arg(&args, 0))
                .map_err(|e| Thrown::from(exchange_error(&e)))?
                .ok_or_else(|| Thrown::error("TypeError", "Missing Coinbase options"))?;
            host.client(
                &factory(options, identity.clone())
                    .map_err(|e| Thrown::from(exchange_error(&e)))?,
            )
            .map_err(|e| Thrown::from(exchange_error(&e)))
        });
        let host = self.clone();
        let limiter = Js::function("rateLimit", move |_, args| {
            let identity = host.register_client(arg(&args, 0));
            let limited = rate_limit(identity).map_err(|e| Thrown::from(exchange_error(&e)))?;
            host.client(&limited)
                .map_err(|e| Thrown::from(exchange_error(&e)))
        });
        let client = self
            .receiver()
            .call_method(
                "createXhrWithSecondaryKey",
                vec![
                    to_js(&options),
                    factory,
                    Js::object(vec![("rateLimit", limiter)]),
                ],
            )
            .map_err(from_thrown)?;
        Ok(self.register_client(client))
    }
    fn rate_limit(&self, client: Frame, max_rps: f64) -> Result<Frame, PortfolioFailure> {
        self.client(&client)?
            .call_method(
                "__configureRateLimit",
                vec![Js::object(vec![("maxRPS", Js::Number(max_rps))])],
            )
            .map_err(from_thrown)?;
        Ok(client)
    }
    fn construct_public_base(
        &self,
        _parent: ClassIdentity,
    ) -> Result<SocketHost, PortfolioFailure> {
        SocketHost::new(self.clone(), false)
    }
    fn construct_private_base(
        &self,
        _parent: ClassIdentity,
    ) -> Result<SocketHost, PortfolioFailure> {
        SocketHost::new(self.clone(), true)
    }
}
