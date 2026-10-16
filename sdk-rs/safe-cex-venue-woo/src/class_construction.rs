use crate::legacy_lifecycle::LifecycleOps;
use std::rc::Rc;

pub struct ConstructionOps<V, E> {
    pub value: Rc<LifecycleOps<V, E>>,
    /// Super evaluation binds this and initializes the active class's instance elements.
    pub super_construct: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub set: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
}
fn imported<V, E>(
    h: &ConstructionOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.import)(name)?;
    (h.value.value.invoke)(callee, (h.value.value.undefined)(), args()?)
}
fn rate_limit<V: Clone + 'static, E: 'static>(h: Rc<ConstructionOps<V, E>>) -> V {
    let value = h.value.value.clone();
    (value.function)(
        "rateLimit",
        1,
        Rc::new(move |args| {
            imported(&h, "rateLimit", || {
                let inner = imported(&h, "rateLimit", || {
                    Ok(vec![
                        args.first()
                            .cloned()
                            .unwrap_or_else(|| (h.value.value.undefined)()),
                        (h.value.value.object)(vec![
                            ("perMilliseconds", (h.value.value.number)(1000.)),
                            ("maxRequests", (h.value.value.number)(4.)),
                        ]),
                    ])
                })?;
                Ok(vec![
                    inner,
                    (h.value.value.object)(vec![
                        ("maxRequests", (h.value.value.number)(20.)),
                        ("perMilliseconds", (h.value.value.number)(6000.)),
                    ]),
                ])
            })
        }),
    )
}
// ts: WOOXExchange.constructor
pub fn exchange_constructor<V: Clone + 'static, E: 'static>(
    h: Rc<ConstructionOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let value = &h.value.value;
    let argument = |index| {
        args.get(index)
            .cloned()
            .unwrap_or_else(|| (value.undefined)())
    };
    let opts = argument(1);
    let receiver =
        (h.super_construct)(active, target, vec![argument(0), opts.clone(), argument(2)])?;
    let clients = imported(&h, "createApiWithSecondaryKey", || {
        Ok(vec![
            opts,
            receiver.clone(),
            (h.value.import)("createAPI")?,
            (value.object)(vec![("rateLimit", rate_limit(h.clone()))]),
        ])
    })?;
    let clients = (h.value.binding)(clients, &[true, true, true])?;
    for (key, client) in ["xhr", "_primaryXhr", "_secondaryXhr"]
        .into_iter()
        .zip(clients)
    {
        (h.set)(&receiver, key, client)?;
    }
    let proxy = imported(&h, "createAPI", || {
        let transport = imported(&h, "getTransport", || {
            Ok(vec![(value.get)(&receiver, "options")?])
        })?;
        let require_proxy = (value.get)(&transport, "requireProxy")?;
        let options = (value.get)(&receiver, "options")?;
        let options = (value.invoke)(require_proxy, transport, vec![options])?;
        Ok(vec![options, receiver.clone()])
    })?;
    (h.set)(&receiver, "_proxyEnforcedXhr", proxy)?;
    for (key, class) in [
        ("publicWebsocket", "WooPublicWebsocket"),
        ("privateWebsocket", "WooPrivateWebscoket"),
    ] {
        let constructor = (h.value.import)(class)?;
        let socket = (h.construct)(constructor, vec![receiver.clone()])?;
        (h.set)(&receiver, key, socket)?;
    }
    let on = (value.get)(&receiver, "on")?;
    (value.invoke)(
        on,
        receiver.clone(),
        vec![
            (value.string)("fill"),
            (value.get)(&receiver, "handleFillForPositionSync")?,
        ],
    )?;
    Ok(receiver)
}
