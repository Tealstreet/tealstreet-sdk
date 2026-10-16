/// Host value operations preserve property/prototype lookup, function identity and live iteration.
/// Array filter invokes the supplied native predicate; iterator closing preserves source exceptions.
pub struct WeexMessageHandlerRuntime<H, V, I, M, E> {
    pub registry: fn(&mut H) -> Result<V, E>,
    pub get_property: fn(&mut H, &V, &[u16]) -> Result<V, E>,
    pub set_property: fn(&mut H, &V, &[u16], V) -> Result<(), E>,
    pub truthy: fn(&V) -> bool,
    pub nullish: fn(&V) -> bool,
    pub array: fn(&mut H, Vec<V>) -> Result<V, E>,
    pub spread: fn(&mut H, &V) -> Result<Vec<V>, E>,
    pub filter: fn(&mut H, &V, &mut dyn FnMut(&V) -> bool) -> Result<V, E>,
    pub strict_equal: fn(&V, &V) -> bool,
    pub iterator: fn(&mut H, &V) -> Result<I, E>,
    pub next: fn(&mut H, &mut I) -> Result<Option<V>, E>,
    pub close_on_throw: fn(&mut H, &mut I, E) -> E,
    pub invoke: fn(&mut H, &V, &M) -> Result<(), E>,
    pub cleanup_stats: fn(&mut H, &[u16]) -> Result<(), E>,
}

pub fn add_handler<H, V, I, M, E>(
    host: &mut H,
    key: &[u16],
    handler: V,
    runtime: &WeexMessageHandlerRuntime<H, V, I, M, E>,
) -> Result<(), E> {
    let assignment_registry = (runtime.registry)(host)?;
    let read_registry = (runtime.registry)(host)?;
    let mut previous = (runtime.get_property)(host, &read_registry, key)?;
    if !(runtime.truthy)(&previous) {
        previous = (runtime.array)(host, Vec::new())?;
    }
    let mut handlers = (runtime.spread)(host, &previous)?;
    handlers.push(handler);
    let handlers = (runtime.array)(host, handlers)?;
    (runtime.set_property)(host, &assignment_registry, key, handlers)
}

pub fn remove_handler<H, V, I, M, E>(
    host: &mut H,
    key: &[u16],
    handler: &V,
    runtime: &WeexMessageHandlerRuntime<H, V, I, M, E>,
) -> Result<(), E> {
    let assignment_registry = (runtime.registry)(host)?;
    let read_registry = (runtime.registry)(host)?;
    let mut previous = (runtime.get_property)(host, &read_registry, key)?;
    if !(runtime.truthy)(&previous) {
        previous = (runtime.array)(host, Vec::new())?;
    }
    let handlers = (runtime.filter)(host, &previous, &mut |item| {
        !(runtime.strict_equal)(item, handler)
    })?;
    (runtime.set_property)(host, &assignment_registry, key, handlers)?;
    let registry = (runtime.registry)(host)?;
    let handlers = (runtime.get_property)(host, &registry, key)?;
    let length = (runtime.get_property)(
        host,
        &handlers,
        &"length".encode_utf16().collect::<Vec<_>>(),
    )?;
    if !(runtime.truthy)(&length) {
        (runtime.cleanup_stats)(host, key)?;
    }
    Ok(())
}

pub fn handle_routed_message<H, V, I, M, E>(
    host: &mut H,
    key: &[u16],
    message: &M,
    runtime: &WeexMessageHandlerRuntime<H, V, I, M, E>,
) -> Result<(), E> {
    let registry = (runtime.registry)(host)?;
    let handlers = (runtime.get_property)(host, &registry, key)?;
    if (runtime.nullish)(&handlers) {
        return Ok(());
    }
    let length = (runtime.get_property)(
        host,
        &handlers,
        &"length".encode_utf16().collect::<Vec<_>>(),
    )?;
    if !(runtime.truthy)(&length) {
        return Ok(());
    }
    let mut iterator = (runtime.iterator)(host, &handlers)?;
    while let Some(handler) = (runtime.next)(host, &mut iterator)? {
        if let Err(error) = (runtime.invoke)(host, &handler, message) {
            return Err((runtime.close_on_throw)(host, &mut iterator, error));
        }
    }
    Ok(())
}
