#!/usr/bin/env python3
"""Compile retained indexing/risk controls; superseded venue controls use parity/orphan_findings_controls.py."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]


def replace_once(text, before, after):
    if text.count(before) != 1:
        raise RuntimeError('mutation target must occur exactly once')
    return text.replace(before, after, 1)




def mutate(text, name):
    if name == 'lighter-declarations':
        start = text.index('impl TransactionInfo for L2CreateOrderTxInfo {')
        end = text.index('impl TransactionInfo for TxInfo {', start)
        original_macro = '''macro_rules! transaction_info {
    ($ty:ty,$kind:ident) => {
        impl TransactionInfo for $ty {
            fn kind(&self) -> TransactionKind { TransactionKind::$kind }
            fn header(&self) -> &TransactionHeader { &self.header }
        }
    };
}
transaction_info!(L2CreateOrderTxInfo, CreateOrder);
transaction_info!(L2CreateGroupedOrdersTxInfo, CreateGroupedOrders);
transaction_info!(L2CancelOrderTxInfo, CancelOrder);
transaction_info!(L2TransferTxInfo, Transfer);
transaction_info!(L2UpdateLeverageTxInfo, UpdateLeverage);
transaction_info!(L2ApproveIntegratorTxInfo, ApproveIntegrator);
transaction_info!(L2WithdrawalTxInfo, Withdrawal);
'''
        return text[:start] + original_macro + text[end:]
    if name == 'owner-conditional':
        return replace_once(text, '&& import.unconditional', '&& (import.unconditional || !import.unconditional)')
    if name == 'owner-namespace':
        text = replace_once(text, '                        reexports.namespace.push(i.ident.to_string());\n', '')
        return replace_once(text, '                        reexports.namespace.pop();\n', '')
    if name == 'twap-scope':
        before = '''// ts: twapManager. One shared manager in each host thread; hosts may also own isolated managers.
pub fn twap_manager() -> Rc<TwapManager> {
    thread_local! {
        static TWAP_MANAGER: Rc<TwapManager> = Rc::new(TwapManager::default());
    }'''
        after = '''thread_local! {
    static TWAP_MANAGER: Rc<TwapManager> = Rc::new(TwapManager::default());
}
// ts: twapManager. One shared manager in each host thread; hosts may also own isolated managers.
pub fn twap_manager() -> Rc<TwapManager> {'''
        return replace_once(text, before, after)
    if name == 'indexer-scope':
        before = '''// ts: chaserOrderManagerInstance. Shared per native host thread.
pub fn chaser_order_manager_instance() -> Rc<ChaserOrderManager> {
    // Function-local static storage remains shared across calls on each thread.
    thread_local! {
        static CHASER_ORDER_MANAGER_INSTANCE: Rc<ChaserOrderManager> = ChaserOrderManager::new();
    }'''
        after = '''thread_local! {
    static CHASER_ORDER_MANAGER_INSTANCE: Rc<ChaserOrderManager> = ChaserOrderManager::new();
}
// ts: chaserOrderManagerInstance. Shared per native host thread.
pub fn chaser_order_manager_instance() -> Rc<ChaserOrderManager> {'''
        return replace_once(text, before, after)
    if name == 'indexer-assertion':
        return replace_once(text, 'let target = "sample/src/lib.rs::chaser_order_manager_instance";', 'let target = "sample/src/lib.rs::missing_accessor_mutation";')
    if name == 'singleton':
        return replace_once(text, 'CHASER_ORDER_MANAGER_INSTANCE.with(Rc::clone)', 'CHASER_ORDER_MANAGER_INSTANCE.with(|_| ChaserOrderManager::new())')
    if name == 'risk-order':
        return replace_once(text, 'row.position_size_before_fee = true;', 'row.position_size_before_fee = false;')
    if name == 'risk':
        return replace_once(text, '''        require_array_bucket(
            &order.symbol,
            "openOrdersBySymbol[o.symbol].push is not a function",
        )?;''', '')
    raise RuntimeError('unknown mutation')


CASES = {
    'lighter-declarations': ('safe-cex-venue-lighter', 'src/http_transactions.rs', ['--test', 'gates', 'production_lighter_transaction_declarations_are_indexable'], 'LIGHTER transaction source index'),
    'owner-conditional': ('safe-cex-parity', 'src/reexports.rs', ['--test', 'gates', 'uncertain_imported_impl_owners_still_fail_closed'], 'OWNER uncertain origin must refuse'),
    'owner-namespace': ('safe-cex-parity', 'src/lib.rs', ['--test', 'gates', 'uncertain_imported_impl_owners_still_fail_closed'], 'OWNER uncertain origin must refuse'),
    'twap-scope': ('safe-cex-webworker', 'src/twap_manager.rs', ['--test', 'gates', 'production_twap_singleton_keeps_indexable_declarations'], 'TWAP production source index'),
    'indexer-scope': ('safe-cex-webworker', 'src/chaser_manager.rs', ['--test', 'gates', 'production_chaser_singleton_keeps_indexable_declarations'], 'CHASER production source index'),
    'indexer-assertion': ('safe-cex-parity', 'tests/gates.rs', ['--test', 'gates', 'production_chaser_singleton_keeps_indexable_declarations'], 'CHASER public accessor indexed'),
    'singleton': ('safe-cex-webworker', 'src/chaser_manager.rs', ['--test', 'chaser_manager_singleton', 'accessor_preserves_native_singleton_identity', '--', '--exact'], 'CHASER native singleton identity'),
    'risk-order': ('safe-cex-store', 'src/risk.rs', ['--test', 'derived', 'live_typescript_risk_and_returns', '--', '--include-ignored'], 'RISK-PROFILE source branch field order'),
    'risk': ('safe-cex-store', 'src/risk.rs', ['--lib', 'risk::tests::prototype_bucket_errors_are_catchable'], 'risk::prototype symbol must return TypeError'),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('control', choices=CASES)
    parser.add_argument('--cargo', default=os.environ.get('CARGO', 'cargo'))
    args = parser.parse_args()
    crate, relative, target, assertion = CASES[args.control]
    path = ROOT / 'sdk-rs' / crate / relative
    original = path.read_bytes()
    tracked = subprocess.check_output(['git', 'show', 'HEAD:' + str(path.relative_to(ROOT))], cwd=ROOT)
    if original != tracked:
        raise RuntimeError('refusing to mutate a file with uncommitted changes')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    test_crate = 'safe-cex-parity' if args.control in ('indexer-scope', 'twap-scope', 'lighter-declarations') else crate
    command = [args.cargo, 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', test_crate, '--jobs', '1'] + target
    log_dir = ROOT / 'sdk-rs' / 'target'
    log_dir.mkdir(exist_ok=True)
    try:
        path.write_text(mutate(original.decode(), args.control))
        broken = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        (log_dir / ('orphan-control-' + args.control + '-broken.log')).write_text(broken.stdout)
    finally:
        path.write_bytes(original)
    restored = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (log_dir / ('orphan-control-' + args.control + '-restored.log')).write_text(restored.stdout)
    compiled = 'Finished `test`' in broken.stdout and 'running 1 test' in broken.stdout
    caught = broken.returncode == 101 and compiled and assertion in broken.stdout and 'FAILED' in broken.stdout
    print(f'{head} {args.control}: mutated={broken.returncode} compiled={compiled} named_assertion={assertion!r} caught={caught}; restored={restored.returncode}', flush=True)
    if not caught:
        print(broken.stdout[-5000:], file=sys.stderr)
    if restored.returncode:
        print(restored.stdout[-5000:], file=sys.stderr)
    return 0 if caught and restored.returncode == 0 else 1


if __name__ == '__main__':
    raise SystemExit(main())
