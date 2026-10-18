use crate::command::get_command_executor;

pub type TelegramExecutor = Rc<
    dyn Fn(Rc<dyn Exchange>, String, String) -> Result<ExchangeFuture<'static, ()>, ExchangeError>,
>;
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_types::PositionSide;
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

#[derive(Clone, Deserialize)]
pub struct TelegramChat {
    pub id: f64,
}
#[derive(Clone, Deserialize)]
pub struct TelegramTextMessage {
    pub chat: TelegramChat,
    pub text: String,
}
#[derive(Clone, Deserialize)]
pub struct TelegramMessage {
    pub message: TelegramTextMessage,
    pub update_id: f64,
}

/// Imported Axios operations and timer ownership are supplied by the native host.
/// Request construction runs synchronously; returned futures represent settlement.
pub trait TelegramHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime>;
    /// A construction error corresponds to a throw before the source Promise exists.
    fn command_executor(&self) -> Option<TelegramExecutor> {
        get_command_executor().map(|executor| {
            Rc::new(move |exchange, symbol, command| Ok(executor(exchange, symbol, command, None)))
                as TelegramExecutor
        })
    }
    fn get_updates(
        &self,
        url: String,
    ) -> Result<ExchangeFuture<'static, Vec<TelegramMessage>>, ExchangeError>;
    fn post_message(
        &self,
        url: String,
        chat_id: f64,
        text: String,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError>;
}

/// Constructor inputs stay opaque until a runtime method needs the typed host ABI.
pub struct TelegramWorker<Token, ExchangeValue> {
    pub token: Token,
    pub get_exchange: Rc<dyn Fn(&TelegramWorker<Token, ExchangeValue>) -> ExchangeValue>,
    pub tick_counter: Cell<f64>,
    seen_messages: RefCell<HashSet<u64>>,
}
impl<Token, ExchangeValue> TelegramWorker<Token, ExchangeValue> {
    pub fn new(
        get_exchange: Rc<dyn Fn(&TelegramWorker<Token, ExchangeValue>) -> ExchangeValue>,
        token: Token,
    ) -> Rc<Self> {
        Rc::new(Self {
            token,
            get_exchange,
            tick_counter: Cell::new(0.0),
            seen_messages: RefCell::new(HashSet::new()),
        })
    }
    pub fn exchange(&self) -> ExchangeValue {
        (self.get_exchange)(self)
    }
    pub fn seen_count(&self) -> usize {
        self.seen_messages.borrow().len()
    }
    pub fn has_seen(&self, id: f64) -> bool {
        self.seen_messages.borrow().contains(&number_key(id))
    }
}
// Set<number> uses SameValueZero: all NaNs match, and +0/-0 are one key.
fn number_key(value: f64) -> u64 {
    if value.is_nan() {
        f64::NAN.to_bits()
    } else if value == 0.0 {
        0
    } else {
        value.to_bits()
    }
}
fn detach(host: Rc<dyn TelegramHost>, future: ExchangeFuture<'static, ()>, method: &'static str) {
    let runtime = host.runtime();
    let report = runtime.clone();
    runtime.spawn_local(Box::pin(async move {
        if let Err(error) = future.await {
            report.report_exception(&error, &serde_json::json!({"method":method}).into());
        }
    }));
}

impl<Token: AsRef<str> + 'static> TelegramWorker<Token, Result<Rc<dyn Exchange>, ExchangeError>> {
    pub fn start(self: &Rc<Self>, host: Rc<dyn TelegramHost>) {
        let future = self.tick(host.clone());
        detach(host, future, "TelegramWorker.start");
    }
    /// Source async method has no await in its loop: execute its prefix immediately.
    /// It intentionally does not add IDs to the seen set; tick owns that update.
    pub fn process_messages(
        self: &Rc<Self>,
        host: Rc<dyn TelegramHost>,
        messages: &[TelegramMessage],
    ) -> Result<(), ExchangeError> {
        for message in messages {
            if self.has_seen(message.update_id) {
                continue;
            }
            if message.message.text == "/positions" {
                let exchange = self.exchange()?;
                let text = exchange
                    .base()
                    .store
                    .positions()?
                    .iter()
                    .map(|position| {
                        format!(
                            "{} {} UPNL: {}",
                            position.symbol,
                            if position.side == PositionSide::Long {
                                "long"
                            } else {
                                "short"
                            },
                            safe_cex_num::js_number_string(position.unrealized_pnl)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let future = host.post_message(
                    format!(
                        "https://api.telegram.org/bot{}/sendMessage",
                        self.token.as_ref()
                    ),
                    message.message.chat.id,
                    text,
                )?;
                detach(host.clone(), future, "TelegramWorker.positionsPost");
            } else if message.message.text.starts_with("/cli ") {
                let rest = safe_cex_num::js_trim_start(&message.message.text[4..]);
                let parts = safe_cex_num::js_split_whitespace(rest);
                let symbol = parts[0];
                let command = parts[1..].join(" ");
                if symbol.is_empty() || command.is_empty() {
                    let future = host.post_message(
                        format!(
                            "https://api.telegram.org/bot{}/sendMessage",
                            self.token.as_ref()
                        ),
                        message.message.chat.id,
                        "Usage: /cli SYMBOL command [args]\nExample: /cli BTCUSDT buy 0.01".into(),
                    )?;
                    detach(host.clone(), future, "TelegramWorker.usagePost");
                    continue;
                }
                if let Some(executor) = host.command_executor() {
                    let future = match self
                        .exchange()
                        .and_then(|exchange| executor(exchange, symbol.to_owned(), command))
                    {
                        Ok(future) => future,
                        Err(error) => {
                            let result = self.exchange().and_then(|exchange| {
                                exchange.base().emit(
                                    "exec-log",
                                    safe_cex_exchange::js_runtime::value::Js::object(vec![(
                                        "message",
                                        safe_cex_exchange::js_runtime::value::Js::str(&format!(
                                            "Command failed: {}",
                                            error.message
                                        )),
                                    )]),
                                )
                            });
                            if let Err(error) = result {
                                detach(
                                    host.clone(),
                                    Box::pin(async move { Err(error) }),
                                    "TelegramWorker.command",
                                );
                            }
                            continue;
                        }
                    };
                    let worker = self.clone();
                    let task = Box::pin(async move {
                        let result = future.await;
                        if let Err(error) = result {
                            // The catch re-reads the live exchange, rather than the one used to execute.
                            worker.exchange()?.base().emit(
                                "exec-log",
                                safe_cex_exchange::js_runtime::value::Js::object(vec![(
                                    "message",
                                    safe_cex_exchange::js_runtime::value::Js::str(&format!(
                                        "Command failed: {}",
                                        error.message
                                    )),
                                )]),
                            )?;
                        }
                        Ok(())
                    });
                    detach(host.clone(), task, "TelegramWorker.command");
                }
            }
        }
        Ok(())
    }
    /// Start the GET during invocation, before the first await, as in the source.
    pub fn tick(self: &Rc<Self>, host: Rc<dyn TelegramHost>) -> ExchangeFuture<'static, ()> {
        let request = host.get_updates(format!(
            "https://api.telegram.org/bot{}/getUpdates",
            self.token.as_ref()
        ));
        let worker = self.clone();
        Box::pin(async move {
            let messages = request?.await?;
            let counter = worker.tick_counter.get() + 1.0;
            worker.tick_counter.set(counter);
            if counter < 10.0 {
                for message in &messages {
                    worker
                        .seen_messages
                        .borrow_mut()
                        .insert(number_key(message.update_id));
                }
            } else {
                // Source ignores the processMessages promise, then marks every ID seen.
                if let Err(error) = worker.process_messages(host.clone(), &messages) {
                    host.runtime().report_exception(
                        &error,
                        &serde_json::json!({"method":"TelegramWorker.processMessages"}).into(),
                    );
                }
                for message in messages {
                    worker
                        .seen_messages
                        .borrow_mut()
                        .insert(number_key(message.update_id));
                }
            }
            let sleep = host.runtime().sleep(1000);
            let timer_host = host.clone();
            detach(
                host,
                Box::pin(async move {
                    sleep.await?;
                    worker.tick(timer_host).await
                }),
                "TelegramWorker.tickTimer",
            );
            Ok(())
        })
    }
}
