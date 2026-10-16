"""Reject compiling missing/no-op native Phemex tunnel providers; restore bytes."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'safe-cex-registry/src/native_host.rs'
TEST = 'phemex_rest_runs_over_the_tealstreet_tunnel_with_signed_account_query'


def run():
    r = subprocess.run(['cargo', 'test', '--locked', '--jobs', '1', '-q', '-p',
        'safe-cex-registry', '--test', 'native_host_tunnel', TEST, '--', '--exact'],
        cwd=ROOT, capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def passing():
    code, output = run()
    assert code == 0 and 'test result: ok. 1 passed' in output, output


passing()
original = SOURCE.read_bytes()
start = original.index(b'    fn phemex(')
end = original.index(b'\n    fn ', start + 20)
part = original[start:end]
field = part.index(b'                install_transport: Some(')
following = part.index(b'                external_public_data:', field)
assert part[field:following].endswith(b'                })),\n')
for label, replacement in [
    ('absent provider', b'                install_transport: None,\n'),
    ('no-op provider', b'''                install_transport: Some(Rc::new(move |client, options, name| {
                    let _ = (&installer_http, &provider, client, options, name);
                    Ok(())
                })),
'''),
]:
    try:
        changed = part[:field] + replacement + part[following:]
        if label == 'absent provider':
            needle = b'        let provider = self.runtime.transport();\n'
            assert changed.count(needle) == 1
            changed = changed.replace(needle, needle + b'        let _ = (&installer_http, &provider);\n')
        SOURCE.write_bytes(original[:start] + changed + original[end:])
        code, output = run()
        assert code == 101 and 'test result: FAILED' in output and f'---- {TEST} stdout ----' in output, output
    finally:
        SOURCE.write_bytes(original)
    passing()
    print(f'Phemex tunnel {label}: compiling mutation fails101; exact restoration passes0', flush=True)
