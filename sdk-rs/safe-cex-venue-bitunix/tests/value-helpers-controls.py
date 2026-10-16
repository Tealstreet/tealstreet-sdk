"""Per-row production mutations, sequential one-target corpus runs."""
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bitunix/src/value_helpers.rs'
original=source.read_text()
logs=root/'sdk-rs/target/bitunix-value-controls'
logs.mkdir(parents=True,exist_ok=True)
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--test','value_helpers','--jobs','1','--','--nocapture']
controls=[('parsed.is_finite()', 'parsed.is_nan()', 'parseFiniteNumber'),('!= 0.0','== 0.0','isApiTradingSupported'),('Ok(js_number(value)?.is_finite())','Ok(!js_number(value)?.is_finite())','hasFiniteTriggerPrice')]
for old,new,name in controls:
    assert original.count(old)==1,(old,original.count(old))
    try:
        source.write_text(original.replace(old,new))
        result=subprocess.run(command,text=True,capture_output=True)
        (logs/(name+'-fail.log')).write_text(result.stdout+result.stderr)
        assert result.returncode not in (0,126) and name in result.stdout+result.stderr,result.stdout+result.stderr
        print('FAIL observed: '+name,flush=True)
    finally:
        source.write_text(original)
    result=subprocess.run(command,text=True,capture_output=True)
    (logs/(name+'-pass.log')).write_text(result.stdout+result.stderr)
    assert result.returncode==0,result.stdout+result.stderr
    print('PASS restored: '+name,flush=True)
