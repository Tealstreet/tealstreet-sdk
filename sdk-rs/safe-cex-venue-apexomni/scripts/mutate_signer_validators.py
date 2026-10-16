"""Eight independent validator mutations in one build and Node corpus."""
from pathlib import Path
import os
import subprocess
root=Path(__file__).resolve().parents[3]
source=root/"sdk-rs/safe-cex-venue-apexomni/src/signer_utils.rs"
original=source.read_text()
mutated=original
controls=[
 ("assertChainId","*value > 0.0","*value <= 0.0"),
 ("assertAccountId","*value > 0.0","*value <= 0.0"),
 ("assertNonce","*value >= 0.0","*value > 0.0"),
 ("assertTokenId","*value > 0.0","*value <= 0.0"),
 ("assertPubKeyHash","count() == 42","count() == 41"),
 ("assertTs","*value > 1_700_000_000.0","*value >= 1_700_000_000.0"),
 ("assertWithdrawFeeRatio","*value <= 10000.0","*value < 10000.0"),
 ("assertTradingFeeRatio","*value >= -255.0","*value > -255.0"),
]
for name,before,after in controls:
 start=mutated.index("// ts: "+name+"\n")
 end=mutated.find("// ts: ",start+1)
 if end==-1:end=len(mutated)
 block=mutated[start:end]
 assert block.count(before)==1,name
 mutated=mutated[:start]+block.replace(before,after)+mutated[end:]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH")
cmd=["/opt/homebrew/opt/rustup/bin/cargo","test","--locked","--manifest-path",str(root/"sdk-rs/Cargo.toml"),"-p","safe-cex-venue-apexomni","--test","signer_utils","--jobs","1","validator_rows_match_actual_typescript","--","--test-threads=1","--nocapture"]
logs=root/"sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True,exist_ok=True)
try:
 source.write_text(mutated)
 result=subprocess.run(cmd,cwd=root,capture_output=True,text=True)
 output=result.stdout+result.stderr
 (logs/"signer-validator-mutations.log").write_text(output)
 assert result.returncode==101 and "test result: FAILED" in output,output
 for name,_,_ in controls:assert name+" direct result" in output,output
 print("8 compiled production mutations failed 8 named validator assertions",flush=True)
finally:
 source.write_text(original)
result=subprocess.run(cmd,cwd=root,capture_output=True,text=True)
(logs/"signer-validator-restored.log").write_text(result.stdout+result.stderr)
assert result.returncode==0,result.stdout+result.stderr
print("Restored 8 rows pass; two sequential Cargo builds, one Node corpus per run",flush=True)
