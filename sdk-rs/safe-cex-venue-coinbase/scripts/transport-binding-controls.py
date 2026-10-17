#!/usr/bin/env python3
"""Compile isolated production reversions against Coinbase real-host transport tests."""
import hashlib, os, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
source=root/'safe-cex-venue-coinbase/src/binding/api.rs'
original=source.read_bytes()
text=original.decode()
OLD_FACTORY='    fn safe_create(\n        &self,\n        config: ApiCreateConfig,\n        _options: Frame,\n        _exchange: Frame,\n    ) -> Result<Frame, PortfolioFailure> {\n        let headers = Js::object(vec![(\n            "Accept",\n            Js::str("application/json, text/plain, */*"),\n        )]);\n        for (name, value) in config.headers {\n            headers\n                .define(name, value.as_ref().map(to_js).unwrap_or(Js::Undefined))\n                .map_err(from_thrown)?;\n        }\n        let serialize = config.serialize;\n        let client = client::create(\n            self.shared.http.clone(),\n            Js::object(vec![\n                ("baseURL", Js::str(config.base_url)),\n                ("timeout", to_js(&config.timeout)),\n                ("headers", headers),\n                (\n                    "paramsSerializer",\n                    Js::object(vec![(\n                        "serialize",\n                        Js::function("serialize", move |_, args| {\n                            let value = from_js(&arg(&args, 0))\n                                .map_err(|error| Thrown::from(exchange_error(&error)))?\n                                .unwrap_or_else(|| Frame::Object(Rc::default()));\n                            serialize(value)\n                                .map(|text| Js::str(&text))\n                                .map_err(|error| Thrown::from(exchange_error(&error)))\n                        }),\n                    )]),\n                ),\n            ]),\n        );\n        Ok(self.register_client(client))\n    }\n'
OLD_INSTALLER='    fn install_transport_adapter(\n        &self,\n        _client: Frame,\n        _options: Frame,\n        _exchange_name: Option<Frame>,\n    ) -> Result<(), PortfolioFailure> {\n        // The supplied JsHttpHost owns the terminal transport, including HOW.\n        Ok(())\n    }\n'

def replace_region(start,end,replacement):
    a=text.index(start);b=text.index(end,a)
    return text[:a]+replacement+text[b:]
def run(target=None):
    args=['cargo','test','--locked','-p','safe-cex-venue-coinbase','--jobs=1','--test','transport_binding']
    if target:args += [target,'--','--exact','--nocapture']
    return subprocess.run(args,cwd=root,env={**os.environ,'RUSTC_BOOTSTRAP':'1'},stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
factory='real_coinbase_factory_uses_provider_routing'
tunnel='real_coinbase_tunnel_installs_adapter_or_retains_http_host'
admission='            && key.as_ref() != "adapter".encode_utf16().collect::<Vec<_>>()\n'
retention='                    if !adapter.is_undefined() {\n                        output.define("adapter", adapter)?;\n                    }'
assert text.count(admission)==1 and text.count(retention)==1
mutants=[
    ('provider-factory',factory,'real Coinbase factory must consult provider routing',replace_region('    fn safe_create(','    fn serialize_repeat(',OLD_FACTORY)),
    ('tunnel-installer',tunnel,'real Coinbase tunnel must consult provider adapter',replace_region('    fn install_transport_adapter(','    fn replace_url_params(',OLD_INSTALLER)),
    ('executable-adapter-admission',tunnel,'account failed Coinbase available=true',text.replace(admission,'')),
    ('executable-adapter-retention',tunnel,'real Coinbase tunnel uses selected adapter',text.replace(retention,'                    drop(adapter);')),
]
baseline=run();assert baseline.returncode==0,baseline.stdout
print('BASELINE real-host tests and actual TS8 scenes PASS',flush=True)
for name,target,label,mutant in mutants:
    assert source.read_bytes()==original
    try:
        source.write_text(mutant)
        result=run(target)
        assert result.returncode==101 and 'test result: FAILED' in result.stdout and label in result.stdout,result.stdout
        print(result.stdout,flush=True)
    finally:
        source.write_bytes(original)
        assert source.read_bytes()==original
    restored=run(target);assert restored.returncode==0,restored.stdout
    print('CONTROL '+name+': compiled named RED / exact-restored PASS',flush=True)
final=run();assert final.returncode==0,final.stdout
assert source.read_bytes()==original
print('RESTORED real-host+TS8 PASS; SHA256 '+hashlib.sha256(original).hexdigest(),flush=True)
