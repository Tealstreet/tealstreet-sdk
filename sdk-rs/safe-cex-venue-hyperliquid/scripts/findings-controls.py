#!/usr/bin/env python3
"""Sequential compiling mutations against the selected findings' native gates.

Run from any cwd. CARGO and SAFE_CEX_TYPESCRIPT_PATH bind local tools. Logs and
receipts stay in an external output directory; this script never commits or pushes.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
CRATE = ROOT / 'sdk-rs/safe-cex-venue-hyperliquid'


def change(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError(f'expected one mutation site, found {text.count(old)}: {old!r}')
    return text.replace(old, new, 1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--start-index',type=int,default=0)
    parser.add_argument('--output', type=Path, default=Path(tempfile.mkdtemp(prefix='hyper-findings-controls-')))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    cargo = os.environ.get('CARGO') or shutil.which('cargo')
    if not cargo:
        raise RuntimeError('cargo is required')
    sha = subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    env = dict(os.environ, RUSTFLAGS='-Dwarnings')
    base = [cargo,'test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-hyperliquid']
    def gate(target, name):
        return base + (['--lib'] if target == 'lib' else ['--test',target]) + [name,'--locked','--jobs','2']
    cases = [
        ('constructor-name','src/order_response.rs','name: "HyperliquidUnreadableOrderResponseError",','name: "BrokenUnreadableError",','finding_constructors','unreadable_constructor_owns_exact_name_and_arbitrary_message','ERROR constructor native name'),
        ('constructor-default','src/rate_limit.rs','capacity.unwrap_or(1200.0)','capacity.unwrap_or(600.0)','lib','source_constructor_defaults_preserve_omission_and_explicit_values','DEFAULTS native msPerToken'),
        ('ignored-surrogate','src/api.rs','pub fn get_request_action_type(data: &Value) -> Option<String> {','pub fn get_request_action_type(data: &Value) -> Option<String> {\n if data.as_str().is_some_and(|raw|raw.contains(r"\\ud800")){return None;}','weight_ingress','request_action_type_parser_findings_are_entry_specific','ACTION native ignored surrogate does not suppress type'),
        ('key-precedence','src/api.rs','crate::signing::validate_wallet_key(&self.options.secret)','crate::signing::validate_wallet_key(&format!("0x{}", "11".repeat(32)))','api','signing_validates_wallet_before_invalid_vault_and_encoded_cancel_retains_action','KEY native key failure precedes invalid vault'),
        ('wallet-bool','src/signing.rs','return Ok(Value::Bool(wallet_truthy(&value)));','return Ok(Value::Bool({let _=wallet_truthy(&value);matches!(value,WalletView::Json(Value::Bool(true)))}));','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIGN native signature r'),
        ('wallet-bytes','src/signing.rs','let text = value.as_str().ok_or("invalid BytesLike value")?;\n        address_to_bytes(text)?;','if let Some(text)=value.as_str(){address_to_bytes(text)?;}','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIGN native refused source-invalid wallet payload'),
        ('recovery-v0','src/signing.rs','0 | 1 => 27 + bytes[64],','0 | 1 => bytes[64],','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIG native recovery bits'),
        ('compact-signature','src/signing.rs','if !matches!(bytes.len(), 64 | 65) {','if bytes.len() != 65 {','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIG native source-accepted compact/full signature'),
        ('event-user','src/spot_user_event.rs','| Self::NonUserCancels { user, .. } => user,','| Self::NonUserCancels { user, .. } => {static EMPTY: JsString=JsString(Vec::new());let _=user;&EMPTY},','finding_user_event','core_event_retains_all_four_native_intersection_alternatives','EVENT native owned user units'),
        ('core-client-attachment','src/core_exchange_class.rs','WsPingHost::set(h, &owner, "xhr", &xhr)?;','WsPingHost::set(h, &owner, "xhr", &{let _=&xhr;owner.clone()})?;','finding_core_class','native_core_class_constructs_clients_sockets_and_callable_frame','CORE native attached client xhr'),
        ('core-return-adoption','src/helper_tasks.rs','start_impl(host, body, false)','start_impl(host, body, true)','finding_core_class','native_core_class_constructs_clients_sockets_and_callable_frame','CORE no duplicate outer return assimilation'),
        ('public-fixed-order','src/ws_emission.rs','for name in ["allMids", "fastAssetCtxs"] {','for name in ["fastAssetCtxs", "allMids"] {','finding_public_class','source_class_uses_installed_native_arrows_and_live_subscription_state','PUBLIC native fixed feed order'),
        ('public-bbo','src/ws_emission.rs','&["l2Book", "bbo"]','&["l2Book"]','finding_public_class','source_class_uses_installed_native_arrows_and_live_subscription_state','PUBLIC candle plus depth plus BBO native send floor'),
        ('native-diagnostic-unit','src/api.rs','.encode_utf16().take(300)','.encode_utf16().take(299)','api','required_native_error_string_records_exact_split_surrogate_cost_before_source_health_getters','DIAGNOSTIC native exact split-surrogate replacement'),
        ('wallet-inherited-bool','src/signing.rs','WalletView::Stock(value) => value.truthy(),','WalletView::Stock(value) => {let _=value;false},','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIGN native signature r object-toString-inherited'),
        ('wallet-nested-inheritance','src/signing.rs','WalletView::Stock(reference.get(name))','WalletView::Stock(if name=="toString"{safe_cex_num::prototype_realm::PrototypeValue::Undefined}else{reference.get(name)})','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIGN native signature r nested-object-inherited'),
        ('constructor-message','src/order_response.rs','message: message.into(),','message: {let _: String=message.into();String::new()},','finding_constructors','unreadable_constructor_owns_exact_name_and_arbitrary_message','ERROR constructor native original message'),
        ('encoded-action-callsite','src/api.rs','get_request_action_type(&Value::String(raw.to_owned()))','serde_json::from_str::<Value>(raw).ok().as_ref().and_then(get_request_action_type)','api','signing_validates_wallet_before_invalid_vault_and_encoded_cancel_retains_action','ACTION native encoded cancellation suppresses terminal toast despite ignored surrogate'),
        ('wallet-primary-type','src/signing.rs','projected["primaryType"] = kind.clone().into();','projected["primaryType"] = data.get("primaryType").cloned().unwrap_or_else(|| kind.clone().into());','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIGN native source-accepted wallet payload "wrong-primary-type"'),
        ('wallet-integral-f64','src/signing.rs','            return Ok(Value::String(if number == 0.0 {','            if matches!(&value,Value::Number(original) if original.is_f64()){return Ok(value); }\n            return Ok(Value::String(if number == 0.0 {','lib','wallet_projection_and_signature_parser_follow_source_native_values','SIGN native source-accepted wallet payload "uint-number-one"'),
        ('public-user-composition','src/ws_emission.rs','    method(h, socket, "subscribeUserTopics", &[])?;\n    Ok(())\n}\npub fn core_public_send_subscription','    Ok(())\n}\npub fn core_public_send_subscription','finding_public_class','source_class_uses_installed_native_arrows_and_live_subscription_state','PUBLIC candle plus depth plus BBO native send floor: installed fixed-to-user edge'),
        ('public-dispose-composition','src/public_class.rs','    lifecycle_delegates::public_dispose(&mut scope, receiver)?;','    if false { lifecycle_delegates::public_dispose(&mut scope, receiver)?; }','finding_public_class','source_class_uses_installed_native_arrows_and_live_subscription_state','PUBLIC installed dispose reaches stop then super'),
        ('public-interval-own-undefined', 'src/ws_emission.rs', '            if *kind == "candle" {\n                h.define_data(&sub, "interval", &interval)?;\n            }\n            h.call(&sender, socket, &[sub])?;', '            if *kind == "candle" {\n                h.define_data(&sub, "interval", &interval)?;\n            } else {\n                let undefined = h.undefined();\n                h.define_data(&sub, "interval", &undefined)?;\n            }\n            h.call(&sender, socket, &[sub])?;', 'finding_public_class', 'source_class_uses_installed_native_arrows_and_live_subscription_state', 'PUBLIC composed native own subscription keys 1'),
    ]
    receipt={'base_sha':sha,'scope':'selected16 findings, finite mutation floor; no exhaustive mutation claim','mutations':[]}
    for index,(name,relative,old,new,target,test,assertion) in enumerate(cases):
        if index < args.start_index:
            continue
        path = CRATE / relative
        original = path.read_bytes()
        record={'index':index,'name':name,'call_site':str(path.relative_to(ROOT)),'base_blob_sha256':hashlib.sha256(original).hexdigest(),'target':test,'named_assertion':assertion,'command':gate(target,test)}
        # Record a portable tool name in public receipts, not a local cargo path.
        record['command'][0]='cargo'
        try:
            path.write_text(change(original.decode(),old,new))
            broken = subprocess.run(gate(target,test),cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
            (args.output/(name+'-broken.log')).write_text(broken.stdout)
            record['broken_exit']=broken.returncode
            record['assertion_seen']=assertion in broken.stdout
            record['compiled']='could not compile' not in broken.stdout and 'error[E' not in broken.stdout
            record['runtime_failed']='test result: FAILED' in broken.stdout
            record['disposition']='caught' if broken.returncode == 101 and record['compiled'] and record['runtime_failed'] and record['assertion_seen'] else 'SURVIVOR' if broken.returncode==0 else 'INVALID_CONTROL'
        finally:
            path.write_bytes(original)
        restored = subprocess.run(gate(target,test),cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        (args.output/(name+'-restored.log')).write_text(restored.stdout)
        record['restored_exit']=restored.returncode
        record['restored_one_native_test']='1 passed; 0 failed' in restored.stdout
        receipt['mutations'].append(record)
        (args.output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
        print(json.dumps({k:record[k] for k in ['index','name','broken_exit','disposition','restored_exit']}),flush=True)
        if record['disposition']=='INVALID_CONTROL' or restored.returncode or not record['restored_one_native_test']:
            raise RuntimeError('control or restoration requires inspection; no compile failure counts as a gate')
    receipt['stopped_after']=len(cases)-1
    receipt['stop_name']=cases[-1][0]
    receipt['head_unchanged']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==sha
    (args.output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')


if __name__ == '__main__':
    main()
