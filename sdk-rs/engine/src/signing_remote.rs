//! Versioned remote-signing recipes (P8). A recipe turns non-secret inputs into
//! the exact bytes one `Sign` operation covers, so a remote signer can rebuild
//! and check the preimage instead of signing opaque bytes. Inputs are JSON text;
//! each recipe parses them into its own typed form.
use crate::signing::{Algorithm, ExtraKey, Operation, PayloadPart, SignError, SignRequest};
use serde_json::value::RawValue;

/// Serialized `inputs` limit, separate from the 1 MiB payload bound.
pub const MAX_REMOTE_INPUT_BYTES: usize = 64 * 1024;
/// A remote executor signs only while the recipe's own stamp is this fresh
/// against its protocol clock, and never more than `MAX_REMOTE_LEAD_MS` ahead,
/// so a server cannot pre-mint signatures for later use.
pub const REMOTE_FRESHNESS_MS: f64 = 5000.;
pub const MAX_REMOTE_LEAD_MS: f64 = 2000.;

/// One production canonicalizer, shared by the server step, the server guard and
/// the browser executor. `canonicalize` is also the allowlist: any input it does
/// not accept is `SignError::Refused`.
pub trait RemoteRecipe: Sync {
    /// Versioned id, e.g. `paradex.order.1`; a format change is a new id.
    fn id(&self) -> &'static str;
    fn algorithm(&self) -> Algorithm;
    /// `None` signs with the identity's ordinary signing ref.
    fn role(&self) -> Option<ExtraKey>;
    fn canonicalize(&self, inputs: &str) -> Result<Vec<u8>, SignError>;
    /// The protocol time (ms) the signed bytes carry; every recipe has one so
    /// the executor can refuse stale or future-dated work.
    fn stamp_ms(&self, inputs: &str) -> Result<f64, SignError>;
    /// An input value that must equal the executing account's own presented
    /// credential (e.g. a JWT `kid`), so a recipe cannot sign for another key.
    fn bound_credential(&self, _inputs: &str) -> Result<Option<String>, SignError> {
        Ok(None)
    }
}

/// Parses recipe inputs into the recipe's typed form; any shape it does not
/// declare (unknown fields included, via `deny_unknown_fields`) is refused.
pub fn parse_inputs<'a, T: serde::Deserialize<'a>>(inputs: &'a str) -> Result<T, SignError> {
    if inputs.len() > MAX_REMOTE_INPUT_BYTES {
        return Err(SignError::InvalidRequest);
    }
    serde_json::from_str(inputs).map_err(|_| SignError::Refused)
}

/// Checks a signature over the bound payload against the account's declared
/// public key. Venues build it from non-secret config; the engine has no crypto.
pub type RemoteVerifier = Box<dyn Fn(&[u8], &[u8]) -> bool + Send>;

/// The step-side remote description of one prepared `SignRequest`.
pub struct RemoteOp {
    pub recipe: &'static dyn RemoteRecipe,
    /// JSON text the recipe canonicalizes.
    pub inputs: String,
    /// When present, every result is verified before release; families with no
    /// verification key (Coinbase, Lighter) rely on venue rejection instead.
    pub verify: Option<RemoteVerifier>,
}
impl RemoteOp {
    pub fn new(recipe: &'static dyn RemoteRecipe, inputs: String) -> Self {
        Self {
            recipe,
            inputs,
            verify: None,
        }
    }
    pub fn verified_by(mut self, verify: RemoteVerifier) -> Self {
        self.verify = Some(verify);
        self
    }
}

/// What crosses to a host provider beside the unchanged `SignRequest`.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWire {
    pub recipe: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ExtraKey>,
    pub inputs: Box<RawValue>,
}
impl PartialEq for RemoteWire {
    fn eq(&self, other: &Self) -> bool {
        self.recipe == other.recipe
            && self.role == other.role
            && self.inputs.get() == other.inputs.get()
    }
}

impl RemoteOp {
    /// Binds the description to the actual request: exactly one `Sign` of the
    /// recipe's algorithm whose literal bytes are the recipe's canonical bytes,
    /// beside `Present` operations only. Anything else never leaves.
    pub fn bind(&self, request: &SignRequest) -> Result<RemoteWire, SignError> {
        if self.inputs.len() > MAX_REMOTE_INPUT_BYTES {
            return Err(SignError::InvalidRequest);
        }
        let inputs =
            RawValue::from_string(self.inputs.clone()).map_err(|_| SignError::InvalidRequest)?;
        let mut signed = None;
        for operation in &request.operations {
            match operation {
                Operation::Present { .. } => {}
                Operation::Sign {
                    algorithm, payload, ..
                } if signed.is_none() && *algorithm == self.recipe.algorithm() => {
                    signed = Some(payload);
                }
                _ => return Err(SignError::InvalidRequest),
            }
        }
        let payload = signed.ok_or(SignError::InvalidRequest)?;
        let mut bytes = Vec::new();
        for part in payload {
            match part {
                PayloadPart::Bytes { bytes: part } => bytes.extend_from_slice(part),
                PayloadPart::Credential { .. } | PayloadPart::Result { .. } => {
                    return Err(SignError::InvalidRequest);
                }
            }
        }
        if self.recipe.canonicalize(&self.inputs)? != bytes {
            return Err(SignError::InvalidRequest);
        }
        Ok(RemoteWire {
            recipe: self.recipe.id(),
            role: self.recipe.role(),
            inputs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::{Context, KeyRef};

    /// Canonical bytes are `order:<qty>`; quantities over 100 are refused.
    struct Order;
    impl RemoteRecipe for Order {
        fn id(&self) -> &'static str {
            "test.order.1"
        }
        fn algorithm(&self) -> Algorithm {
            Algorithm::Ed25519
        }
        fn role(&self) -> Option<ExtraKey> {
            None
        }
        fn canonicalize(&self, inputs: &str) -> Result<Vec<u8>, SignError> {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Inputs {
                qty: u64,
            }
            let Inputs { qty } = parse_inputs(inputs)?;
            if qty > 100 {
                return Err(SignError::Refused);
            }
            Ok(format!("order:{qty}").into_bytes())
        }
        fn stamp_ms(&self, _: &str) -> Result<f64, SignError> {
            Ok(0.)
        }
    }
    static ORDER: Order = Order;

    fn op(qty: u64) -> RemoteOp {
        RemoteOp::new(&ORDER, format!(r#"{{"qty":{qty}}}"#))
    }
    fn sign(algorithm: Algorithm, payload: Vec<PayloadPart>) -> Operation {
        Operation::Sign {
            algorithm,
            key: KeyRef("primary.signing".into()),
            payload,
        }
    }
    fn bytes(text: &str) -> PayloadPart {
        PayloadPart::Bytes {
            bytes: text.as_bytes().to_vec(),
        }
    }
    fn request(operations: Vec<Operation>) -> SignRequest {
        SignRequest {
            receipt: Default::default(),
            operations,
            context: Context::default(),
        }
    }

    #[test]
    fn binds_one_matching_sign_beside_presentations() {
        let work = request(vec![
            Operation::Present {
                key: KeyRef("primary.credential".into()),
            },
            sign(Algorithm::Ed25519, vec![bytes("order:"), bytes("7")]),
        ]);
        assert_eq!(
            op(7).bind(&work),
            Ok(RemoteWire {
                recipe: "test.order.1",
                role: None,
                inputs: RawValue::from_string(r#"{"qty":7}"#.into()).unwrap(),
            })
        );
    }

    #[test]
    fn refuses_anything_the_recipe_does_not_describe_exactly() {
        let matching = || sign(Algorithm::Ed25519, vec![bytes("order:7")]);
        for (operations, expected) in [
            (
                vec![sign(Algorithm::Ed25519, vec![bytes("order:8")])],
                SignError::InvalidRequest,
            ),
            (
                vec![sign(Algorithm::HmacSha256, vec![bytes("order:7")])],
                SignError::InvalidRequest,
            ),
            (vec![matching(), matching()], SignError::InvalidRequest),
            (vec![], SignError::InvalidRequest),
            (
                vec![
                    Operation::Sha256 {
                        payload: vec![bytes("x")],
                    },
                    matching(),
                ],
                SignError::InvalidRequest,
            ),
            (
                vec![sign(
                    Algorithm::Ed25519,
                    vec![
                        bytes("order:"),
                        PayloadPart::Credential {
                            key: KeyRef("primary.credential".into()),
                        },
                    ],
                )],
                SignError::InvalidRequest,
            ),
        ] {
            assert_eq!(op(7).bind(&request(operations)), Err(expected));
        }
        // The recipe's own allowlist refusal is a designed refusal, not a bug.
        let over = request(vec![sign(Algorithm::Ed25519, vec![bytes("order:101")])]);
        assert_eq!(op(101).bind(&over), Err(SignError::Refused));
        let huge = RemoteOp::new(
            &ORDER,
            format!(
                r#"{{"qty":7,"pad":"{}"}}"#,
                "x".repeat(MAX_REMOTE_INPUT_BYTES)
            ),
        );
        // Unknown fields are refused by the typed parse, not ignored.
        let extra = request(vec![matching()]);
        assert_eq!(
            RemoteOp::new(&ORDER, r#"{"qty":7,"to":"x"}"#.into()).bind(&extra),
            Err(SignError::Refused)
        );
        assert_eq!(
            huge.bind(&request(vec![matching()])),
            Err(SignError::InvalidRequest)
        );
    }

    #[test]
    fn wire_omits_an_absent_role() {
        let wire = op(7)
            .bind(&request(vec![sign(
                Algorithm::Ed25519,
                vec![bytes("order:7")],
            )]))
            .unwrap();
        assert_eq!(
            serde_json::to_string(&wire).unwrap(),
            r#"{"recipe":"test.order.1","inputs":{"qty":7}}"#
        );
    }
}
