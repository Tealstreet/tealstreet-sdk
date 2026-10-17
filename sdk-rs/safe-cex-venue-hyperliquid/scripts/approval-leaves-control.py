#!/usr/bin/env python3
"""One reversible production mutation for nine retained roots; run on committed inputs."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

crate = Path(__file__).resolve().parents[1]
root = crate.parents[1]
paths = [crate/'src/builder_fee.rs', crate/'src/agent.rs', crate/'src/types.rs']
originals = {path:path.read_text() for path in paths}
sha = subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
subprocess.run(['git','diff','--exit-code','HEAD','--',str(crate/'src'),str(crate/'tests'),str(crate/'scripts')],cwd=root,check=True)
logs = Path(tempfile.mkdtemp(prefix='hyperliquid-approval-control-'))
command = ['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-hyperliquid','--test','approval_leaves','--jobs','2','--','--nocapture']
environment=dict(os.environ)
environment.setdefault('SAFE_CEX_TYPESCRIPT_PATH','/Users/sam/cs/tealstreet-next/node_modules/typescript')
required=[
 'APPROVAL native DEFAULT_BUILDER_FEE_TENTHS_BPS bits',
 'APPROVAL native DEFAULT_BUILDER_FEE_RATE text',
 'APPROVAL native HYPERLIQUID_BUILDER_DOMAIN name',
 'APPROVAL native HYPERLIQUID_BUILDER_DOMAIN_TESTNET name',
 'APPROVAL native APPROVE_BUILDER_FEE_TYPES field name 0',
 'APPROVAL native APPROVE_AGENT_TYPES field name 0',
 'APPROVAL native generateAgentWalletName name case 0',
 'APPROVAL native hasEthereumWallet value case 1',
 'APPROVAL native CancelOrderRequests asset bits case 1 item 0',
]
try:
    builder,agent,core=paths
    text=originals[builder]
    assert text.count('= TEALSTREET_BUILDER_FEE;')==1
    text=text.replace('= TEALSTREET_BUILDER_FEE;','= TEALSTREET_BUILDER_FEE + 1.0;')
    assert text.count('name: "HyperliquidSignTransaction"')==2
    text=text.replace('name: "HyperliquidSignTransaction"','name: "HyperliquidSignTransaction#CONTROL"')
    assert text.count('name: "hyperliquidChain"')==1
    text=text.replace('name: "hyperliquidChain"','name: "hyperliquidChain#CONTROL"')
    builder.write_text(text)
    text=originals[agent]
    assert text.count('name: "hyperliquidChain"')==1 and text.count('for _ in 0..4')==1 and text.count('window.ethereum_truthy()')==1
    text=text.replace('name: "hyperliquidChain"','name: "hyperliquidChain#CONTROL"').replace('for _ in 0..4','for _ in 0..3').replace('window.ethereum_truthy()','window.ethereum_truthy().map(|value| !value)')
    agent.write_text(text)
    pattern=r'(struct CancelOrderRequestsItemWire\s*\{[^}]+\})'
    def swap(match):
        block=match[1]
        assert block.count('rename = "a"')==block.count('rename = "o"')==1
        # Symmetric encode/decode metadata swaps evade a roundtrip; native
        # 17.5/91.25 member assertions must detect the decoded swap instead.
        return block.replace('rename = "a"','rename = "#TEMP"').replace('rename = "o"','rename = "a"').replace('rename = "#TEMP"','rename = "o"')
    text,count=re.subn(pattern,swap,originals[core]); assert count==1
    core.write_text(text)
    mutant=subprocess.run(command,cwd=root,env=environment,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    (logs/'mutant.log').write_text(mutant.stdout)
finally:
    for path,text in originals.items(): path.write_text(text)
restored=subprocess.run(command,cwd=root,env=environment,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(logs/'restored.log').write_text(restored.stdout)
result={'gate_sha':sha,'cargo_mutant_exit':mutant.returncode,'cargo_restored_exit':restored.returncode,
        'named_native_failures':[name for name in required if name in mutant.stdout],
        'restored_files_equal':all(path.read_text()==text for path,text in originals.items()),'logs':str(logs)}
print(json.dumps(result,indent=2))
assert mutant.returncode==101 and '4 failed' in mutant.stdout, 'must reach all four failing native tests'
assert all(name in mutant.stdout for name in required), 'missing assertion; a surviving control is a finding'
assert restored.returncode==0 and '4 passed' in restored.stdout, 'restored gate must pass'
