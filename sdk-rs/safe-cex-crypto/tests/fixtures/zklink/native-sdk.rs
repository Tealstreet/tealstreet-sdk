// Oracle calls the pinned official SDK unchanged; it never imports safe-cex-crypto.
use serde::Serialize;
use zklink_sdk_signers::zklink_signer::ZkLinkSigner;
#[derive(Serialize)]
struct Case {name:String,seed:String,payload:String,#[serde(rename="publicKey")]public_key:String,signature:String}
#[derive(Serialize)]
struct Corpus {oracle:&'static str,#[serde(rename="sdkRevision")]sdk_revision:&'static str,compatibility:&'static str,cases:Vec<Case>}
fn main() {
    let args:Vec<_>=std::env::args().collect();
    let npm:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    for case in npm["cases"].as_array().unwrap() {
        let seed=hex::decode(case["seed"].as_str().unwrap()).unwrap();
        let payload=hex::decode(case["payload"].as_str().unwrap()).unwrap();
        let signer=ZkLinkSigner::new_from_seed(&seed).unwrap();let signature=signer.sign_musig(&payload).unwrap();
        assert!(signature.verify_musig(&payload));
        assert_eq!(signature.pub_key.as_hex(),case["signed"]["signature"]["pubKey"].as_str().unwrap(),"native SDK/npm public-key cross-link");
        assert_eq!(hex::encode(signature.signature.as_bytes()),case["signed"]["signature"]["signature"].as_str().unwrap(),"native SDK/npm signature cross-link");
    }
    let mut seed=vec![1u8;64];seed.push(27);let signer=ZkLinkSigner::new_from_seed(&seed).unwrap();
    let cases=[0usize,92].into_iter().map(|length|{
        let payload:Vec<u8>=(0..length).map(|i|i as u8+1).collect();
        let signature=signer.sign_musig(&payload).unwrap();assert!(signature.verify_musig(&payload));
        Case{name:format!("boundary-{length}"),seed:hex::encode(&seed),payload:hex::encode(payload),public_key:signature.pub_key.as_hex().trim_start_matches("0x").to_owned(),signature:hex::encode(signature.signature.as_bytes())}
    }).collect();
    let corpus=Corpus{oracle:"unmodified zkLinkProtocol/zklink_sdk native ZkLinkSigner",sdk_revision:"3152525e2615ecbbdb56fd6728ce265ad2dde485",compatibility:"time 0.3.36 family only; stable compiler; crypto pins unchanged",cases};
    let result=serde_json::to_string_pretty(&corpus).unwrap()+"\n";
    if args.iter().any(|arg|arg=="--capture") {
        use std::io::Write;
        std::fs::OpenOptions::new().write(true).create_new(true).open(&args[2]).expect("immutable native fixture already exists").write_all(result.as_bytes()).unwrap();
    } else {assert_eq!(result,std::fs::read_to_string(&args[2]).unwrap(),"native SDK boundary oracle drift");}
    println!("native SDK matches all four npm cases and both 0/92-byte boundaries");
}
