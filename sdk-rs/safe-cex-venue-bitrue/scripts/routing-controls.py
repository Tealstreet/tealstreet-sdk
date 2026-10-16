"""Sequential compiling controls for the concrete source HTTP factory binding."""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitrue/src/binding/private.rs'
original = source.read_text()
command = ['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'--locked','-p','safe-cex-venue-bitrue','--test','exchange_binding','source_routing_authorizes_signed_requests_and_installs_bigint_only_for_tunnel','--','--exact']
controls = [
 ('factory','    fn axios_create(', '    fn transport(', '    fn axios_create(&self, config: Js, _options: Js, _exchange: Js) -> Result<Js, Thrown> { Ok(client::create(self.shared.http.clone(), config)) }\n','ROUTING selected factory base'),
 ('adapter','    fn transport(', '    fn clean_float(', '    fn transport(&self, _client: Js, _options: Js, _name: Js, _parser: &str) -> Result<(), Thrown> { Ok(()) }\n','ROUTING both source clients use bigInt adapter'),
]
for name, start, end, replacement, assertion in controls:
 a, b = original.index(start), original.index(end)
 try:
  source.write_text(original[:a] + replacement + original[b:])
  result = subprocess.run(command,capture_output=True,text=True)
  output = result.stdout + result.stderr
  Path('/tmp/bitrue-routing-control-'+name+'.log').write_text(output)
  assert result.returncode == 101 and assertion in output and 'could not compile' not in output, output
  print('COMPILED EXIT101 '+name+' -> '+assertion,flush=True)
 finally:
  source.write_text(original)
 result = subprocess.run(command,capture_output=True,text=True)
 assert result.returncode == 0, result.stdout+result.stderr
 print('RESTORED PASS '+name,flush=True)
