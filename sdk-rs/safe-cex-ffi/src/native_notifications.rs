use crate::native_value::{JsData, check_status};
use napi::{
    Env, JsValue, Status,
    bindgen_prelude::{FnArgs, Function, Unknown},
    sys,
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};

pub(crate) struct Notifications {
    queue: ThreadsafeFunction<FnArgs<(bool, JsData)>, (), FnArgs<(bool, JsData)>, Status, false>,
}

impl Notifications {
    pub(crate) fn new(
        env: &Env,
        on_event: Function<'_, Unknown<'static>, Unknown<'static>>,
        on_store: Function<'_, Unknown<'static>, Unknown<'static>>,
    ) -> napi::Result<Self> {
        let on_event = on_event.create_ref()?;
        let on_store = on_store.create_ref()?;
        let dispatch = env.create_function_from_closure::<FnArgs<(bool, JsData)>, (), _>(
            "nativeAccountNotification",
            move |context| {
                let callback = if context.get::<bool>(0)? {
                    &on_store
                } else {
                    &on_event
                };
                let callback = callback.borrow_back(context.env)?;
                let value = context.get::<Unknown<'static>>(1)?;
                let arguments = [value.raw()];
                let mut receiver = std::ptr::null_mut();
                check_status(unsafe { sys::napi_get_undefined(context.env.raw(), &mut receiver) })?;
                let mut ignored = std::ptr::null_mut();
                // Leave a thrown value pending: Function::call would inspect it.
                // The outer ThreadsafeFunction surfaces the original exception.
                check_status(unsafe {
                    sys::napi_call_function(
                        context.env.raw(),
                        receiver,
                        callback.raw(),
                        arguments.len(),
                        arguments.as_ptr(),
                        &mut ignored,
                    )
                })
            },
        )?;
        Ok(Self {
            queue: dispatch
                .build_threadsafe_function::<FnArgs<(bool, JsData)>>()
                .callee_handled::<false>()
                .max_queue_size::<0>()
                .build()?,
        })
    }

    pub(crate) fn event(&self, value: JsData) -> Status {
        self.deliver(false, value)
    }

    pub(crate) fn store(&self, value: JsData) -> Status {
        self.deliver(true, value)
    }

    fn deliver(&self, store: bool, value: JsData) -> Status {
        let status = self
            .queue
            .call((store, value).into(), ThreadsafeFunctionCallMode::NonBlocking);
        if status != Status::Ok {
            let channel = if store { "Store" } else { "event" };
            eprintln!("native account {channel} notification delivery failed: {status:?}");
        }
        status
    }
}
