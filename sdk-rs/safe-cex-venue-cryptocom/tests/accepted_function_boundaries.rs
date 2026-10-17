#[test]
fn native_is_authenticated_envelope() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"label":"valid-empty-signature","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"root-other","input":{"root":{"kind":"other"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":[]}},{"label":"root-string","input":{"root":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":[]}},{"label":"root-number","input":{"root":{"kind":"number","bits":"8000000000000000"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":[]}},{"label":"root-array","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":true,"params":false}},"output":{"ok":false,"events":["array:root"]}},{"label":"root-array-error","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_array":"root"},"output":{"error":true,"events":["array:root"]}},{"label":"method-missing","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"other"},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"method-empty","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"method-case","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[80,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"method-suffix-nul","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114,0]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"method-object","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"object","role":"params"},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"method-number","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"number","bits":"40e394a000000000"},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"key-missing","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"other"},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key"]}},{"label":"key-mismatch","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57342]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key"]}},{"label":"key-empty","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key"]}},{"label":"key-object","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"object","role":"params"},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key"]}},{"label":"key-number","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"number","bits":"40e394a000000000"},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key"]}},{"label":"params-missing","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"other"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params"]}},{"label":"params-string","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"string","units":[]},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params"]}},{"label":"params-number","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"number","bits":"0000000000000000"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params"]}},{"label":"params-array","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":true}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params"]}},{"label":"params-array-error","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_array":"params"},"output":{"error":true,"events":["array:root","get:method","get:api_key","get:params","array:params"]}},{"label":"sig-missing","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"other"},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig"]}},{"label":"sig-object","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"object","role":"params"},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig"]}},{"label":"sig-number","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"number","bits":"0000000000000000"},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig"]}},{"label":"sig-lone-surrogate","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[55296,88,57343]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"sig-text","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[110,111,116,45,97,45,118,97,108,105,100,45,115,105,103,110,97,116,117,114,101]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-negative-zero","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"8000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-nan","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"7ff8000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-infinity","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"7ff0000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-negative-infinity","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"fff0000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-missing","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"other"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-string","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"string","units":[48]}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"nonce-object","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"object","role":"params"}},"arrays":{"root":false,"params":false}},"output":{"ok":false,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"throw-method","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_get":"method"},"output":{"error":true,"events":["array:root","get:method"]}},{"label":"throw-api_key","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_get":"api_key"},"output":{"error":true,"events":["array:root","get:method","get:api_key"]}},{"label":"throw-params","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_get":"params"},"output":{"error":true,"events":["array:root","get:method","get:api_key","get:params"]}},{"label":"throw-sig","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_get":"sig"},"output":{"error":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig"]}},{"label":"throw-nonce","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"throw_get":"nonce"},"output":{"error":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"params-root-alias","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"root"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:root","get:sig","get:nonce"]}},{"label":"method-getter-invalidates-key","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"mutate_key":true},"output":{"ok":false,"events":["array:root","get:method","get:api_key"]}},{"label":"array-check-invalidates-method","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false},"mutate_method":true},"output":{"ok":false,"events":["array:root","get:method"]}},{"label":"empty-argument-key","input":{"root":{"kind":"object","role":"root"},"method_arg":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114],"key_arg":[],"fields":{"method":{"kind":"string","units":[112,114,105,118,97,116,101,47,99,114,101,97,116,101,45,111,114,100,101,114]},"api_key":{"kind":"string","units":[]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}},{"label":"empty-argument-method","input":{"root":{"kind":"object","role":"root"},"method_arg":[],"key_arg":[107,55296,57343],"fields":{"method":{"kind":"string","units":[]},"api_key":{"kind":"string","units":[107,55296,57343]},"params":{"kind":"object","role":"params"},"sig":{"kind":"string","units":[]},"nonce":{"kind":"number","bits":"0000000000000000"}},"arrays":{"root":false,"params":false}},"output":{"ok":true,"events":["array:root","get:method","get:api_key","get:params","array:params","get:sig","get:nonce"]}}]"###).unwrap();
    assert_eq!(cases.len(), 44, "envelope independent fixture count");
    let mut executed = 0;
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_venue_cryptocom::api_envelope_body::{
                EnvelopeValue, is_authenticated_envelope,
            };
            use std::{cell::Cell, rc::Rc};
            let root = Rc::new(());
            let params = Rc::new(());
            let error = Rc::new(());
            let convert = |v: &serde_json::Value| -> EnvelopeValue<Rc<()>> {
                match v["kind"].as_str().unwrap() {
                    "other" => EnvelopeValue::Other,
                    "string" => {
                        EnvelopeValue::String(serde_json::from_value(v["units"].clone()).unwrap())
                    }
                    "number" => EnvelopeValue::Number(f64::from_bits(
                        u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
                    )),
                    "object" => EnvelopeValue::Object(if v["role"] == "root" {
                        root.clone()
                    } else {
                        params.clone()
                    }),
                    _ => panic!("unknown independent envelope fixture carrier"),
                }
            };
            let input = convert(&value["root"]);
            let method: Vec<u16> = serde_json::from_value(value["method_arg"].clone()).unwrap();
            let key: Vec<u16> = serde_json::from_value(value["key_arg"].clone()).unwrap();
            let events = std::cell::RefCell::new(vec![]);
            let changed_key = Cell::new(false);
            let changed_method = Cell::new(false);
            let actual = is_authenticated_envelope(
                &input,
                &method,
                &key,
                |handle| {
                    let role = if Rc::ptr_eq(handle, &root) {
                        "root"
                    } else {
                        assert!(
                            Rc::ptr_eq(handle, &params),
                            "envelope native array carrier identity"
                        );
                        "params"
                    };
                    events.borrow_mut().push(format!("array:{role}"));
                    if value["throw_array"] == role {
                        return Err(error.clone());
                    }
                    if role == "root" && value["mutate_method"] == true {
                        changed_method.set(true)
                    }
                    Ok(value["arrays"][role].as_bool().unwrap())
                },
                |handle, field| {
                    assert!(
                        Rc::ptr_eq(handle, &root),
                        "envelope native property receiver identity"
                    );
                    events.borrow_mut().push(format!("get:{field}"));
                    if value["throw_get"] == field {
                        return Err(error.clone());
                    }
                    if field == "method" && value["mutate_key"] == true {
                        changed_key.set(true)
                    }
                    if field == "method" && changed_method.get()
                        || field == "api_key" && changed_key.get()
                    {
                        return Ok(EnvelopeValue::Other);
                    }
                    Ok(convert(&value["fields"][field]))
                },
            );
            match actual {
                Ok(result) => serde_json::json!({"ok":result,"events":events.into_inner()}),
                Err(thrown) => {
                    assert!(
                        Rc::ptr_eq(&thrown, &error),
                        "envelope native original error identity"
                    );
                    serde_json::json!({"error":true,"events":events.into_inner()})
                }
            }
        };
        assert_eq!(
            observed, case["output"],
            "envelope native fields/order/completion case {}",
            case["label"]
        );
        executed += 1;
    }
    assert_eq!(executed, 44, "envelope executed fixture count");
}

#[test]
fn native_map_candlestick_interval() {
    let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":{"key":"1m"},"output":{"kind":"string","value":"1m"}},{"input":{"key":"5m"},"output":{"kind":"string","value":"5m"}},{"input":{"key":"15m"},"output":{"kind":"string","value":"15m"}},{"input":{"key":"30m"},"output":{"kind":"string","value":"30m"}},{"input":{"key":"1h"},"output":{"kind":"string","value":"1h"}},{"input":{"key":"2h"},"output":{"kind":"string","value":"2h"}},{"input":{"key":"4h"},"output":{"kind":"string","value":"4h"}},{"input":{"key":"12h"},"output":{"kind":"string","value":"12h"}},{"input":{"key":"1d"},"output":{"kind":"string","value":"1D"}},{"input":{"key":"1w"},"output":{"kind":"string","value":"7D"}},{"input":{"key":"constructor"},"output":{"kind":"reference","member":"Constructor","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"__defineGetter__"},"output":{"kind":"reference","member":"DefineGetter","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"__defineSetter__"},"output":{"kind":"reference","member":"DefineSetter","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"hasOwnProperty"},"output":{"kind":"reference","member":"HasOwnProperty","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"__lookupGetter__"},"output":{"kind":"reference","member":"LookupGetter","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"__lookupSetter__"},"output":{"kind":"reference","member":"LookupSetter","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"isPrototypeOf"},"output":{"kind":"reference","member":"IsPrototypeOf","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"propertyIsEnumerable"},"output":{"kind":"reference","member":"PropertyIsEnumerable","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"toString"},"output":{"kind":"reference","member":"ToString","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"valueOf"},"output":{"kind":"reference","member":"ValueOf","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":"__proto__"},"output":{"kind":"reference","member":"Prototype","callable":false,"marker_bits":"4022000000000000"}},{"input":{"key":"toLocaleString"},"output":{"kind":"reference","member":"ToLocaleString","callable":true,"marker_bits":"4022000000000000"}},{"input":{"key":""},"output":{"error":"Unsupported Crypto.com candlestick interval: "}},{"input":{"key":"1M"},"output":{"error":"Unsupported Crypto.com candlestick interval: 1M"}},{"input":{"key":"1D"},"output":{"error":"Unsupported Crypto.com candlestick interval: 1D"}},{"input":{"key":"7D"},"output":{"error":"Unsupported Crypto.com candlestick interval: 7D"}},{"input":{"key":"1m\u0000"},"output":{"error":"Unsupported Crypto.com candlestick interval: 1m\u0000"}},{"input":{"key":" 1m"},"output":{"error":"Unsupported Crypto.com candlestick interval:  1m"}},{"input":{"key":"2d"},"output":{"error":"Unsupported Crypto.com candlestick interval: 2d"}},{"input":{"key":"constructor\u0000"},"output":{"error":"Unsupported Crypto.com candlestick interval: constructor\u0000"}},{"input":{"key":"\u00e9"},"output":{"error":"Unsupported Crypto.com candlestick interval: \u00e9"}},{"input":{"key":"acceptedCustom","prototype":{"k":"undefined"}},"output":{"error":"Unsupported Crypto.com candlestick interval: acceptedCustom"}},{"input":{"key":"acceptedCustom","prototype":{"k":"null"}},"output":{"error":"Unsupported Crypto.com candlestick interval: acceptedCustom"}},{"input":{"key":"acceptedCustom","prototype":{"k":"boolean","v":false}},"output":{"error":"Unsupported Crypto.com candlestick interval: acceptedCustom"}},{"input":{"key":"acceptedCustom","prototype":{"k":"number","bits":"8000000000000000"}},"output":{"error":"Unsupported Crypto.com candlestick interval: acceptedCustom"}},{"input":{"key":"acceptedCustom","prototype":{"k":"number","bits":"7ff8000000000000"}},"output":{"error":"Unsupported Crypto.com candlestick interval: acceptedCustom"}},{"input":{"key":"acceptedCustom","prototype":{"k":"string","v":""}},"output":{"error":"Unsupported Crypto.com candlestick interval: acceptedCustom"}},{"input":{"key":"acceptedCustom","prototype":{"k":"number","bits":"401c000000000000"}},"output":{"kind":"number","bits":"401c000000000000"}},{"input":{"key":"acceptedCustom","prototype":{"k":"boolean","v":true}},"output":{"kind":"boolean","value":true}},{"input":{"key":"acceptedCustom","prototype":{"k":"string","v":"X"}},"output":{"kind":"string","value":"X"}}]"###).unwrap();
    assert_eq!(cases.len(), 40, "interval independent fixture count");
    let mut executed = 0;
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_num::{
                ProtoMember,
                prototype_realm::{
                    BuiltinIdentity, PrototypeRealm, PrototypeScalar, PrototypeValue,
                },
            };
            use std::rc::Rc;
            let realm = Rc::new(PrototypeRealm::default());
            let _binding = realm.bind();
            let key = value["key"].as_str().unwrap();
            if let Some(proto) = value.get("prototype") {
                let scalar = match proto["k"].as_str().unwrap() {
                    "undefined" => PrototypeScalar::Undefined,
                    "null" => PrototypeScalar::Null,
                    "boolean" => PrototypeScalar::Boolean(proto["v"].as_bool().unwrap()),
                    "string" => PrototypeScalar::String(proto["v"].as_str().unwrap().into()),
                    "number" => PrototypeScalar::Number(f64::from_bits(
                        u64::from_str_radix(proto["bits"].as_str().unwrap(), 16).unwrap(),
                    )),
                    _ => panic!("unknown independent prototype scalar fixture"),
                };
                realm.reference(ProtoMember::Prototype).set(key, scalar);
            }
            match safe_cex_venue_cryptocom::types::map_candlestick_interval(key) {
                Err(error) => serde_json::json!({"error":error}),
                Ok(PrototypeValue::String(text)) => {
                    serde_json::json!({"kind":"string","value":text})
                }
                Ok(PrototypeValue::Boolean(boolean)) => {
                    serde_json::json!({"kind":"boolean","value":boolean})
                }
                Ok(PrototypeValue::Number(number)) => {
                    serde_json::json!({"kind":"number","bits":format!("{:016x}",number.to_bits())})
                }
                Ok(PrototypeValue::Reference(reference)) => {
                    assert!(
                        Rc::ptr_eq(reference.realm(), &realm),
                        "interval native realm identity"
                    );
                    let BuiltinIdentity::Object(member) = reference.member() else {
                        panic!("interval native object builtin identity")
                    };
                    let name = match member {
                        ProtoMember::Constructor => "Constructor",
                        ProtoMember::DefineGetter => "DefineGetter",
                        ProtoMember::DefineSetter => "DefineSetter",
                        ProtoMember::HasOwnProperty => "HasOwnProperty",
                        ProtoMember::LookupGetter => "LookupGetter",
                        ProtoMember::LookupSetter => "LookupSetter",
                        ProtoMember::IsPrototypeOf => "IsPrototypeOf",
                        ProtoMember::PropertyIsEnumerable => "PropertyIsEnumerable",
                        ProtoMember::ToString => "ToString",
                        ProtoMember::ValueOf => "ValueOf",
                        ProtoMember::Prototype => "Prototype",
                        ProtoMember::ToLocaleString => "ToLocaleString",
                    };
                    reference.set("acceptedMarker", PrototypeScalar::Number(9.0));
                    let Ok(PrototypeValue::Reference(second)) =
                        safe_cex_venue_cryptocom::types::map_candlestick_interval(key)
                    else {
                        panic!("interval native retained reference kind")
                    };
                    assert_eq!(
                        reference, second,
                        "interval native retained reference identity"
                    );
                    let PrototypeValue::Number(marker) = second.get("acceptedMarker") else {
                        panic!("interval native retained marker kind")
                    };
                    assert_eq!(
                        marker.to_bits(),
                        9.0f64.to_bits(),
                        "interval native retained marker bits"
                    );
                    serde_json::json!({"kind":"reference","member":name,"callable":reference.is_callable(),"marker_bits":format!("{:016x}",marker.to_bits())})
                }
                Ok(_) => panic!("interval native unexpected variant"),
            }
        };
        assert_eq!(
            observed, case["output"],
            "interval native variants/identity/bits input {value}"
        );
        executed += 1;
    }
    assert_eq!(executed, 40, "interval executed fixture count");
}

#[test]
fn native_signing_value_clone_construction() {
    use safe_cex_venue_cryptocom::signing_value::SigningValue;
    use serde_json::json;
    fn requires_clone<T: Clone>() {}
    requires_clone::<SigningValue>();
    let original = SigningValue::from_value(json!({"child":{"v":1},"items":[{"v":2}],"text":"Ω"}));
    let cloned = <SigningValue as Clone>::clone(&original);
    assert_eq!(
        original.to_value(),
        json!({"child":{"v":1},"items":[{"v":2}],"text":"Ω"}),
        "signing clone native unchanged original fields"
    );
    assert_eq!(
        cloned.to_value(),
        json!({"child":{"v":1},"items":[{"v":2}],"text":"Ω"}),
        "signing clone native preserved fields"
    );
    assert!(
        original.same_reference(&cloned),
        "signing clone native root identity"
    );
    let child = original.property("child").unwrap();
    let cloned_child = cloned.property("child").unwrap();
    assert!(
        child.same_reference(&cloned_child),
        "signing clone native child identity"
    );
    let items = original.property("items").unwrap();
    let cloned_items = cloned.property("items").unwrap();
    assert!(
        items.same_reference(&cloned_items),
        "signing clone native array identity"
    );
    let element = items.element(0).unwrap();
    assert!(
        element.same_reference(&cloned_items.element(0).unwrap()),
        "signing clone native element identity"
    );
    assert!(child.set_property("v".into(), SigningValue::from_value(json!(7))));
    assert_eq!(
        cloned_child.property("v").unwrap().to_value(),
        json!(7),
        "signing clone native child mutation"
    );
    assert!(cloned_items.set_element(1, SigningValue::from_value(json!(9))));
    assert_eq!(
        items.element(1).unwrap().to_value(),
        json!(9),
        "signing clone native array mutation"
    );
    assert!(cloned.set_property("extra".into(), SigningValue::from_value(json!(false))));
    assert_eq!(
        original.property("extra").unwrap().to_value(),
        json!(false),
        "signing clone native root mutation"
    );
    let scalar_cases = [
        json!(null),
        json!(false),
        json!(-0.0),
        json!(17),
        json!("Ω"),
    ];
    let mut executed = 0;
    for scalar in scalar_cases {
        let value = SigningValue::from_value(scalar.clone());
        let copy = <SigningValue as Clone>::clone(&value);
        assert!(
            !value.same_reference(&copy),
            "signing clone native scalar is value"
        );
        assert_eq!(
            copy.to_value(),
            scalar,
            "signing clone native scalar preservation"
        );
        if scalar.is_number() {
            assert_eq!(
                copy.to_value().as_f64().unwrap().to_bits(),
                scalar.as_f64().unwrap().to_bits(),
                "signing clone native scalar number bits"
            );
        }
        executed += 1;
    }
    assert_eq!(executed, 5, "signing clone native scalar executed cases");
}

#[test]
fn native_public_endpoints_clone_construction() {
    use safe_cex_venue_cryptocom::{public_endpoints::PublicEndpoints, types::PUBLIC_ENDPOINTS};
    fn requires_clone<T: Clone>() {}
    requires_clone::<PublicEndpoints>();
    let original = PUBLIC_ENDPOINTS;
    let saved = original.slots();
    let cloned = <PublicEndpoints as Clone>::clone(&original);
    assert_eq!(
        cloned.slots(),
        saved,
        "endpoint clone native preserved slots"
    );
    assert_eq!(
        original.slots(),
        saved,
        "endpoint clone native unchanged original slots"
    );

    assert!(
        original.same_reference(&cloned),
        "endpoint clone native module identity"
    );
    assert_eq!(saved.len(), 9, "endpoint clone independent initial slots");
    let custom = "/accepted/clone".encode_utf16().collect::<Vec<_>>();
    assert_eq!(
        cloned.push(custom.clone()),
        10,
        "endpoint clone native append length"
    );
    assert_eq!(
        original.slots().last(),
        Some(&Some(custom)),
        "endpoint clone native mutation alias"
    );
    original.delete(9);
    assert_eq!(
        cloned.slots()[9],
        None,
        "endpoint clone native reverse mutation"
    );
    cloned.set(12, vec![0xd800, 0, 0xdfff]);
    assert_eq!(
        original.slots()[12],
        Some(vec![0xd800, 0, 0xdfff]),
        "endpoint clone native UTF16 alias"
    );
    assert_eq!(
        original.slots()[10..12],
        [None, None],
        "endpoint clone native sparse slots"
    );
    cloned.replace_contents(saved.clone());
    assert_eq!(original.slots(), saved, "endpoint clone native restoration");
}
