# Run from repo root, alone: each independent mutation is restored before the next.
from pathlib import Path
import os, subprocess, json
root = Path.cwd()
base = root / 'sdk-rs/safe-cex-transport'
env = dict(os.environ)
env['PATH'] = '/Users/sam/.volta/tools/image/yarn/1.22.21/bin:/opt/homebrew/opt/rustup/bin:' + env['PATH']
cases = []
def add(name, path, old, new, test):
    cases.append((name, path, old, new, ['cargo','test','-p','safe-cex-transport',test,'--','--include-ignored']))
how = base/'src/how.rs'
routing = base/'src/routing.rs'
add('simulation-capability', how,
    'if !crate::how_config::should_simulate_http_over_websocket() {', 'if true {',
    'simulation_accepts_direct_only_hosts_and_real_dials_still_fail_closed')
add('refresh-error-sharing', how,
    '*attempt.outcome.borrow_mut() = Some(Err(error.clone()));',
    '*attempt.outcome.borrow_mut() = Some({ let _ = error; Ok(()) });',
    'bad_auth_joiners_share_one_failure_and_later_calls_can_retry')
add('raw-corruption-outbound', how,
    'let protected = protect_big_integer_tokens(text);',
    'let protected = { let _ = protect_big_integer_tokens(text); text.to_owned() };',
    'raw_parser_corruption_sends_null_in_the_outbound_frame')
add('ts-correction-detected', root/'packages/json-parse/src/jsonParse.ts',
    'return jsonParse<T>(safeStr);', 'return jsonParse<T>(str);',
    'raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace')
add('js-whitespace-feff', how, 'safe_cex_num::js_trim_start(tail).len()',
    r"tail.trim_start_matches(|ch: char| ch != '\u{feff}' && safe_cex_num::js_trim(&ch.to_string()).is_empty()).len()",
    'raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace')
add('js-whitespace-nel', how, 'safe_cex_num::js_trim_start(tail).len()',
    r"tail.trim_start_matches(|ch: char| ch == '\u{0085}' || safe_cex_num::js_trim(&ch.to_string()).is_empty()).len()",
    'raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace')
add('staging-selection', routing, 'staging.as_deref() == Some("1")', 'staging.as_deref() == Some("never")',
    'staging_switch_matches_typescript_for_how_and_relay_dials')
add('staging-routing', routing, 'let endpoint = tunnel_endpoint(egress);',
    'let endpoint = match egress { HowEgress::Jp => generated::TUNNEL_JP, HowEgress::Sg => generated::TUNNEL_SG };',
    'staging_switch_matches_typescript_for_how_and_relay_dials')
cases.append(('staging-wasm', routing,
    'option_env!("NEXT_PUBLIC_TRADETUNNEL_STAGING").map(str::to_owned)',
    'None::<String>',
    ['env','NEXT_PUBLIC_TRADETUNNEL_STAGING=1','yarn','--cwd',str(base),'warning:wasm']))
results=[]
for name,path,old,new,cmd in cases:
    original=path.read_text()
    assert original.count(old)==1,(name,original.count(old))
    path.write_text(original.replace(old,new))
    try:
        result=subprocess.run(cmd,cwd=root/'sdk-rs',env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        Path('/tmp/p2b-ruling-control-'+name+'.log').write_text(result.stdout)
        if name=='staging-wasm':
            ok=result.returncode!=0 and 'AssertionError' in result.stdout
        else:
            ok=result.returncode==101 and 'test result: FAILED' in result.stdout
        print(name,result.returncode,'assertion-red' if ok else 'BAD',flush=True)
        results.append(dict(name=name,exit=result.returncode,assertion_failure=ok))
        if not ok: print(result.stdout[-1800:],flush=True)
    finally:
        path.write_text(original)
Path('/tmp/p2b-ruling-controls-results.json').write_text(json.dumps(results,indent=2))
assert all(row['assertion_failure'] for row in results)
