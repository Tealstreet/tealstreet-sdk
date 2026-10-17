"""Compile canonical polling/candle wiring faults, then restore exact bytes."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
binding = root / "safe-cex-venue-breakoutprop/src/binding.rs"
date = root / "safe-cex-num/src/date.rs"
originals = {p: p.read_bytes() for p in (binding, date)}
base = ["cargo", "test", "--locked", "--jobs", "1", "-p"]
command = base + ["safe-cex-venue-breakoutprop", "--test", "binding", "real_trait_"]
iso = base + ["safe-cex-num", "--test", "date_iso"]
names = ["real_trait_candles_reach_signed_post_and_preserve_both_rows",
         "real_trait_candles_propagate_original_http_failure",
         "real_trait_book_receives_polling_ticks_then_cleanup_removes_handler",
         "real_trait_candle_listener_receives_polling_ticks_then_cleans_up",
         "real_trait_candles_invalid_date_refuses_before_marketdata_post"]

def run(label, failures=(), args=command):
    subprocess.run(["df", "-h", "/"], check=True)
    if shutil.disk_usage("/").free < 15 * 1024**3:
        subprocess.run(["cargo", "clean", "--target-dir", str(root / "target")], cwd=root, check=True)
    result = subprocess.run(args, cwd=root, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    Path("/tmp/breakout-stream-control-" + label + ".log").write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and "test result: FAILED" in result.stdout, result.stdout
        assert all("test " + name + " ... FAILED" in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and "test result: ok" in result.stdout, result.stdout
    print(label, "named failure" if failures else "passed", flush=True)

run("baseline")
s = originals[binding].decode()
start = s.index("impl Exchange for BreakoutpropExchange")
for name in ["listen_ohlcv", "listen_order_book", "fetch_ohlcv"]:
    pos = s.index("    fn " + name + "(", start)
    end = s.index("    fn ", pos + 8)
    s = s[:pos] + s[end:]
# Caller removal intentionally orphans only these imported ABI names.
s = s.replace("use safe_cex_exchange::{", "#[allow(unused_imports)]\nuse safe_cex_exchange::{", 1)
s = s.replace("use safe_cex_types::{", "#[allow(unused_imports)]\nuse safe_cex_types::{", 1)
controls = [("trait-defaults", binding, s, names)]
s = originals[binding].decode()
needle = "        Ok(Box::new(move || cleanup()))"
assert s.count(needle) == 2
controls.append(("listener-cleanup", binding, s.replace(needle, "        Ok(Box::new(move || { let _ = cleanup; Ok(()) }))"), names[2:4]))
start = s.index('        "fetchOHLCV" => {')
end = s.index('        "fetchTickers" => {', start)
for count in (0,1):
    arm = '''        "fetchOHLCV" => {
            let opts = decoded(&arg(&args, 0));
            let pending = opts.map(|opts| reads.fetch_ohlcv(opts));
            Box::pin(async move {
                let rows = pending?.await?;
                wire(&rows.into_iter().take(COUNT).collect::<Vec<_>>())
            })
        }
'''.replace("COUNT",str(count))
    controls.append(("candles-take-"+str(count), binding, s[:start]+arm+s[end:], names[:1]))
s = originals[date].decode()
needle = "    let milliseconds = js_time_clip(value);"
assert s.count(needle) == 1
controls.append(("date-time-clip", date, s.replace(needle,"    let milliseconds = value;"), names[4:]))
for label, path, mutation, failures in controls:
    try:
        path.write_text(mutation)
        run(label, failures)
        if path == date:
            run(label+"-source", ["date_iso_matches_native_js_time_clip_and_extended_years"], iso)
    finally:
        for original, content in originals.items():
            original.write_bytes(content)
    run(label+"-restored")
    if path == date:
        run(label+"-restored-source", args=iso)
assert all(p.read_bytes()==b for p,b in originals.items())
