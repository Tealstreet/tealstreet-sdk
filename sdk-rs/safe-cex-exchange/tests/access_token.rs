use safe_cex_exchange::access_token::*;
use safe_cex_store::virtual_clock::VirtualClock;
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(waker)) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready fixture"),
    }
}
const VALID: &str = "e30.eyJleHAiOjQxMDI0NDQ4MDB9.x";
#[test]
fn contextual_token_owns_call_and_preserves_error_identity() {
    let access = AccessToken::new(None, VirtualClock::default(), false);
    access.set_proxy_token("someone else's session".into());
    assert_eq!(
        ready(access.get_proxy_token()).unwrap(),
        "someone else's session"
    );
    let refreshes = Rc::new(Cell::new(0));
    let count = refreshes.clone();
    access.set_token_refresh_function(Some(Rc::new(move || {
        count.set(count.get() + 1);
        Box::pin(async { Ok(VALID.into()) })
    })));
    access.set_contextual_proxy_token_provider(Some(Rc::new(|| {
        Ok(Some(Rc::new(|| Box::pin(async { Ok("expired".into()) }))))
    })));
    let error = ready(access.get_proxy_token()).unwrap_err();
    assert_eq!(error.name, "ProxyTokenUnavailableError");
    assert_eq!(
        error.message,
        "proxy token missing or expired for this request context"
    );
    assert_eq!(refreshes.get(), 0);
    assert_eq!(access.current_proxy_token(), "someone else's session");
    let sentinel = Rc::new(ProxyTokenUnavailableError::new(Some("specific reason")));
    let clone = sentinel.clone();
    access.set_contextual_proxy_token_provider(Some(Rc::new(move || {
        let error = clone.clone();
        Ok(Some(Rc::new(move || {
            let error = error.clone();
            Box::pin(async move { Err(error) })
        })))
    })));
    assert!(Rc::ptr_eq(
        &ready(access.refresh_proxy_token()).unwrap_err(),
        &sentinel
    ));
    let arbitrary = Rc::new(ProxyTokenUnavailableError {
        name: "ProviderError".into(),
        message: "token-bearing detail".into(),
    });
    access.set_contextual_proxy_token_provider(Some(Rc::new(move || {
        let error = arbitrary.clone();
        Ok(Some(Rc::new(move || {
            let error = error.clone();
            Box::pin(async move { Err(error) })
        })))
    })));
    assert_eq!(
        ready(access.get_proxy_token()).unwrap_err().message,
        "proxy token provider failed"
    );
    access.set_contextual_proxy_token_provider(Some(Rc::new(|| {
        Ok(Some(Rc::new(|| Box::pin(async { Ok(VALID.into()) }))))
    })));
    assert_eq!(ready(access.refresh_proxy_token()).unwrap(), VALID);
    assert_eq!(access.current_proxy_token(), "someone else's session");
    access.set_contextual_proxy_token_provider(None);
    assert_eq!(ready(access.get_proxy_token()).unwrap(), VALID);
    assert_eq!(refreshes.get(), 1);
    assert_eq!(ready(access.get_proxy_token()).unwrap(), VALID);
    assert_eq!(refreshes.get(), 1);
    access.set_token_refresh_function(Some(Rc::new(|| Box::pin(async { Ok(String::new()) }))));
    assert_eq!(ready(access.refresh_proxy_token()).unwrap(), VALID);
    access.set_token_refresh_function(Some(Rc::new(|| {
        Box::pin(async { Err(Rc::new(ProxyTokenUnavailableError::new(None))) })
    })));
    assert_eq!(ready(access.refresh_proxy_token()).unwrap(), VALID);
    access.set_token_refresh_function(None);
    ready(access.initialize_proxy_token());
    assert_eq!(
        ProxyTokenUnavailableError::new(None).message,
        "Tealstreet proxy token unavailable"
    );
}
