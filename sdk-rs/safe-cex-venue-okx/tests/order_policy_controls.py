from pathlib import Path
import subprocess

tests = Path(__file__).resolve().parent
source = tests.parent / "src/exchange.rs"
original = source.read_bytes()
controls = [
    ("predicate", [("OrderType::Market | OrderType::Limit => false", "OrderType::Market | OrderType::Limit | OrderType::StopLoss => false"),
                   ("        | OrderType::StopLoss\n", "")], "ORDER-POLICY native algo identity 3"),
    ("priority", [("OrderType::StopLoss => 0", "OrderType::StopLoss => 2")], "ORDER-POLICY native priority identity 3"),
]
for name, replacements, assertion in controls:
    try:
        text = original.decode()
        for before, after in replacements:
            assert text.count(before) == 1, f"order policy control drift: {name}"
            text = text.replace(before, after)
        source.write_text(text)
        result = subprocess.run(
            ["cargo", "test", "--locked", "--manifest-path", str(tests.parents[1] / "Cargo.toml"),
             "-p", "safe-cex-venue-okx", "--test", "order_policy", "--jobs", "1", "--", "--include-ignored"],
            capture_output=True, text=True,
        )
        output = result.stdout + result.stderr
        assert "Finished `test`" in output, output
        assert result.returncode == 101 and assertion in output, output
        print(f"{name}: compiled, exit 101 at {assertion}")
    finally:
        source.write_bytes(original)
