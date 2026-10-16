"""One compiled native-value mutation for each transaction declaration."""
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-apexomni/src/transaction_records.rs"
original = source.read_text()
logs = root / "sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True, exist_ok=True)
command = ["cargo", "test", "--locked", "--manifest-path", str(root / "sdk-rs/Cargo.toml"),
           "-p", "safe-cex-venue-apexomni", "--test", "transaction_records", "--jobs", "2",
           "--", "--test-threads=1"]
swaps = [
    ("TxSignature", "tx_signature", "pub_key", "signature", "pubKey"),
    ("SignContractPayload", "sign_contract_payload", "account_id", "sub_account_id", "accountId"),
    ("TxContractData", "tx_contract_data", "account_id", "sub_account_id", "accountId"),
    ("SignOrderPayload", "sign_order_payload", "account_id", "sub_account_id", "accountId"),
    ("TxOrderData", "tx_order_data", "account_id", "sub_account_id", "accountId"),
    ("SignTransferPayload", "sign_transfer_payload", "account_id", "from_sub_account_id", "accountId"),
    ("TxTransferData", "tx_transfer_data", "account_id", "from_sub_account_id", "accountId"),
    ("SignWithdrawPayload", "sign_withdraw_payload", "to_chain_id", "sub_account_id", "toChainId"),
    ("TxWithdrawData", "tx_withdraw_data", "to_chain_id", "sub_account_id", "toChainId"),
    ("ChangePubKeyCREATE2", "change_pub_key_create2", "creator_address", "salt_arg", "creatorAddress"),
    ("SignChangePubKeyPayload", "sign_change_pub_key_payload", "chain_id", "account_id", "chainId"),
    ("TxChangePubKeyData", "tx_change_pub_key_data", "sub_account_id", "account_id", "subAccountId"),
]
wrappers = [
    ("SignedContractData", "signed_contract_data", "pair_id", "pairId"),
    ("SignedOrderData", "signed_order_data", "base_token_id", "baseTokenId"),
    ("SignedTransferData", "signed_transfer_data", "to_sub_account_id", "toSubAccountId"),
    ("SignedWithdrawData", "signed_withdraw_data", "withdraw_fee_ratio", "withdrawFeeRatio"),
    ("SignedChangePubKeyData", "signed_change_pub_key_data", "chain_id", "chainId"),
]

def replace_record(text, name, changes):
    start = text.index(f"impl<'de> serde::Deserialize<'de> for {name} {{")
    end = text.index(f"impl JsNumberOutput for {name} {{", start)
    part = text[start:end]
    for before, after in changes:
        pattern = r"(?<![a-zA-Z0-9_])" + re.escape(before)
        assert len(re.findall(pattern, part)) == 1, (name, before)
        part = re.sub(pattern, lambda _: after, part)
    return text[:start] + part + text[end:]

def replace_fields(text, name, changes):
    start = text.index(f"pub struct {name} {{")
    end = text.index("\n}", start)
    part = text[start:end]
    for before, after in changes:
        assert part.count(before) == 1, (name, before)
        part = part.replace(before, after)
    return text[:start] + part + text[end:]

def wire_name(field):
    first, *rest = field.split("_")
    return first + "".join(word.title() for word in rest)

mutant = original
expectations = []
for name, test, first, second, wire in swaps:
    mutant = replace_record(mutant, name, [(f"{first}: fields.{first},", f"{first}: fields.{second},"),
                                          (f"{second}: fields.{second},", f"{second}: fields.{first},")])
    first_wire, second_wire = wire_name(first), wire_name(second)
    mutant = replace_fields(mutant, name, [(f'#[serde(rename = "{first_wire}")]', '#[serde(rename = "MUTATION_PLACEHOLDER")]'),
                                         (f'#[serde(rename = "{second_wire}")]', f'#[serde(rename = "{first_wire}")]'),
                                         ('#[serde(rename = "MUTATION_PLACEHOLDER")]', f'#[serde(rename = "{second_wire}")]')])
    expectations.append((test, f"{name} native {wire}"))
for name, test, field, wire in wrappers:
    mutant = replace_record(mutant, name, [("tx: fields.tx", f"tx: {{ let mut tx = fields.tx; tx.{field} += 1.0; tx }}")])
    child = name.removeprefix("Signed").removesuffix("Data")
    child = "Tx" + child + "Data"
    serializer = "control_" + test
    mutant = replace_fields(mutant, name, [('pub tx:', f'#[serde(serialize_with = "{serializer}")]\n    pub tx:')])
    mutant += f'\nfn {serializer}<S: serde::Serializer>(value: &{child}, serializer: S) -> Result<S::Ok, S::Error> {{\n    let mut restored = value.clone(); restored.{field} -= 1.0; serde::Serialize::serialize(&restored, serializer)\n}}\n'
    expectations.append((test, f"{name} native tx.{wire}"))
mutant = replace_record(mutant, "ChangePubKeyECDSA", [("eth_signature: fields.eth_signature,", 'eth_signature: fields.eth_signature + "!",')])
expectations.append(("change_pub_key_ecdsa", "ChangePubKeyECDSA native ethSignature"))
before = 'pub enum OnchainKind {\n    Onchain,\n}'
assert mutant.count(before) == 1
mutant = mutant.replace(before, 'pub enum OnchainKind {\n    Onchain,\n    Control,\n}')
start = mutant.index("impl<'de> serde::Deserialize<'de> for OnchainKind")
end = mutant.index("// ts: TxSignature", start)
part = mutant[start:end].replace('"Onchain" => Ok(Self::Onchain)', '"Onchain" => Ok(Self::Control)')
part = part.replace('self, enum { Onchain }', 'self, enum { Onchain, Control }')
mutant = mutant[:start] + part + mutant[end:]
expectations.append(("change_pub_key_onchain", "ChangePubKeyOnchain native type"))
start = mutant.index("impl<'de> serde::Deserialize<'de> for ChangePubkeyTypes")
end = mutant.index("impl JsNumberOutput for ChangePubkeyTypes", start)
part = mutant[start:end].replace('"Onchain" => Ok(Self::Onchain)', '"Onchain" => Ok(Self::EthECDSA)')
mutant = mutant[:start] + part + mutant[end:]
expectations.append(("change_pubkey_types", "ChangePubkeyTypes native variant"))
assert len(expectations) == 20
try:
    source.write_text(mutant)
    result = subprocess.run(command, cwd=root, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / "transaction-mutation.log").write_text(output)
    assert result.returncode == 101 and "test result: FAILED." in output, output
    for test, label in expectations:
        assert f"test {test}_native ... FAILED" in output and label in output, (test, label, output)
    print("20 compiled row controls: exit 101, each named decoded native assertion failed", flush=True)
finally:
    source.write_text(original)
result = subprocess.run(command, cwd=root, capture_output=True, text=True)
(logs / "transaction-restored.log").write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stdout + result.stderr
print("restored transaction target: exit 0", flush=True)
