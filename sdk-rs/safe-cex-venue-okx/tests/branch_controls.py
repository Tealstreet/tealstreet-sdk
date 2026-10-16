from pathlib import Path
import subprocess

tests = Path(__file__).resolve().parent
controls = [
    ("position-oracle.mjs", "x.row.type==='constructor'", "false", "POSITION-FILTER constructor executions"),
    ("position-oracle.mjs", "x.row.type==='__proto__'", "false", "POSITION-FILTER prototype executions"),
    ("ticker-events-oracle.mjs", "if(method===names[2])", "if(false)", "TICKER-BRANCH assertion executions"),
    ("ticker-events-oracle.mjs", "if(method===names[3]&&!error)", "if(false)", "TICKER-BRANCH assertion executions"),
    ("ticker-events-oracle.mjs", "if(method===names[3]&&error)", "if(false)", "TICKER-BRANCH assertion executions"),
    ("rest-operations-oracle.mjs", "if(['constructor','__proto__','toString','valueOf','hasOwnProperty'].includes(state))", "if(false)", "REST-BRANCH inherited status executions"),
]
for filename, before, after, assertion in controls:
    path = tests / filename
    original = path.read_bytes()
    try:
        text = original.decode()
        assert text.count(before) == 1, f"control drift: {filename}/{before}"
        path.write_text(text.replace(before, after))
        result = subprocess.run(["node", str(path)], capture_output=True, text=True)
        assert result.returncode == 1 and assertion in result.stderr, result.stderr
        print(f"{filename}: exit 1 at {assertion}")
    finally:
        path.write_bytes(original)

path = tests / "public_socket.rs"
original = path.read_bytes()
try:
    text = original.decode()
    before = '.any(|frame| frame == "pong")'
    assert text.count(before) == 1, "pong branch control drift"
    path.write_text(text.replace(before, '.any(|_frame| false)'))
    result = subprocess.run(
        ["cargo", "test", "--locked", "--manifest-path", str(tests.parents[1] / "Cargo.toml"),
         "-p", "safe-cex-venue-okx", "--test", "public_socket", "--jobs", "1", "--", "--include-ignored"],
        capture_output=True, text=True,
    )
    output = result.stdout + result.stderr
    assert "Finished `test`" in output, output
    assert result.returncode == 101 and "PONG-BRANCH assertion executions" in output, output
    print("public_socket: compiled, exit 101 at PONG-BRANCH assertion executions")
finally:
    path.write_bytes(original)
