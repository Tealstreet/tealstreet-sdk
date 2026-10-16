//! Order-result selection over fallible source value primitives.
pub struct WeexOrderResultRuntime<H, V, E> {
    pub optional_property: fn(&mut H, &V, &str) -> Result<V, E>,
    pub is_false: fn(&V) -> bool,
    pub truthy: fn(&V) -> bool,
    pub string: fn(&mut H, &str) -> Result<V, E>,
    pub emit_error: fn(&mut H, &V) -> Result<(), E>,
    pub new_error: fn(&mut H, &V) -> E,
    pub uuid: fn(&mut H) -> Result<V, E>,
    pub integer_string: fn(&mut H, &V) -> Result<Vec<u16>, E>,
}

pub fn parse_order_result<H, V, E>(
    host: &mut H,
    runtime: &WeexOrderResultRuntime<H, V, E>,
    result: &V,
    fallback_id: V,
) -> Result<Vec<u16>, E> {
    let success = (runtime.optional_property)(host, result, "success")?;
    if (runtime.is_false)(&success) {
        let mut message = (runtime.optional_property)(host, result, "errorMessage")?;
        if !(runtime.truthy)(&message) {
            message = (runtime.optional_property)(host, result, "errorCode")?;
            if !(runtime.truthy)(&message) {
                message = (runtime.string)(host, "WEEX order rejected")?;
            }
        }
        (runtime.emit_error)(host, &message)?;
        return Err((runtime.new_error)(host, &message));
    }
    let mut id = (runtime.optional_property)(host, result, "orderId")?;
    if !(runtime.truthy)(&id) {
        id = (runtime.optional_property)(host, result, "clientOrderId")?;
        if !(runtime.truthy)(&id) {
            id = fallback_id;
            if !(runtime.truthy)(&id) {
                id = (runtime.uuid)(host)?;
            }
        }
    }
    (runtime.integer_string)(host, &id)
}
