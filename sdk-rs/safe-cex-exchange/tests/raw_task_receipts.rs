#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    native::{native_replay_runtime, replay_raw_tasks},
    replay_clock,
    runtime::{ExchangeRuntime, RuntimeTaskKind},
    websocket::{WebSocketConnectionOptions, WsRuntime},
};
use std::{cell::Cell, rc::Rc};

async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

#[test]
fn unknown_and_finite_work_block_even_when_awaiting_future_timers() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        replay_clock::activate(runtime.replay_clock.as_ref().unwrap()).unwrap();
        let sleep = runtime.sleep(100);
        let unknown = runtime.spawn_local(Box::pin(async move {
            sleep.await.unwrap();
        }));
        let sleep = runtime.sleep(100);
        let finite = runtime.spawn_classified(
            RuntimeTaskKind::Finite,
            Box::pin(async move {
                sleep.await.unwrap();
            }),
        );
        settle().await;
        let receipt = replay_raw_tasks(&runtime);
        assert_eq!(receipt["unknown"], 1);
        assert_eq!(receipt["finite"], 1);
        assert_eq!(receipt["waitingTimers"], 0);
        assert_eq!(receipt["blocking"], 2);
        unknown.cancel();
        finite.cancel();
        settle().await;
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
    });
}

#[test]
fn timer_wakeup_and_pending_callback_block_until_completion() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        replay_clock::activate(runtime.replay_clock.as_ref().unwrap()).unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let entered = Rc::new(Cell::new(false));
        let callback = entered.clone();
        let sleep = runtime.sleep(100);
        let _task = runtime.spawn_classified(
            RuntimeTaskKind::Timer,
            Box::pin(async move {
                sleep.await.unwrap();
                callback.set(true);
                receive.await.unwrap();
            }),
        );
        // An unpolled task is not known to be waiting.
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 1);
        settle().await;
        assert_eq!(replay_raw_tasks(&runtime)["waitingTimers"], 1);
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
        replay_clock::advance(runtime.replay_clock.as_ref().unwrap(), 100.0)
            .await
            .unwrap();
        settle().await;
        assert!(entered.get());
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 1);
        send.send(()).unwrap();
        settle().await;
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
        assert_eq!(replay_raw_tasks(&runtime)["waitingTimers"], 0);
    });
}

#[test]
fn idle_socket_is_waiting_but_its_unfinished_handler_blocks_drain() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::task::spawn_local(async move {
            let (stream, _) = listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });
        let socket = runtime
            .dial(
                format!("ws://{address}"),
                vec![],
                WebSocketConnectionOptions::default(),
                false,
            )
            .await
            .unwrap();
        let mut peer = server.await.unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let handled = Rc::new(Cell::new(false));
        let observed = handled.clone();
        let _task = runtime.spawn_classified(
            RuntimeTaskKind::SocketReader,
            Box::pin(async move {
                socket.next_event().await.unwrap(); // Open
                socket.next_event().await.unwrap(); // actual input
                observed.set(true);
                receive.await.unwrap(); // asynchronous handler work
            }),
        );
        settle().await;
        assert_eq!(replay_raw_tasks(&runtime)["waitingSockets"], 1);
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
        use tokio::io::AsyncWriteExt;
        peer.get_mut().write_all(b"\x81\x05frame").await.unwrap();
        for _ in 0..1000 {
            if handled.get() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert!(handled.get());
        assert_eq!(replay_raw_tasks(&runtime)["waitingSockets"], 0);
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 1);
        send.send(()).unwrap();
        settle().await;
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
    });
}
