#!/usr/bin/env python3
"""Remove each concrete string frontdoor, require a tagged-map failure, restore."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-crypto/src/lib.rs'
original = source.read_bytes()
command = ['cargo', 'test', '-p', 'safe-cex-crypto', '--test', 'string_enum_ingress', '--jobs', '1']


def run():
    result = subprocess.run(command, cwd=workspace, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, end='', flush=True)
    return result


try:
    for owner, name in [
        ('Encoding', 'encoding_accepts_only_literal_strings_and_preserves_native_variants'),
        ('WsOutputFormat', 'ws_output_format_accepts_only_literal_strings_and_preserves_native_variants'),
    ]:
        text = original.decode()
        start = text.index(f"impl<'de> Deserialize<'de> for {owner} {{")
        first = text.index('        let value = String::deserialize(deserializer)?;', start)
        last = text.index('\n    }\n}', first)
        mutant = text[:first] + f'        {owner}Wire::deserialize(deserializer)' + text[last:]
        source.write_text(mutant)
        result = run()
        assert result.returncode == 101, f'{owner}: expected runtime rejection'
        assert f'test {name} ... FAILED' in result.stdout, f'{owner}: named witness must fail'
        assert 'raw JSON tagged map must be refused for hex' in result.stdout, f'{owner}: valid tagged-map failure required'
        source.write_bytes(original)
        assert run().returncode == 0, f'{owner}: exact restoration must pass'
finally:
    source.write_bytes(original)

assert source.read_bytes() == original
print('Both string-frontdoor removals compile and fail their named tagged-map assertions; exact restorations pass.')
