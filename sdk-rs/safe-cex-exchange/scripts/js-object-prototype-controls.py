"""Require compiled ordinary-object prototype faults to fail actual Node/native witnesses."""
from pathlib import Path
import os
import subprocess

workspace = Path(__file__).resolve().parents[2]
base = workspace / 'safe-cex-exchange/src/js_runtime'
checks = [
    ('allocator', 'value.rs', None,
     '''proto: RefCell::new(Some(Js::Builtin(
                safe_cex_num::PrototypeRealm::current()
                    .reference(safe_cex_num::ProtoMember::Prototype),
            ))),''', 'proto: RefCell::new(None),',
     'ordinary_allocations_keep_their_realms_default_object_prototype',
     'OBJECT_PROTOTYPE ordinary allocator links canonical realm identity'),
    ('legacy', 'value.rs', None,
     'self.set_prototype(value)?;', 'let _ = value;',
     'legacy_prototype_setter_preserves_receiver_null_shadow_and_resource',
     'OBJECT_PROTOTYPE legacy setter uses original receiver'),
    ('static-binding', 'globals.rs', None,
     'arg(&args, 0).set_prototype(arg(&args, 1))', 'Ok(arg(&args, 0))',
     'object_set_prototype_of_binding_preserves_order_identity_and_refusals',
     'OBJECT_PROTOTYPE static binding bypasses shadowing'),
    ('extensibility', 'value.rs', '    pub(super) fn set_prototype(',
     'if !self.is_extensible()? {', 'if false && !self.is_extensible()? {',
     'object_set_prototype_of_binding_preserves_order_identity_and_refusals',
     'OBJECT_PROTOTYPE static validation and metadata semantics'),
]
for name, filename, start, before, after, test, marker in checks:
    source = base / filename
    original = source.read_bytes()
    text = original.decode()
    offset = text.index(start) if start else 0
    end = text.index('\n    ///', offset + 1) if start else len(text)
    body = text[offset:end]
    assert body.count(before) == 1, (name, body.count(before))
    command = ['cargo', 'test', '-p', 'safe-cex-exchange', '--features', 'native',
               '--jobs', '1', '--test', 'js_object_prototypes', test, '--', '--exact', '--nocapture']
    try:
        source.write_text(text[:offset] + body.replace(before, after) + text[end:])
        result = subprocess.run(command, cwd=workspace, env=os.environ, capture_output=True, text=True)
        log = result.stdout + result.stderr
        Path('/tmp/js-object-prototype-' + name + '-red.log').write_text(log)
        assert result.returncode == 101 and 'test result: FAILED' in log and marker in log, log
        print(name, 'compiled named RED', flush=True)
    finally:
        source.write_bytes(original)
    result = subprocess.run(command, cwd=workspace, env=os.environ, capture_output=True, text=True)
    log = result.stdout + result.stderr
    Path('/tmp/js-object-prototype-' + name + '-restored.log').write_text(log)
    assert result.returncode == 0 and '1 passed' in log, log
    assert source.read_bytes() == original
    print(name, 'exact source restore GREEN', flush=True)
