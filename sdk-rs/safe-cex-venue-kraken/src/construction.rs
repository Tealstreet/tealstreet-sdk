//! Exchange constructor bodies over original superclass, API and rate-limit bindings.
use crate::rest_account::{Handle, ReadHost, ReadResult};
use crate::trading::units;
use std::rc::Rc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    Futures,
    Spot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseUrl {
    Derivatives,
    History,
}
pub type Factory<'a, H> =
    Rc<dyn Fn(Handle<H>, Handle<H>, BaseUrl) -> ReadResult<H, Handle<H>> + 'a>;
pub type RateLimit<'a, H> = Rc<dyn Fn(Handle<H>) -> ReadResult<H, Handle<H>> + 'a>;
pub trait ConstructorHost: ReadHost {
    fn super_exchange(
        &self,
        name: Handle<Self>,
        options: Handle<Self>,
        store: Handle<Self>,
    ) -> ReadResult<Self, Handle<Self>>;
    fn secondary_client<'a>(
        &'a self,
        options: Handle<Self>,
        receiver: Handle<Self>,
        factory: Factory<'a, Self>,
        rate_limit: RateLimit<'a, Self>,
    ) -> ReadResult<Self, Handle<Self>>;
    fn create_api(
        &self,
        venue: Venue,
        options: Handle<Self>,
        receiver: Handle<Self>,
        base: BaseUrl,
    ) -> ReadResult<Self, Handle<Self>>;
    fn rate_limit(&self, client: Handle<Self>, max_rps: f64) -> ReadResult<Self, Handle<Self>>;
}
pub(crate) fn construct_with<H: ConstructorHost>(
    h: &H,
    venue: Venue,
    name: Handle<H>,
    options: Handle<H>,
    store: Handle<H>,
    initialize: impl FnOnce(Handle<H>) -> ReadResult<H, ()>,
) -> ReadResult<H, Handle<H>> {
    let receiver = h.super_exchange(name, options.clone(), store)?;
    initialize(receiver.clone())?;
    let factory: Factory<'_, H> =
        Rc::new(move |options, receiver, base| h.create_api(venue, options, receiver, base));
    let rate_limit: RateLimit<'_, H> = Rc::new(move |client| h.rate_limit(client, 5.0));
    let client = h.secondary_client(options.clone(), receiver.clone(), factory, rate_limit)?;
    h.assign(&receiver, &units("xhr"), client)?;
    let client = h.create_api(
        venue,
        options.clone(),
        receiver.clone(),
        BaseUrl::Derivatives,
    )?;
    h.assign(&receiver, &units("unlimitedXHR"), client)?;
    if venue == Venue::Futures {
        let client = h.create_api(venue, options, receiver.clone(), BaseUrl::History)?;
        h.assign(&receiver, &units("historyXhr"), client)?;
    }
    Ok(receiver)
}
