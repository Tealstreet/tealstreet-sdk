"""Run consuming sequence-loss controls against actual production wire decoders.

From the worktree root, set SAFE_CEX_TYPESCRIPT_PATH and optionally CARGO, then
run this file with Python 3.14. One scoped Cargo test and one Node corpus per run;
never run alongside another Cargo invocation. Production and tests are restored
in finally. --without-cardinality-guards removes the added count/length guards
while retaining full native-value assertions, which must still fail by name.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--without-cardinality-guards', action='store_true')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[3]
crate = repo / 'sdk-rs/safe-cex-venue-backpack'
production = crate / 'src/types.rs'
test = crate / 'tests/types.rs'
out = repo / 'sdk-rs/target/backpack-cardinality'
out.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'actual TypeScript path required'
original = production.read_text()
original_test = test.read_text()
helper = r'''
// Injected control ONLY: consume the complete sequence, then yield None.
fn consuming_sequence_control<'de, D, T>(decoder: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    struct ConsumeAll<A>(A);
    impl<'de, A: serde::de::SeqAccess<'de>> serde::de::SeqAccess<'de> for ConsumeAll<A> {
        type Error = A::Error;
        fn next_element_seed<S: serde::de::DeserializeSeed<'de>>(
            &mut self,
            _seed: S,
        ) -> Result<Option<S::Value>, Self::Error> {
            while serde::de::SeqAccess::next_element::<serde::de::IgnoredAny>(&mut self.0)?.is_some() {}
            Ok(None)
        }
    }
    struct SequenceVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for SequenceVisitor<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, input: A) -> Result<Self::Value, A::Error> {
            serde::Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(ConsumeAll(input)))
        }
    }
    decoder.deserialize_seq(SequenceVisitor(std::marker::PhantomData))
}
'''
fields = [
    ('BackpackDepth', 'bids'), ('BackpackDepth', 'asks'),
    ('BackpackCollateral', 'collateral'),
    ('BackpackWsSubscribeMessage', 'params'),
    ('BackpackWsUnsubscribeMessage', 'params'),
    ('BackpackWsDepthUpdate', 'b'), ('BackpackWsDepthUpdate', 'a'),
]
mode = 'guards-off' if args.without_cardinality_guards else 'guards-on'
mutated_test = original_test
if args.without_cardinality_guards:
    # Disable ALL newly added cardinality checks, not merely the first failure.
    # Remove the traversal too: with no length guard its indexed walk could
    # panic instead of reaching the independent full native-value assertion.
    mutated_test = mutated_test.replace('include!("cardinality.inc");\n', '')
    mutated_test = re.sub(r'assert_native_cardinality\([\s\S]*?\);',
                          lambda m: m[0].replace('assert_native_cardinality(', 'let _ = (', 1),
                          mutated_test)
    mutated_test = re.sub(r'assert_eq!\([\s\S]*?\);',
                          lambda m: m[0].replace('assert_eq!(', 'let _ = (', 1)
                          if 'CARDINALITY' in m[0] else m[0], mutated_test)
    assert mutated_test != original_test
cmd = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', 'sdk-rs/Cargo.toml',
       '-p', 'safe-cex-venue-backpack', '--test', 'types', '--jobs', '1']
results = []
for record, field in fields:
    name = f'consuming-{record}-{field}-{mode}'
    start = original.index(f'struct {record}Wire {{')
    end = original.index('\n}', start)
    block = original[start:end]
    needle = f'    {field}: Vec<'
    assert block.count(needle) == 1, (record, field)
    changed = block.replace(needle, '    #[serde(deserialize_with = "consuming_sequence_control")]\n' + needle)
    mutant = original[:start] + changed + original[end:] + helper
    assertion = (f'decoded native fields/bits {record}' if args.without_cardinality_guards
                 else f'NATIVE-CARDINALITY array {record}')
    try:
        production.write_text(mutant)
        test.write_text(mutated_test)
        result = subprocess.run(cmd, cwd=repo, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / f'{name}.log').write_text(result.stdout)
        assert result.returncode == 101 and assertion in result.stdout, (name, result.returncode, out / f'{name}.log')
        record_result = {'name': name, 'exit': result.returncode,
                         'assertion': next(line for line in result.stdout.splitlines() if assertion in line)}
    finally:
        production.write_text(original)
        test.write_text(original_test)
    restored = subprocess.run(cmd, cwd=repo, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (out / f'{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, (name, out / f'{name}-restored.log')
    record_result['restored_exit'] = restored.returncode
    results.append(record_result)
    (out / f'consuming-{mode}-results.json').write_text(json.dumps(results, indent=2))
    print(record_result, flush=True)
