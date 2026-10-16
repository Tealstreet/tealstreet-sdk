"""Run serial native and CI gates at one committed tree; structural reports are separate."""
from pathlib import Path
import json
import os
import subprocess


ROOT = Path(__file__).resolve().parents[3]
LOGS = ROOT / '.aimux/bybit-api/gates'
CARGO = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--jobs', '1']
ENV = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1',
           SAFE_CEX_TYPESCRIPT_PATH=str(ROOT / 'node_modules/typescript'))
GATE_COUNT = 24


def clean(head):
    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
    for args in [['git', 'diff', '--exit-code'], ['git', 'diff', '--cached', '--exit-code']]:
        subprocess.run(args, cwd=ROOT, check=True, capture_output=True)


def main():
    yarn = Path(os.environ['YARN_CLI']).resolve(strict=True)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    clean(head)
    LOGS.mkdir(parents=True, exist_ok=True)
    commands = [
        ('cancellation-classification', CARGO + ['-p', 'safe-cex-exchange', '--all-features', '--test', 'cancellable_future_inventory', 'production_cancellable_future_classifications_are_current', '--', '--exact']),
        ('shared-result-lifetimes', CARGO + ['-p', 'safe-cex-exchange', '--all-features', '--lib', 'shared_result']),
        ('http-receipt-codec', CARGO + ['-p', 'safe-cex-exchange', '--all-features', '--lib', 'http::receipt_tests::']),
        ('http-receipt-transform-hooks', CARGO + ['-p', 'safe-cex-exchange', '--all-features', '--test', 'core_health_http_token', 'http_response_']),
        ('transport-response-transform', CARGO + ['-p', 'safe-cex-transport', '--test', 'response_transform']),
        ('transport-receipts', CARGO + ['-p', 'safe-cex-transport', '--test', 'p2c', 'receipt']),
        ('transport-effective-config', CARGO + ['-p', 'safe-cex-transport', '--test', 'p2c', 'effective_config']),
        ('transport-signature-retry', CARGO + ['-p', 'safe-cex-transport', '--test', 'p2c', 'signature_']),
        ('blofin-effective-config', CARGO + ['-p', 'safe-cex-venue-blofin', '--test', 'receipt_config']),
        ('hyperliquid-effective-config', CARGO + ['-p', 'safe-cex-venue-hyperliquid', '--test', 'receipt_config']),
        ('factory-source-and-native', CARGO + ['-p', 'safe-cex-venue-bybit', '--test', 'api_factory', '--', '--include-ignored']),
        ('regional-provider-forwarding', CARGO + ['-p', 'safe-cex-transport', '--test', 'tealstreet_transport', 'native_provider_selection_matches_actual_source', '--', '--exact']),
        ('regional-provider-raw-values', CARGO + ['-p', 'safe-cex-transport', '--test', 'tealstreet_transport', 'native_authorization_empty_tokens_headers_identity_and_adapter_dispatch', '--', '--exact']),
        ('workspace-all-targets-check', ['cargo', 'check', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--workspace', '--all-targets', '--keep-going', '--jobs', '1']),
        ('host-workspace-clippy', ['cargo', 'clippy', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--workspace', '--all-targets', '--keep-going', '--jobs', '1', '--', '-D', 'warnings']),
        ('host-exchange-all-features-clippy', ['cargo', 'clippy', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-exchange', '--all-features', '--all-targets', '--jobs', '1', '--', '-D', 'warnings']),
        ('wasm-workspace-clippy', ['node', '.github/scripts/rust-wasm.mjs', '--clippy']),
        ('wasm-workspace-build', ['node', '.github/scripts/rust-wasm.mjs', '--build']),
        ('wasm-exchange-default-build', ['cargo', 'build', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-exchange', '--target', 'wasm32-unknown-unknown', '--jobs', '1']),
        ('wasm-exchange-all-features-build', ['cargo', 'build', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-exchange', '--all-features', '--target', 'wasm32-unknown-unknown', '--jobs', '1']),
        ('host-workspace-build', ['cargo', 'build', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--workspace', '--jobs', '1']),
        ('source-extractor-check', ['node', str(yarn), 'parity:extract:check']),
        ('workspace-rustfmt', ['cargo', 'fmt', '--manifest-path', 'sdk-rs/Cargo.toml', '--all', '--', '--check']),
        ('diff-check', ['git', 'diff', '--check']),
    ]
    assert len(commands) == GATE_COUNT
    expected_tests = dict(zip([name for name, _ in commands[:13]],
                              [1, 8, 3, 2, 3, 2, 2, 5, 2, 1, 15, 1, 1], strict=True))
    report = dict(measured_sha=head, accepted=False, declared_gates=GATE_COUNT, gates=[])
    receipt = LOGS / 'report.json'
    receipt.write_text(json.dumps(report, indent=2) + '\n')
    for name, command in commands:
        clean(head)
        print('START', name, head, flush=True)
        log = LOGS / (name + '.log')
        with log.open('w') as output:
            result = subprocess.run(command, cwd=ROOT, env=ENV, stdout=output, stderr=subprocess.STDOUT)
        count = expected_tests.get(name)
        witnessed = count is None or f'test result: ok. {count} passed; 0 failed; 0 ignored;' in log.read_text()
        report['gates'].append(dict(name=name, exit=result.returncode, command=command,
                                   expected_tests=count, test_floor_observed=witnessed))
        receipt.write_text(json.dumps(report, indent=2) + '\n')
        print('EXIT', name, result.returncode, flush=True)
        if result.returncode or not witnessed:
            raise SystemExit(1)
        clean(head)
    assert len(report['gates']) == GATE_COUNT
    report['accepted'] = True
    receipt.write_text(json.dumps(report, indent=2) + '\n')
    print('COMPLETE', head, flush=True)


if __name__ == '__main__':
    main()
