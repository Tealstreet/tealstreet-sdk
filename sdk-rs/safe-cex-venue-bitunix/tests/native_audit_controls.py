#!/usr/bin/env python3
"""Compiled controls for concrete signing, ID and malformed batch-row regressions.

Run from any cwd, serially. Each mutation must compile and fail its named native
assertion; exact production bytes are restored even if a control fails.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

SDK = Path(__file__).resolve().parents[2]
ROOT = SDK.parent
OUT = SDK / "target" / "venue-audit-controls"
OUT.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ, CARGO_BUILD_JOBS="1")
BASE = ["cargo", "test", "--locked", "--jobs", "1"]
CONTROLS = []

def control(name, file, old, new, crate, target, assertion, test=None):
    CONTROLS.append((name, file, old, new, crate, target, assertion, test))

clock = "safe_cex_store::virtual_clock::VIRTUAL_CLOCK"
control("blofin-rest-clock", "safe-cex-venue-blofin/src/native_host.rs",
        clock+".get_current_time()", "self.runtime.now_ms()",
        "blofin", "native_clock", "synced timestamp")
control("blofin-ws-clock", "safe-cex-venue-blofin/src/native_host.rs",
        clock+".get_current_time_ms()", "self.runtime.now_ms()",
        "blofin", "native_clock", "synced timestamp")
control("bitunix-rest-clock", "safe-cex-venue-bitunix/src/native_http.rs",
        clock+".get_current_time()", "client.base.runtime.now_ms()",
        "bitunix", "native_clock", "synced timestamp")
control("bitunix-ws-clock", "safe-cex-venue-bitunix/src/sockets.rs",
        clock+".get_current_time()", "runtime.now_ms()",
        "bitunix", "native_clock", "synced timestamp")
control("kraken-clock", "safe-cex-venue-kraken/src/binding/host.rs",
        clock+".get_current_time_ms()", "self.now()",
        "kraken", "native_clock", "synced Kraken nonce")
control("shared-js-clock", "safe-cex-exchange/src/js_runtime/globals.rs",
        'Js::function("getCurrentTimeMs", move |_, _| { Ok(Js::Number('+clock+'.get_current_time_ms(),)) })',
        'Js::function("getCurrentTimeMs", { let clock_rt=rt.clone(); move |_, _| { Ok(Js::Number(scope_of(&clock_rt)?.now())) } })',
        "exchange", "js_runtime", "JS virtualClock must retain server offset",
        "virtual_clock_global_preserves_synced_time_without_changing_date_now")
control("bitunix-login-sync", "safe-cex-venue-bitunix/src/sockets.rs",
        "|| async { "+clock+".sync_bybit_client_server_time(None).await; Ok(()) }",
        "|| async { Ok(()) }", "bitunix", "native_clock", "sendLogin must await real native sync")
control("bitunix-retry-sync", "safe-cex-venue-bitunix/src/sockets.rs",
        "async fn sync(&mut self) -> Result<(), ExchangeError> { "+clock+".sync_bybit_client_server_time(None).await; Ok(()) }",
        "async fn sync(&mut self) -> Result<(), ExchangeError> { Ok(()) }", "bitunix", "native_clock", "login failure must resync before its retry delay")
for name,file,expr,crate,target,assertion in [
    ("bitunix-rest-nonce", "native_http.rs", "nonce_runtime.unique_id()", "bitunix", "native_identity", "native REST nonce must strip"),
    ("bitunix-ws-nonce", "sockets.rs", "nonce.unique_id()", "bitunix", "native_clock", "WS nonce uses source utils/uuid"),
    ("kraken-client-id", "binding/host.rs", "self.shared.runtime.unique_id()", "kraken", "native_identity", "source utils/uuid client ID"),
]:
    control(name, f"safe-cex-venue-{crate}/src/{file}", expr+".replace(['-', '_'], \"\")", expr, crate,target,assertion)

# These two identical expressions occur in separate concrete host methods.
# Qualified anchors keep the controls independent after rustfmt.
control("bitunix-client-id", "safe-cex-venue-bitunix/src/adapter_orders.rs",
        "Ok(self.venue.runtime.unique_id().replace(['-', '_'], \"\"))",
        "Ok(self.venue.runtime.unique_id())", "bitunix", "native_identity",
        "native generated client ID must use utils/uuid")
control("bitunix-fallback-id", "safe-cex-venue-bitunix/src/adapter_orders.rs",
        "self.venue.runtime.unique_id().replace(['-', '_'], \"\")",
        "self.venue.runtime.unique_id()", "bitunix", "native_identity",
        "batch missing-ID fallback uses utils/uuid")
# Direct null row reads are independent of optional envelope reads.
for key in ["code", "orderId", "errorMsg"]:
    control("null-row-"+key,"safe-cex-venue-bitunix/src/order_placement.rs",
            "if host.nullish(row) {", "if host.nullish(row) && key != \""+key+"\" {",
            "bitunix", "native_identity", "malformed batch must retain only source IDs" if key != "errorMsg" else "exact caught TypeError/chunk behavior")

control("bitunix-market-refusal", "safe-cex-venue-bitunix/src/adapter_rest.rs",
        '|e: &Fault| me.runtime.console("error", &e.0.js_error_string()),',
        '|e: &Fault| me.trace(&e.0),', "bitunix", "native_market_refusal",
        "typed market refusal must be visible at error level with its original cause")

if __name__ == "__main__":
    selected = set(sys.argv[1:])
    unknown = selected - {row[0] for row in CONTROLS}
    if unknown:
        raise AssertionError(f"Unknown controls: {sorted(unknown)}")
    receipt = OUT/"results.json"
    prior = json.loads(receipt.read_text()) if receipt.exists() else []
    results=[row for row in prior if selected and row["control"] not in selected]
    for name,file,old,new,crate,target,assertion,test in CONTROLS:
        if selected and name not in selected:
            continue
        path = SDK / file
        original = path.read_bytes()
        text = original.decode()
        # Match the exact non-whitespace tokens even after rustfmt wraps a call.
        compact = "".join(c for c in text if not c.isspace())
        offsets = [i for i,c in enumerate(text) if not c.isspace()]
        anchor = "".join(c for c in old if not c.isspace())
        import re
        matches = [(offsets[m.start()], offsets[m.end()-1]+1) for m in re.finditer(re.escape(anchor),compact)]
        if name == "bitunix-fallback-id":
            matches = [(a,b) for a,b in matches if a > text.index("struct BatchFrame")]
        if len(matches) != 1:
            raise AssertionError(f"{name}: expected one mutation anchor, got {len(matches)}")
        begin,end=matches[0]
        args = BASE + ["-p", "safe-cex-"+("exchange" if crate=="exchange" else "venue-"+crate),"--test",target]
        if test: args += [test]
        try:
            path.write_text(text[:begin]+new+text[end:])
            log=OUT/(name+"-red.log")
            with log.open("w") as stream:
                result=subprocess.run(args,cwd=SDK,env=ENV,stdout=stream,stderr=subprocess.STDOUT)
            output=log.read_text()
            if result.returncode!=101 or "test result: FAILED" not in output or assertion not in output or "could not compile" in output:
                raise AssertionError(f"{name}: expected compiled named failure; inspect {log}")
            row = {"control":name,"exit":result.returncode,"assertion":assertion,
                   "sourceSha256":hashlib.sha256(original).hexdigest(),"command":args}
        finally:
            path.write_bytes(original)
        log=OUT/(name+"-restored.log")
        with log.open("w") as stream:
            restored=subprocess.run(args,cwd=SDK,env=ENV,stdout=stream,stderr=subprocess.STDOUT)
        if restored.returncode != 0 or "test result: ok" not in log.read_text():
            raise AssertionError(f"{name}: restored native test failed; inspect {log}")
        row["restoredExit"] = restored.returncode
        results.append(row)
        receipt.write_text(json.dumps(results,indent=2)+"\n")
        print(json.dumps(row),flush=True)
