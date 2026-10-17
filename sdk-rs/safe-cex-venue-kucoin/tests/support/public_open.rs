use safe_cex_exchange::runtime::ExchangeError;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Error = Rc<ExchangeError>;
pub fn error() -> Error {
    let mut error = ExchangeError::new("public callback failure");
    error.name = "TypeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::Js::str("PUBLIC"));
    error.store_subscriber_ids = vec![7, 3];
    Rc::new(error)
}
pub fn open_cases<F>(kind: &str, regular: &[&str], mut run: F) -> (Vec<Value>, Vec<Value>)
where
    F: FnMut(
        &Cell<bool>,
        &mut dyn FnMut(&str) -> Result<(), Error>,
        &dyn Fn() -> bool,
    ) -> Result<(), Error>,
{
    let mut cases = vec![];
    let mut native = vec![];
    for disposed in [false, true] {
        for enabled in [false, true] {
            for flip in [-1, 0, regular.len() as i32 - 1] {
                for fail_at in [-1, 0, regular.len() as i32 - 1, regular.len() as i32] {
                    let is_disposed = Cell::new(disposed);
                    let flag = Cell::new(enabled);
                    let events = RefCell::new(vec![]);
                    let index = Cell::new(0);
                    let error = error();
                    let mut invoke = |action: &str| {
                        events.borrow_mut().push(action.to_owned());
                        let i = index.get();
                        index.set(i + 1);
                        if i == flip {
                            is_disposed.set(true);
                            flag.set(!flag.get());
                        }
                        if i == fail_at {
                            Err(error.clone())
                        } else {
                            Ok(())
                        }
                    };
                    let read_enabled = || {
                        events.borrow_mut().push("enabled".into());
                        flag.get()
                    };
                    let result = run(&is_disposed, &mut invoke, &read_enabled);
                    let mut expected = vec![];
                    let mut expected_flag = enabled;
                    let mut rejected = false;
                    if !disposed {
                        for (i, action) in regular.iter().enumerate() {
                            expected.push((*action).to_owned());
                            if i as i32 == flip {
                                expected_flag = !expected_flag;
                            }
                            if i as i32 == fail_at {
                                rejected = true;
                                break;
                            }
                        }
                        if !rejected {
                            expected.push("enabled".into());
                            if expected_flag {
                                expected.push("emitWsReconnected".into());
                                rejected = fail_at == regular.len() as i32;
                            }
                        }
                    }
                    assert_eq!(*events.borrow(), expected, "OPEN native call order {kind}");
                    assert_eq!(result.is_err(), rejected, "OPEN native rejection {kind}");
                    if let Err(actual) = &result {
                        assert!(
                            Rc::ptr_eq(actual, &error),
                            "OPEN native error pointer {kind}"
                        );
                        assert_eq!(actual, &error, "OPEN native complete error {kind}");
                    }
                    cases.push(json!({"kind":kind,"disposed":disposed,"enabled":enabled,"flip":flip,"failAt":fail_at}));
                    native.push(json!({"events":*events.borrow(),"error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
                }
            }
        }
    }
    assert_eq!(cases.len(), 48, "declared open profile floor {kind}");
    (cases, native)
}
