use safe_cex_exchange::throttle::ThrottleManager;
use std::{cell::RefCell, rc::Rc};

#[path = "support/runtime.rs"]
mod runtime;
use runtime::Runtime;
#[test]
fn throttle_keys_latest_tail_and_clear_follow_source_timing() {
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let recorded = calls.clone();
    let clock = runtime.now.clone();
    let manager = ThrottleManager::new(
        runtime.clone(),
        move |value: (&'static str, u32)| recorded.borrow_mut().push((clock.get(), value)),
        250,
        |value| value.0.into(),
    );
    manager.call(("a", 1));
    runtime.advance(100);
    manager.call(("a", 2));
    manager.call(("b", 3));
    runtime.advance(200);
    manager.call(("a", 4));
    runtime.advance(349);
    assert_eq!(calls.borrow().len(), 2);
    runtime.advance(450);
    assert_eq!(
        *calls.borrow(),
        vec![(0, ("a", 1)), (100, ("b", 3)), (450, ("a", 4))]
    );
    manager.call(("a", 5));
    manager.call(("a", 6));
    manager.clear();
    runtime.advance(800);
    assert_eq!(calls.borrow().len(), 4);
    manager.call(("a", 7));
    manager.call(("a", 8));
    let weak = Rc::downgrade(&manager);
    drop(manager);
    assert!(weak.upgrade().is_none());
    runtime.advance(1200);
    assert_eq!(calls.borrow().len(), 5);
}
#[test]
fn source_older_tail_survives_new_leading_and_clear() {
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = calls.clone();
    let manager = ThrottleManager::new(
        runtime.clone(),
        move |x| log.borrow_mut().push(x),
        250,
        |_| "key".into(),
    );
    manager.call(1);
    runtime.advance(200);
    manager.call(2);
    runtime.advance(250);
    manager.call(3);
    manager.clear();
    runtime.advance(450);
    assert_eq!(*calls.borrow(), vec![1, 3, 2]);
}

#[test]
fn throttle_orphan_tail_matches_live_typescript() {
    let output=std::process::Command::new("node").args(["--input-type=module","-e",r#"
import {loadStandalone} from './sdk-rs/safe-cex-webworker/tests/source_loader.mjs';
const {ThrottleManager} = await loadStandalone('./packages/safe-cex/src/webworker/handlers/ThrottleManager.ts');
let now=0,id=0;const timers=new Map();const calls=[];
Date.now=()=>now;
globalThis.setTimeout=(fn,delay)=>{timers.set(++id,{fn,at:now+delay});return id};
globalThis.clearTimeout=id=>timers.delete(id);
const advance=t=>{now=t;for(const [id,entry] of [...timers])if(entry.at<=now){timers.delete(id);entry.fn()}};
const manager=new ThrottleManager(x=>calls.push(x),250,()=> 'key');
manager.call(1);advance(200);manager.call(2);advance(250);manager.call(3);manager.clear();advance(450);
process.stdout.write(JSON.stringify(calls));
"#]).current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../..")).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "[1,3,2]");
}

#[test]
fn discarded_timer_tasks_release_the_key_without_firing_a_tail() {
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = calls.clone();
    let manager = ThrottleManager::new(
        runtime.clone(),
        move |x| log.borrow_mut().push(x),
        250,
        |_| "key".into(),
    );
    manager.call(1);
    manager.call(2);
    let tasks = std::mem::take(&mut *runtime.tasks.borrow_mut());
    drop(tasks);
    manager.call(3);
    assert_eq!(*calls.borrow(), vec![1, 3]);
}
