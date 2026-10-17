//! Private websocket lifecycle bodies at typed state and explicit host boundaries.
use crate::public_subscriptions::{
    ArraySlot, ChannelValue, SubscriptionFrame, SubscriptionParams, SubscriptionState,
    SubscriptionTransport,
};
use std::{
    cell::{Cell, RefCell},
    future::{Ready, ready},
    rc::Rc,
};

pub struct PrivateSocketState<W, H> {
    pub channels: SubscriptionState,
    pub is_disposed: Cell<bool>,
    pub is_authenticated: Cell<bool>,
    pub reconnect_auth_timer: RefCell<Option<Rc<H>>>,
    pub ws: RefCell<Option<Rc<W>>>,
}

pub trait ConnectionHost<W> {
    type Error;
    type Urls;
    fn testnet(&self) -> Result<bool, Self::Error>;
    fn urls(&self, environment: &'static str) -> Result<Self::Urls, Self::Error>;
    /// Resolve createWebsocket, read USER on the original urls object, then invoke.
    fn create(&self, urls: Self::Urls) -> Result<Rc<W>, Self::Error>;
    /// Resolve addEventListener and the matching this.onX handle; preserve both receivers.
    fn add_listener(&self, socket: Option<Rc<W>>, event: &'static str) -> Result<(), Self::Error>;
}

pub fn connect_and_subscribe<W, H, T: ConnectionHost<W>>(
    state: &PrivateSocketState<W, H>,
    host: &T,
) -> Ready<Result<(), T::Error>> {
    // JavaScript async bodies execute immediately until their first await (none here).
    ready((|| {
        if !state.is_disposed.get() {
            let environment = if host.testnet()? {
                "TESTNET"
            } else {
                "PRODUCTION"
            };
            let urls = host.urls(environment)?;
            let socket = host.create(urls)?;
            state.ws.replace(Some(socket));
            for event in ["open", "message", "close"] {
                let socket = state.ws.borrow().clone();
                host.add_listener(socket, event)?;
            }
        }
        Ok(())
    })())
}

pub trait TimerHost<H> {
    type Error;
    type AuthReturn;
    fn track_last_received(&self) -> Result<(), Self::Error>;
    fn clear_timeout(&self, timer: Option<Rc<H>>) -> Result<(), Self::Error>;
    fn set_timeout<F: Fn() -> Result<(), Self::Error> + 'static>(
        &self,
        callback: F,
        delay: f64,
    ) -> Result<Rc<H>, Self::Error>;
    /// Includes this.authenticate lookup/call; returned promise/value is ignored by callback.
    fn authenticate(&self) -> Result<Self::AuthReturn, Self::Error>;
}

pub fn run_reconnect_auth<W, H, R, E>(
    state: &PrivateSocketState<W, H>,
    authenticate: impl FnOnce() -> Result<R, E>,
) -> Result<(), E> {
    if !state.is_disposed.get() {
        authenticate()?;
    }
    Ok(())
}

pub fn on_open<W: 'static, H: 'static, T: TimerHost<H> + 'static>(
    state: Rc<PrivateSocketState<W, H>>,
    host: Rc<T>,
) -> Result<(), T::Error> {
    if !state.is_disposed.get() {
        host.track_last_received()?;
        state.is_authenticated.set(false);
        let timer = state.reconnect_auth_timer.borrow().clone();
        host.clear_timeout(timer)?;
        let callback_state = state.clone();
        let callback_host = host.clone();
        let timer = host.set_timeout(
            move || run_reconnect_auth(&callback_state, || callback_host.authenticate()),
            1000.0,
        )?;
        state.reconnect_auth_timer.replace(Some(timer));
    }
    Ok(())
}

pub trait AuthenticationHost {
    type Error;
    type Message;
    type Send;
    type Emit;
    /// Includes generator resolution and key/secret property reads/call.
    fn generate(&self) -> Result<Self::Message, Self::Error>;
    fn prepare_send(&self) -> Result<Self::Send, Self::Error>;
    fn stringify_and_send(
        &self,
        send: Self::Send,
        message: Self::Message,
    ) -> Result<(), Self::Error>;
    fn prepare_error_emit(&self) -> Result<Self::Emit, Self::Error>;
    fn error_as_template(&self, error: Self::Error) -> Result<Vec<u16>, Self::Error>;
    fn emit_error(&self, emit: Self::Emit, message: Vec<u16>) -> Result<(), Self::Error>;
}

pub fn authenticate<T: AuthenticationHost>(host: &T) -> Ready<Result<(), T::Error>> {
    ready((|| {
        let result = (|| {
            let message = host.generate()?;
            let send = host.prepare_send()?;
            host.stringify_and_send(send, message)
        })();
        if let Err(error) = result {
            // Catch-body effects/errors lie outside the source try block.
            let emit = host.prepare_error_emit()?;
            let message = "Failed to authenticate WebSocket: "
                .encode_utf16()
                .chain(host.error_as_template(error)?)
                .collect();
            host.emit_error(emit, message)?;
        }
        Ok(())
    })())
}

fn values<W, H>(
    state: &PrivateSocketState<W, H>,
) -> Rc<crate::public_subscriptions::OrderedStringSet> {
    state.channels.subscriptions.borrow().clone()
}
fn send_channels<W, H, T: SubscriptionTransport>(
    state: &PrivateSocketState<W, H>,
    channels: Vec<ArraySlot>,
    transport: &T,
) -> Result<(), T::Error> {
    let prepared = transport.prepare()?;
    let id = state.channels.message_id.get();
    state.channels.message_id.set(id + 1.0);
    let frame = SubscriptionFrame {
        id,
        method: "subscribe",
        params: SubscriptionParams { channels },
        nonce: transport.clock()?,
    };
    transport.send(prepared, frame)
}

pub fn resubscribe<W, H, T: SubscriptionTransport>(
    state: &PrivateSocketState<W, H>,
    transport: &T,
) -> Ready<Result<(), T::Error>> {
    ready((|| {
        if state.is_authenticated.get() && !values(state).is_empty() {
            let prepared = transport.prepare()?;
            let id = state.channels.message_id.get();
            state.channels.message_id.set(id + 1.0);
            let channels = values(state)
                .values()
                .into_iter()
                .map(ArraySlot::Value)
                .collect();
            let frame = SubscriptionFrame {
                id,
                method: "subscribe",
                params: SubscriptionParams { channels },
                nonce: transport.clock()?,
            };
            transport.send(prepared, frame)?;
        }
        Ok(())
    })())
}

pub fn subscribe_to_order_updates<W, H, T: SubscriptionTransport>(
    state: &PrivateSocketState<W, H>,
    transport: &T,
) -> Result<(), T::Error> {
    let channels = ["user.order", "user.advance.order"]
        .map(|channel| ChannelValue::String(channel.encode_utf16().collect()));
    let new_channels: Vec<_> = channels
        .into_iter()
        .filter(|channel| !values(state).contains(channel))
        .collect();
    for channel in &new_channels {
        values(state).insert(channel.clone());
    }
    if state.is_authenticated.get() && !new_channels.is_empty() {
        send_channels(
            state,
            new_channels.into_iter().map(ArraySlot::Value).collect(),
            transport,
        )?;
    }
    Ok(())
}
fn subscribe_one<W, H, T: SubscriptionTransport>(
    state: &PrivateSocketState<W, H>,
    channel: &str,
    transport: &T,
) -> Result<(), T::Error> {
    let channel = ChannelValue::String(channel.encode_utf16().collect());
    if !values(state).contains(&channel) {
        values(state).insert(channel.clone());
        if state.is_authenticated.get() {
            send_channels(state, vec![ArraySlot::Value(channel)], transport)?;
        }
    }
    Ok(())
}
pub fn subscribe_to_position_updates<W, H, T: SubscriptionTransport>(
    state: &PrivateSocketState<W, H>,
    transport: &T,
) -> Result<(), T::Error> {
    subscribe_one(state, "user.positions", transport)
}
pub fn subscribe_to_balance_updates<W, H, T: SubscriptionTransport>(
    state: &PrivateSocketState<W, H>,
    transport: &T,
) -> Result<(), T::Error> {
    subscribe_one(state, "user.balance", transport)
}

pub trait UnsubscribeChannels<E> {
    type Native;
    fn length(&self) -> Result<f64, E>;
    /// Return the original array carrier; do not snapshot or rebuild it.
    fn native(&self) -> Self::Native;
    /// Source forEach resolution/lookup, captured length, holes/inherited slots and errors.
    fn for_each<F: FnMut(ChannelValue) -> Result<bool, E>>(&self, callback: F) -> Result<(), E>;
}
pub fn unsubscribe<
    W,
    H,
    E,
    A: UnsubscribeChannels<E>,
    T: SubscriptionTransport<A::Native, Error = E>,
>(
    state: &PrivateSocketState<W, H>,
    channels: &A,
    transport: &T,
) -> Result<(), E> {
    if channels.length()? > 0.0 {
        let prepared = transport.prepare()?;
        let id = state.channels.message_id.get();
        state.channels.message_id.set(id + 1.0);
        let frame = SubscriptionFrame {
            id,
            method: "unsubscribe",
            params: SubscriptionParams {
                channels: channels.native(),
            },
            nonce: transport.clock()?,
        };
        transport.send(prepared, frame)?;
        channels.for_each(|channel| Ok(values(state).remove(&channel)))?;
    }
    Ok(())
}

pub trait DisposeHost<W, H> {
    type Error;
    fn clear_timeout(&self, timer: Option<Rc<H>>) -> Result<(), Self::Error>;
    /// Includes the fresh this.ws lookup and close resolution/call after the truthy guard.
    fn close(&self, socket: Option<Rc<W>>) -> Result<(), Self::Error>;
}
pub fn dispose<W, H, T: DisposeHost<W, H>>(
    state: &PrivateSocketState<W, H>,
    host: &T,
) -> Result<(), T::Error> {
    let timer = state.reconnect_auth_timer.borrow().clone();
    host.clear_timeout(timer)?;
    values(state).clear();
    state.is_authenticated.set(false);
    if state.ws.borrow().is_some() {
        let socket = state.ws.borrow().clone();
        host.close(socket)?;
    }
    Ok(())
}

#[cfg(test)]
mod accepted_helpers {
    use super::*;
    use crate::public_subscriptions::OrderedStringSet;
    type State = PrivateSocketState<(), ()>;
    type Error = Rc<()>;
    struct Transport {
        state: Rc<State>,
        channel: Vec<u16>,
        events: RefCell<Vec<&'static str>>,
        prepared: Rc<()>,
        error: Error,
        fail: Option<&'static str>,
    }
    impl Transport {
        fn event(&self, event: &'static str) -> Result<(), Error> {
            self.events.borrow_mut().push(event);
            if self.fail == Some(event) {
                Err(self.error.clone())
            } else {
                Ok(())
            }
        }
    }
    impl SubscriptionTransport for Transport {
        type Prepared = Rc<()>;
        type Error = Error;
        fn prepare(&self) -> Result<Rc<()>, Error> {
            assert!(
                values(&self.state).contains(&ChannelValue::String(self.channel.clone())),
                "private subscribe native insertion before prepare"
            );
            self.event("prepare")?;
            Ok(self.prepared.clone())
        }
        fn clock(&self) -> Result<f64, Error> {
            self.event("clock")?;
            Ok(-0.)
        }
        fn send(&self, prepared: Rc<()>, frame: SubscriptionFrame) -> Result<(), Error> {
            assert!(
                Rc::ptr_eq(&prepared, &self.prepared),
                "private subscribe native prepared identity"
            );
            assert_eq!(
                frame.id.to_bits(),
                0x8000000000000000,
                "private subscribe native id bits"
            );
            assert_eq!(
                frame.nonce.to_bits(),
                0x8000000000000000,
                "private subscribe native nonce bits"
            );
            assert_eq!(frame.method, "subscribe", "private subscribe native method");
            assert_eq!(
                frame.params.channels,
                vec![ArraySlot::Value(ChannelValue::String(self.channel.clone()))],
                "private subscribe native frame channel"
            );
            self.event("send")
        }
    }
    #[test]
    fn subscribe_one_native_contract() {
        let cases = [
            (false, false, false, None),
            (true, false, false, None),
            (true, true, false, None),
            (false, true, false, None),
            (true, false, true, None),
            (true, false, false, Some("prepare")),
            (true, false, false, Some("clock")),
            (true, false, false, Some("send")),
        ];
        assert_eq!(cases.len(), 8, "private subscribe independent cases");
        let mut executed = 0;
        for (auth, existing, disposed, fail) in cases {
            let retained = Rc::new(OrderedStringSet::default());
            let expected = vec![117, 115, 101, 114, 46, 937];
            if existing {
                retained.insert(ChannelValue::String(expected.clone()));
            }
            let state = Rc::new(State {
                channels: SubscriptionState {
                    subscriptions: RefCell::new(retained.clone()),
                    message_id: Cell::new(-0.),
                },
                is_disposed: Cell::new(disposed),
                is_authenticated: Cell::new(auth),
                reconnect_auth_timer: RefCell::new(None),
                ws: RefCell::new(None),
            });
            let transport = Transport {
                state: state.clone(),
                channel: expected.clone(),
                events: RefCell::new(vec![]),
                prepared: Rc::new(()),
                error: Rc::new(()),
                fail,
            };
            let result = subscribe_one(&state, "user.Ω", &transport);
            if fail.is_some() {
                match result {
                    Err(error) => assert!(
                        Rc::ptr_eq(&error, &transport.error),
                        "private subscribe native original error"
                    ),
                    Ok(()) => panic!("private subscribe swallowed error"),
                }
            } else {
                result.unwrap()
            }
            assert_eq!(
                retained.values(),
                vec![ChannelValue::String(expected)],
                "private subscribe native membership"
            );
            assert!(
                Rc::ptr_eq(&values(&state), &retained),
                "private subscribe native retained set"
            );
            let sends = auth && !existing;
            let expected_events = if !sends {
                &[][..]
            } else {
                match fail {
                    Some("prepare") => &["prepare"][..],
                    Some("clock") => &["prepare", "clock"][..],
                    _ => &["prepare", "clock", "send"][..],
                }
            };
            assert_eq!(
                &*transport.events.borrow(),
                expected_events,
                "private subscribe native event order"
            );
            let expected_id: f64 = if sends && fail != Some("prepare") {
                1.
            } else {
                -0.
            };
            assert_eq!(
                state.channels.message_id.get().to_bits(),
                expected_id.to_bits(),
                "private subscribe native next id"
            );
            assert_eq!(
                state.is_authenticated.get(),
                auth,
                "private subscribe auth preserved"
            );
            assert_eq!(
                state.is_disposed.get(),
                disposed,
                "private subscribe disposal flag preserved"
            );
            executed += 1;
        }
        assert_eq!(executed, 8, "private subscribe executed cases");
    }
}
