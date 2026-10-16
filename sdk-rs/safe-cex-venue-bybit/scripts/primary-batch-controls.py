#!/usr/bin/env python3
"""Compile each consumer fault, require a named runtime failure, restore exact bytes."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
batch = root / "sdk-rs/safe-cex-venue-bybit/src/binding/members/batch.rs"
receiver = root / "sdk-rs/safe-cex-venue-bybit/src/binding/receiver.rs"
original = {p: p.read_text() for p in (batch, receiver)}
command = ["cargo", "test", "--locked", "--manifest-path", "sdk-rs/Cargo.toml",
           "-p", "safe-cex-venue-bybit", "--test", "primary_batch_binding"]
controls = [
    ("receiver", receiver, "    members::batch::install(rt)?;", "    if false { members::batch::install(rt)?; }", "primary_batches_match_twenty_four_actual_source_compositions"),
    ("atomic", batch, "for order in items(&orders)? {", "for order in items(&Js::array(vec![]))? {", "invalid_execution_and_caller_ids_refuse_the_whole_batch_before_writes"),
    ("delay", batch, "Js::Number(1200.)", "Js::Number(0.)", "batch delay observed"),
    ("results", batch, "if result.truthy() {", "if result.truthy() && result.is_undefined() {", "primary_batches_match_twenty_four_actual_source_compositions"),
    ("cache", batch, "valid.get(index)", "valid.get(index.saturating_add(valid.len()))", "batch result reaches recently placed order cache"),
    ("formatter", batch, "Ok(request) => Ok(from_json(&request)),", "Ok(request) => { let _ = from_json(&request); Ok(Js::Null) },", "primary_batches_match_twenty_four_actual_source_compositions"),
    ("special", batch, "|| crate::order_params::has_bybit_single_order_params(&opts)", "|| (false && crate::order_params::has_bybit_single_order_params(&opts))", "special_orders_run_before_batches_and_a_failed_special_does_not_stop_them"),
    ("brackets", batch, "Ok(request) => Ok(from_json(&request)),", 'Ok(request) => { let request = from_json(&request); request.delete(&"stopLoss".encode_utf16().collect::<Vec<_>>()); Ok(request) },', "primary_batches_match_twenty_four_actual_source_compositions"),
]

def run(label):
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/bybit40-control-" + label + ".txt").write_text(result.stdout)
    return result

baseline = run("baseline")
assert baseline.returncode == 0 and "5 passed" in baseline.stdout, baseline.stdout
try:
    for name, target, before, after, witness in controls:
        assert before in original[target], name
        target.write_text(original[target].replace(before, after, 1))
        result = run(name)
        assert result.returncode == 101 and "panicked at" in result.stdout and witness in result.stdout and "error[E" not in result.stdout, (name, result.stdout)
        print(name + ": compiled fault caught", flush=True)
        target.write_text(original[target])
finally:
    for target, content in original.items():
        target.write_text(content)
restored = run("restored")
assert restored.returncode == 0 and "5 passed" in restored.stdout, restored.stdout
assert all(target.read_text() == content for target, content in original.items())
print("8 compiled batch consumer faults caught; exact restoration passes 5 tests.", flush=True)
