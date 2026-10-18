# Validation error categories

The specific-claims assignment's `AsterDEXExchange.validateAccount` row described
an AxiosError/name loss. The constructed production binding reveals two narrower
remaining defects after the shared retained-cause repair:

| Real source/native operation | Defect | Repair |
| --- | --- | --- |
| validateAccount, signed HTTP401 | TS replaces the Axios response rejection with plain Error; native forced isAxiosError=true afterwards. | Preserve the existing retained error marker in OrderExecution::request. |
| fetchBalance, signed HTTP401 | The identical overwrite remained in AccountFetch::request. | Preserve the same canonical marker. |
| validateAccount, unsupported://fixture | TS retains AxiosError/ERR_BAD_REQUEST and its enhanced request-setup message; native misclassified the reqwest builder failure as ECONNRESET. | Shared native transport recognizes protocols Axios rejects and returns the source setup diagnostic. |

`native_error_tests` uses NativeRuntime, reqwest, canonical Store and the actual
constructed member dispatch. The HTTP receiver checks the account path, actual
signature query and API-key header before responding with 401. The independent
Node process constructs the maintained Aster class and sends through real Axios
HTTP. No adapter/error stubs are installed in these new witnesses. Clock sync
uses the existing production clock; this is not a clock-isolation test.
The unsupported protocol exercises the real HTTP adapter's request admission.
Both paths require the exact name, code, Axios marker, enhanced message and
nested getAccount/validation or fetchBalance logs.

The venue-wide setter census found no other post-interceptor overwrite:
Backpack native_http, Bitunix native_http and KuCoin binding/http create the
initial Axios transport error before venue interceptors, matching source Axios.
Those constructors retain their existing behavior. The legacy order-execution
oracle fixture now supplies its declared Axios error through the canonical owned
cause and requires equal names instead of asserting the obsolete Error loss.

Original Error object identity remains the explicitly accepted owned-snapshot
boundary. No diagnostic difference is accepted by that identity ruling.

## Prove-fail receipts

On master e5267d38fa plus tests, both validation regressions compiled and failed:
HTTP401 at the Axios-marker assertion (true versus false), unsupported protocol
at the code assertion (ECONNRESET versus ERR_BAD_REQUEST). After the shared
protocol and OrderExecution fixes, both passed. The separate fetchBalance control
then failed at the same Axios-marker assertion while AccountFetch was unchanged;
removing its overwrite passed. All five native_error_tests, including the existing
retained nested-error controls, pass together. Source expectations are unchanged
between RED and GREEN. The two no-network protocol inputs also match actual
Axios via node -e, including uppercase FTP scheme normalization.

Logs: /tmp/aster-validation-master-red.log, /tmp/aster-balance-master-red.log and
/tmp/aster-validation-native-green.log.
