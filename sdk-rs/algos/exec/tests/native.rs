#![cfg(not(target_arch = "wasm32"))]
use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::{Outcome, Partial, VenueError};
use safe_cex_exec::native::{ExecFactory, serialize_error, translate};
use serde_json::{Value, json};
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;

#[derive(Default)]
struct Port {
    store: RefCell<AlgoStore>,
    next: Cell<u32>,
    commands: RefCell<Vec<Command>>,
    replies: RefCell<Vec<oneshot::Sender<Result<String, VenueError>>>>,
    events: RefCell<Vec<(String, Value)>>,
}
impl AccountPort for Port {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        mpsc::unbounded_channel().1
    }
    fn next_command_id(&self) -> CommandId {
        let n = self.next.get() + 1;
        self.next.set(n);
        n
    }
    fn command(&self, command: Command) -> CommandFuture {
        self.commands.borrow_mut().push(command);
        let (tx, rx) = oneshot::channel();
        self.replies.borrow_mut().push(tx);
        Box::pin(async move {
            rx.await
                .unwrap_or_else(|_| Err(VenueError::new("lost").outcome(Outcome::Unknown)))
        })
    }
    fn emit(&self, op: &str, data: String) {
        self.events
            .borrow_mut()
            .push((op.into(), serde_json::from_str(&data).unwrap()));
    }
}

#[tokio::test]
async fn actual_shared_runtime_delivers_typed_engine_commands_and_wire_reply() {
    LocalSet::new().run_until(async {
  let port=Rc::new(Port::default());let cx=AlgoCx::new(port.clone(),Scope::new());
  ExecFactory.handle("exec",r#"{"symbol":"BTCUSDT","command":"cancel; lev 3","reqId":"r"}"#,&cx).unwrap();
  tokio::task::yield_now().await;
  assert!(matches!(&port.commands.borrow()[0],Command::CancelSymbolOrders{symbol,..} if symbol=="BTCUSDT"));
  assert_eq!(port.commands.borrow().len(),1);
  port.replies.borrow_mut().remove(0).send(Ok("[]".into())).unwrap();
  tokio::task::yield_now().await;tokio::task::yield_now().await;
  assert!(matches!(&port.commands.borrow()[1],Command::SetLeverage{symbol,leverage,..} if symbol=="BTCUSDT" && *leverage==3.));
  port.replies.borrow_mut().remove(0).send(Ok("[]".into())).unwrap();
  tokio::task::yield_now().await;
  assert!(port.events.borrow().iter().any(|(op,data)|op=="exec-result" && data==&json!({"reqId":"r","handled":true,"success":true})));
 }).await;
}

#[tokio::test]
async fn closing_shared_scope_during_a_command_never_issues_the_successor() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            ExecFactory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"cancel; lev 9","reqId":"r"}"#,
                    &cx,
                )
                .unwrap();
            tokio::task::yield_now().await;
            assert_eq!(port.commands.borrow().len(), 1);
            cx.scope.dispose();
            tokio::task::yield_now().await;
            assert!(
                port.replies
                    .borrow_mut()
                    .remove(0)
                    .send(Ok("[]".into()))
                    .is_err()
            );
            tokio::task::yield_now().await;
            assert_eq!(port.commands.borrow().len(), 1);
            assert!(
                port.events
                    .borrow()
                    .iter()
                    .all(|(op, _)| op != "exec-result")
            );
        })
        .await;
}

#[test]
fn cancellation_keeps_routing_symbols_and_partial_errors_keep_real_ids() {
    let command = translate(
        "cancelOrders",
        &json!([[{"id":"7","symbol":"BTC-PERP"}]]),
        1,
    )
    .unwrap();
    assert!(
        matches!(command,Command::CancelOrdersWithSymbols{orders,..} if orders[0].symbol=="BTC-PERP" && orders[0].id=="7")
    );
    let mut error = VenueError::new("lost").outcome(Outcome::Unknown);
    error.partial = Some(Box::new(Partial {
        order_ids: vec!["7".into()],
        total: Some(2),
        failed_index: Some(1),
    }));
    let data = serialize_error(error);
    assert_eq!(data["partialOrderIds"], json!(["7"]));
    assert_eq!(data["totalOrderCount"], 2);
    assert_eq!(data["outcome"], "unknown");
    assert!(translate("invented", &json!([]), 2).is_err());
}
