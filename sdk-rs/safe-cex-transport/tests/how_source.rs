#[test]
fn live_typescript_closed_send_and_orphan_latency_chains_remain_pinned() {
    let script = r#"
const fs = require('node:fs');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const assert = require('node:assert/strict');
const source = fs.readFileSync('packages/http-over-websocket/src/HOWWebSocket.ts','utf8');
const text = source.slice(source.indexOf('export class HOWWebSocket')).replace('export class','class');
const js = ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
let now=0,next=1;const timers=new Map();
global.setTimeout=(fn,delay)=>{const id=next++;timers.set(id,{fn,at:now+delay});return id};
global.clearTimeout=id=>timers.delete(id);
Date.now=()=>now;
const HOW = new Function('Emitter','HOWWebSocketAuthState','getUniqueLatencyDomains','extractDomainFromUrl','nanoid','WebSocket','LATENCY_CHECK_TICK','LATENCY_STALE_THRESHOLD',js+';return HOWWebSocket;')(
 class {},{Unknown:0},()=>['fapi.asterdex.com'],url=>new URL(url).hostname,()=> 'n',{OPEN:1},1000,10000);
const client=new HOW('wss://test.invalid');
client.startLatencyChecks();
client.send({type:'rpc.http',arg:{url:'https://fapi.asterdex.com/fapi/v1/PiNg'}});
assert(client.usedDomains.has('fapi.asterdex.com'));
client.send({type:'rpc.http',arg:{data:{note:'shopping'}}});
assert.throws(()=>client.send({type:'rpc.http',arg:{url:'https://venue/order'}}),{name:'WebSocketNotReadyError'});
client.ping();assert.equal(timers.size,1);client.dispose();assert.equal(timers.size,0);
const measured=new HOW('wss://test.invalid');measured.isOpen=true;
const frames=[];measured.ws={readyState:1,send:frame=>frames.push(JSON.parse(frame)),terminate(){}};
measured.startLatencyChecks();now=100;measured.startLatencyChecks();
assert.equal(timers.size,2);
measured.stopLatencyChecks();assert.equal(timers.size,1);
const advance=t=>{now=t;for(const [id,timer] of [...timers]) if(timer.at<=now){timers.delete(id);timer.fn()}};
advance(1000);assert.equal(timers.size,1);
measured.startLatencyChecks();assert.equal(timers.size,2);
measured.dispose();assert.equal(timers.size,1);
advance(2000);assert.equal(timers.size,0);
assert.equal(frames.length,0);
"#;
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn shared_json_parser_contract_includes_corruption_null_and_number_rounding() {
    use serde_json::{Value, json};
    let corrupt = r#"{"text":"x:1234567890123456,"}"#;
    assert_eq!(big_int_as_string_json_parse(corrupt), Value::Null);
    assert_eq!(json_parse(corrupt), json!({"text":"x:1234567890123456,"}));
    assert_eq!(big_int_as_string_json_parse("not json"), Value::Null);
    assert_eq!(json_parse("not json"), Value::Null);
    assert_eq!(
        big_int_as_string_json_parse(r#"{"n":9007199254740993}"#),
        json!({"n":"9007199254740993"})
    );
    assert_eq!(
        json_parse(r#"{"n":9007199254740993}"#),
        json!({"n":9007199254740992_u64})
    );
    assert_eq!(
        big_int_as_string_json_parse(r#"{"n":1e16}"#),
        json!({"n":10000000000000000_u64})
    );
}

#[test]
fn live_typescript_relay_open_throw_discards_queue_without_closing() {
    let script = r#"
const fs=require('node:fs'), ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript'), assert=require('node:assert/strict');
const source=fs.readFileSync('packages/http-over-websocket/src/RelayWebSocket.ts','utf8');
const text=source.slice(source.indexOf('export class RelayWebSocket')).replace('export class','class');
const js=ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Relay=new Function(js+';return RelayWebSocket;')();
const relay=Object.create(Relay.prototype), sent=[], events=[];
Object.assign(relay,{queue:['first','tail'],readyState:0,dispatchEvent:e=>events.push(e.type),inner:{send:data=>{sent.push(data);throw Error('send failed')}}});
assert.throws(()=>relay.becomeOpen(),/send failed/);
assert.equal(relay.readyState,1);assert.deepEqual(relay.queue,[]);
assert.deepEqual(sent,['first']);assert.deepEqual(events,['open']);
"#;
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn big_int_as_string_json_parse(text: &str) -> serde_json::Value {
    safe_cex_transport::RestResponseBody::Transformed(
        safe_cex_transport::how::big_int_as_string_json_parse(text),
    )
    .json_value()
    .unwrap()
}

fn json_parse(text: &str) -> serde_json::Value {
    safe_cex_transport::RestResponseBody::Transformed(safe_cex_transport::how::json_parse(text))
        .json_value()
        .unwrap()
}
