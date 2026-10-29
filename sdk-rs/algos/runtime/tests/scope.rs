use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algos::Scope;
use tokio::task::LocalSet;

#[tokio::test(start_paused = true)]
async fn dispose_aborts_tasks_in_the_scope_and_its_children() {
    LocalSet::new()
        .run_until(async {
            let ran = Rc::new(Cell::new(0));
            let parent = Scope::new();
            let child = parent.child();
            for scope in [&parent, &child] {
                let ran = ran.clone();
                scope.spawn(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    ran.set(ran.get() + 1);
                });
            }
            parent.dispose();
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert_eq!(ran.get(), 0);
            assert!(child.is_disposed());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_disposed_scope_drops_later_spawns_and_children() {
    LocalSet::new()
        .run_until(async {
            let ran = Rc::new(Cell::new(false));
            let scope = Scope::new();
            scope.dispose();
            let flag = ran.clone();
            scope.spawn(async move { flag.set(true) });
            assert!(scope.child().is_disposed());
            tokio::task::yield_now().await;
            assert!(!ran.get());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn disposing_a_child_leaves_its_parent_running() {
    LocalSet::new()
        .run_until(async {
            let ran = Rc::new(Cell::new(false));
            let parent = Scope::new();
            parent.child().dispose();
            let flag = ran.clone();
            parent.spawn(async move { flag.set(true) });
            tokio::task::yield_now().await;
            assert!(ran.get());
        })
        .await;
}
