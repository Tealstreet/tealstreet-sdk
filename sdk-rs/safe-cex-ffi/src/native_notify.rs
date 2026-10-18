//! One account FIFO for worker events and Store slices. Independent TSFNs can
//! overtake each other even when both are called on the same native host thread.
use crate::native_value::JsData;
use napi::{
    Env, Status,
    bindgen_prelude::{Function, Unknown},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use std::sync::Arc;

pub(crate) type Listener<'env> = Function<'env, Unknown<'static>, Unknown<'static>>;

enum Delivery {
    Event(JsData),
    Store(JsData),
}
type Queue = ThreadsafeFunction<Delivery, Unknown<'static>, (bool, JsData), Status, false>;

pub(crate) struct Notify {
    queue: Arc<Queue>,
    store: bool,
}
impl Notify {
    pub(crate) fn call(&self, data: JsData, mode: ThreadsafeFunctionCallMode) -> Status {
        self.queue.call(
            if self.store {
                Delivery::Store(data)
            } else {
                Delivery::Event(data)
            },
            mode,
        )
    }
}

pub(crate) fn ordered(
    env: &Env,
    on_event: Listener<'_>,
    on_store: Listener<'_>,
) -> napi::Result<(Notify, Notify)> {
    let on_event = on_event.create_ref()?;
    let on_store = on_store.create_ref()?;
    // The wrapper owns both references until the account releases its last
    // producer and the single queue has delivered its already-enqueued values.
    let dispatcher = env.create_function_from_closure::<(bool, JsData), Unknown<'static>, _>(
        "nativeAccountNotify",
        move |context| {
            let callback = if context.first_arg::<bool>()? {
                &on_store
            } else {
                &on_event
            };
            // Forward the projected JS value directly: decoding it into JsData
            // here would add a second traversal and lose object identity.
            callback.borrow_back(context.env)?.call(context.get(1)?)
        },
    )?;
    let queue = Arc::new(
        dispatcher
            .build_threadsafe_function::<Delivery>()
            .callee_handled::<false>()
            .build_callback(|context| {
                Ok(match context.value {
                    Delivery::Event(data) => (false, data),
                    Delivery::Store(data) => (true, data),
                })
            })?,
    );
    Ok((
        Notify {
            queue: queue.clone(),
            store: false,
        },
        Notify { queue, store: true },
    ))
}
