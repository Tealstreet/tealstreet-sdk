"""Run exclusively: compiled DATA projection/omission faults with exact restores."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
method = root / "safe-cex-webworker/src/call_method.rs"
subscriptions = root / "safe-cex-webworker/src/subscriptions.rs"
originals = {p: p.read_bytes() for p in (method, subscriptions)}
command = ["cargo", "test", "--locked", "-p", "safe-cex-webworker", "--test", "data_posts", "--jobs", "1"]

def run(label, failures=()):
    subprocess.run(["df", "-h", "/"], check=True)
    if shutil.disk_usage("/").free < 15 * 1024**3:
        subprocess.run(["cargo", "clean", "--target-dir", str(root / "target")], cwd=root, check=True)
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/worker-data-control-" + label + ".log").write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and "test result: FAILED" in result.stdout, result.stdout
        assert all("test " + name + " ... FAILED" in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and "test result: ok" in result.stdout, result.stdout
    print(label, "named failure" if failures else "passed", flush=True)

s = originals[method].decode()
needle = "    from_typed(&result)"
assert s.count(needle) == 1
controls = [("result-json", method, s.replace(needle, "    from_typed(&serde_json::to_value(&result).map_err(|e| ExchangeError::new(e.to_string()))?)"),
             ["call_method_candles_keep_native_numbers_and_both_rows", "call_method_orders_keep_own_undefined_and_native_numbers"])]
needle = '"fetchOHLCV" => encode(venue.fetch_ohlcv(arg(args, 0)?).await?),'
assert s.count(needle) == 1
controls.append(("candle-subset", method, s.replace(needle, '"fetchOHLCV" => encode(venue.fetch_ohlcv(arg(args, 0)?).await?.into_iter().take(1).collect::<Vec<_>>()),'),
                 ["call_method_candles_keep_native_numbers_and_both_rows"]))
needle = "result.unwrap_or(Js::Undefined)"
assert s.count(needle) == 1
controls.append(("void-null", method, s.replace(needle, "result.unwrap_or(Js::Null)"),
                 ["assigned_void_is_own_undefined_while_unknown_method_omits_result"]))
a = s.index("    fn send(&mut self,")
b = s.index("\n    }\n}\nimpl Drop", a)
body = s[a:b]
assert body.count("        })") == 1
controls.append(("ignored-post-error", method, s[:a] + body.replace("        })", "        }).or(Ok(()))") + s[b:],
                 ["data_post_rejection_reaches_the_original_reply_catch"]))
s = originals[subscriptions].decode()
needle = "let candle = from_typed(&candle)"
assert s.count(needle) == 1
controls.append(("candle-json", subscriptions, s.replace(needle, "let candle = from_typed(&json!(candle))"),
                 ["candle_subscription_keeps_native_numbers_at_the_real_post_sink"]))
run("baseline")
for label, path, mutation, failures in controls:
    try:
        path.write_text(mutation)
        run(label, failures)
    finally:
        for original_path, original in originals.items():
            original_path.write_bytes(original)
    run(label + "-restored")
