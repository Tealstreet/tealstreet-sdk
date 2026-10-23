//! Engine-owned credential choice. A venue declares what a request is for; the
//! selection fixes slot, egress and evidence scope before signing. HTTP method
//! is never an input. See docs/plans/RUST_CREDENTIAL_SELECTION.md.
use crate::request_context::{Credential, PrivateEgress, RoutingIntent};
use crate::signing::KeyRef;
use crate::{AccountReadScope, CredentialSlot, Cx, RequestId, RequestRoute, SignStep};

/// Why a read must stay on the primary account's key and egress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryReason {
    /// Placement identity belongs to the trading account.
    ClientOrderLookup,
    /// Reconciliation after an unknown write outcome.
    PostUnknownOrderRead,
    /// The venue reported the read key degraded for this endpoint.
    DegradedRead,
    Catalog,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialRole {
    /// Account data any configured read key may serve.
    Read {
        scope: Option<AccountReadScope>,
    },
    PrimaryRead {
        reason: PrimaryReason,
        scope: Option<AccountReadScope>,
    },
    Mutation,
    /// REST login, listen keys and private socket authentication.
    SessionAuth,
}

/// Construction-time family data reproducing each venue's existing gates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotPolicy {
    /// The read key serves reads only while a primary key is configured.
    pub secondary_requires_primary: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialUnavailable {
    /// No usable key for this role; the venue keeps its existing presentation
    /// (silent skip, typed failure or an unsigned public request).
    Missing,
}

/// One immutable choice, used for signing, routing and account-auth evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "sign with `Selection::sign` so egress and evidence follow the chosen key"]
pub struct Selection {
    slot: CredentialSlot,
    /// Primary-only reads always declare primary egress.
    pinned_primary: bool,
    secondary_supplied: bool,
    evidence: Option<AccountReadScope>,
    session_auth: bool,
}

impl Selection {
    /// Primary egress for a keyless session whose unsigned requests carry
    /// this key's login or token (`SessionPolicy.private_egress`).
    pub fn session_egress(&self) -> Option<PrivateEgress> {
        (self.session_auth && self.slot == CredentialSlot::Primary).then(PrivateEgress::granted)
    }

    pub fn slot(&self) -> CredentialSlot {
        self.slot
    }

    pub fn intent(&self) -> RoutingIntent {
        match self.slot {
            CredentialSlot::Primary => RoutingIntent::Primary,
            CredentialSlot::Secondary => RoutingIntent::Secondary,
        }
    }

    /// Explicit egress for a non-managed request. Other primary requests
    /// without a read key keep the host default, already the primary egress.
    pub fn route(&self) -> Option<RequestRoute> {
        match self.slot {
            CredentialSlot::Secondary => Some(RequestRoute::Secondary),
            CredentialSlot::Primary => {
                (self.pinned_primary || self.secondary_supplied).then_some(RequestRoute::Primary)
            }
        }
    }

    /// Request-context credential for managed HTTP.
    pub fn credential(&self, reference: KeyRef) -> Credential {
        Credential {
            slot: self.slot,
            reference,
        }
    }

    /// Apply egress and evidence to a request issued on the non-managed path.
    pub fn bind<P>(&self, cx: &mut Cx<'_, P>, request: RequestId) {
        if let Some(route) = self.route() {
            cx.route_request(request, route);
        }
        if let Some(scope) = self.evidence {
            cx.tag_account_read(request, self.slot, scope);
        }
    }

    /// Sign a non-managed request with the selected key and bind its egress
    /// and evidence in one step.
    pub fn sign<P>(&self, cx: &mut Cx<'_, P>, request: RequestId, step: Box<dyn SignStep>) {
        cx.sign_http(request, step);
        self.bind(cx, request);
    }
}

/// A venue's signers per slot. The venue indexes it only through a selection.
#[derive(Clone)]
pub struct Slots<T> {
    primary: Option<T>,
    secondary: Option<T>,
    policy: SlotPolicy,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self::new(None, None, SlotPolicy::default())
    }
}

impl<T> Slots<T> {
    pub fn new(primary: Option<T>, secondary: Option<T>, policy: SlotPolicy) -> Self {
        Self {
            primary,
            secondary,
            policy,
        }
    }

    /// Signing material is reachable only through `select`.
    pub fn has_primary(&self) -> bool {
        self.primary.is_some()
    }

    /// A read key was supplied at construction.
    pub fn secondary_supplied(&self) -> bool {
        self.secondary.is_some()
    }

    pub fn select(&self, role: CredentialRole) -> Result<(Selection, &T), CredentialUnavailable> {
        let (slot, evidence) = match role {
            CredentialRole::Read { scope } if self.secondary_serves_reads() => {
                (CredentialSlot::Secondary, scope)
            }
            CredentialRole::Read { scope } | CredentialRole::PrimaryRead { scope, .. } => {
                (CredentialSlot::Primary, scope)
            }
            CredentialRole::Mutation | CredentialRole::SessionAuth => {
                (CredentialSlot::Primary, None)
            }
        };
        let material = match slot {
            CredentialSlot::Primary => self.primary.as_ref(),
            CredentialSlot::Secondary => self.secondary.as_ref(),
        }
        .ok_or(CredentialUnavailable::Missing)?;
        let selection = Selection {
            slot,
            pinned_primary: matches!(role, CredentialRole::PrimaryRead { .. }),
            secondary_supplied: self.secondary_supplied(),
            evidence,
            session_auth: role == CredentialRole::SessionAuth,
        };
        Ok((selection, material))
    }

    fn secondary_serves_reads(&self) -> bool {
        self.secondary.is_some()
            && (!self.policy.secondary_requires_primary || self.primary.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use CredentialRole::*;

    const BALANCE: Option<AccountReadScope> = Some(AccountReadScope::Balance);

    fn slots(primary: bool, secondary: bool, policy: SlotPolicy) -> Slots<&'static str> {
        Slots::new(
            primary.then_some("primary"),
            secondary.then_some("secondary"),
            policy,
        )
    }

    fn chosen(
        slots: &Slots<&'static str>,
        role: CredentialRole,
    ) -> Option<(&'static str, Option<RequestRoute>)> {
        slots
            .select(role)
            .ok()
            .map(|(selection, material)| (*material, selection.route()))
    }

    #[test]
    fn reads_use_the_read_key_and_its_egress_while_writes_and_sessions_stay_primary() {
        let both = slots(true, true, SlotPolicy::default());
        let primary = Some(RequestRoute::Primary);
        assert_eq!(
            chosen(&both, Read { scope: BALANCE }),
            Some(("secondary", Some(RequestRoute::Secondary)))
        );
        assert_eq!(chosen(&both, Mutation), Some(("primary", primary)));
        assert_eq!(chosen(&both, SessionAuth), Some(("primary", primary)));
        for reason in [
            PrimaryReason::ClientOrderLookup,
            PrimaryReason::PostUnknownOrderRead,
            PrimaryReason::DegradedRead,
            PrimaryReason::Catalog,
            PrimaryReason::History,
        ] {
            let role = PrimaryRead {
                reason,
                scope: None,
            };
            assert_eq!(chosen(&both, role), Some(("primary", primary)));
        }
    }

    #[test]
    fn without_a_read_key_only_primary_only_reads_declare_an_egress() {
        let only = slots(true, false, SlotPolicy::default());
        assert_eq!(chosen(&only, Read { scope: None }), Some(("primary", None)));
        assert_eq!(chosen(&only, Mutation), Some(("primary", None)));
        let lookup = PrimaryRead {
            reason: PrimaryReason::ClientOrderLookup,
            scope: None,
        };
        assert_eq!(
            chosen(&only, lookup),
            Some(("primary", Some(RequestRoute::Primary)))
        );
    }

    #[test]
    fn a_read_key_alone_serves_reads_unless_the_family_requires_a_primary() {
        let lone = slots(false, true, SlotPolicy::default());
        assert_eq!(chosen(&lone, Read { scope: None }).unwrap().0, "secondary");
        assert_eq!(
            lone.select(Mutation).err(),
            Some(CredentialUnavailable::Missing)
        );
        // BloFin, Bitget, WOO, Bitrue and KuCoin gate the read key on a primary.
        let gated = SlotPolicy {
            secondary_requires_primary: true,
        };
        let lone = slots(false, true, gated);
        assert_eq!(
            lone.select(Read { scope: None }).err(),
            Some(CredentialUnavailable::Missing)
        );
        assert_eq!(
            chosen(&slots(true, true, gated), Read { scope: None })
                .unwrap()
                .0,
            "secondary"
        );
        assert!(slots(false, false, gated).select(SessionAuth).is_err());
    }

    #[test]
    fn a_dynamically_degraded_read_switches_role_not_method() {
        // Bitget's degraded balance read: same GET, declared primary-only.
        let both = slots(true, true, SlotPolicy::default());
        let degraded = PrimaryRead {
            reason: PrimaryReason::DegradedRead,
            scope: BALANCE,
        };
        let (selection, material) = both.select(degraded).unwrap();
        assert_eq!(
            (*material, selection.intent()),
            ("primary", RoutingIntent::Primary)
        );
        assert_eq!(selection.evidence, BALANCE);
    }

    #[test]
    fn selection_feeds_managed_context_intent_and_scoped_evidence_only() {
        let both = slots(true, true, SlotPolicy::default());
        let (read, _) = both.select(Read { scope: BALANCE }).unwrap();
        let credential = read.credential(KeyRef("secondary.credential".into()));
        assert_eq!(credential.slot, CredentialSlot::Secondary);
        assert_eq!(read.intent(), RoutingIntent::Secondary);
        assert_eq!(read.evidence, BALANCE);
        let (unscoped, _) = both.select(Read { scope: None }).unwrap();
        assert_eq!(unscoped.evidence, None);
    }
}
