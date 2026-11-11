use super::*;
use crate::delegated::probe::ProbeRefusal;
use safe_cex_remote_signer::SignerLink;

pub(super) struct Completion {
    pub(super) init: InitPayload,
    pub(super) proof: Result<crate::accounts::ProbePermit, ProbeRefusal>,
}

impl Session {
    pub(super) fn init_cosigned_test_admission(&mut self, init: InitPayload) {
        if !crate::delegated::enabled(&init.exchange_name)
            || init.sealed.is_some()
            || init.public_only == Some(true)
            || init.account_name.starts_with("__pub_")
            || self.pending_verifications.len() >= 64
        {
            self.refuse_handover(&init.account_name, false);
            return;
        }
        let user = self.user().to_owned();
        let socket = self.socket;
        let permit = self.registry.probe_permit(&user, &init);
        let out = self.handover_out(&init.account_name);
        let link = SignerLink::new(socket, out.outbox());
        let pending_link = link.clone();
        let registry = self.registry.clone();
        let token = self.identity.token.clone();
        self.next_verification += 1;
        let generation = self.next_verification;
        let account_name = init.account_name.clone();
        let pending_name = account_name.clone();
        let completed = self.verified.clone();
        let task = tokio::spawn(async move {
            let proof = match permit {
                Ok(permit) => {
                    registry
                        .probe_cosigned_test(&user, &init, token, link, permit)
                        .await
                }
                Err(refusal) => Err(refusal),
            };
            let _ = completed.send(JoinCompletion {
                user,
                socket,
                generation,
                account_name,
                candidate: Err(crate::delegated::Refused),
                probe: None,
                cosigned: Some(Completion { init, proof }),
            });
        });
        self.pending_verifications.insert(
            pending_name,
            PendingVerification {
                generation,
                task,
                out,
                cosigned_link: Some(pending_link),
            },
        );
    }

    pub(super) async fn complete_cosigned_test_admission(
        &mut self,
        completed: Completion,
        out: Out,
    ) -> Result<(), Close> {
        let name = completed.init.account_name.clone();
        let account = match completed.proof {
            Ok(proof) => {
                self.registry
                    .obtain_probed_cosigned_test(
                        self.user(),
                        &completed.init,
                        self.socket,
                        self.identity.token.clone(),
                        out.outbox(),
                        proof,
                    )
                    .await
            }
            Err(refusal) => Err(refusal),
        };
        match account {
            Ok(account) => {
                self.metrics.note_init();
                self.join(account, out.clone()).await;
                if self.joined.contains_key(&name) {
                    self.registry
                        .interruptions
                        .deliver(self.user(), &name, &out);
                    if let Some(kept) = self.registry.take_notice(
                        self.user(),
                        &name,
                        &self.joined[&name].account.fingerprint,
                        self.identity.resume_from.as_deref(),
                    ) {
                        for frame in kept.frames(&name) {
                            out.send(frame);
                        }
                    }
                }
            }
            Err(refusal) => {
                self.drop_early(&name, "verification refused");
                out.send(envelope::init_unverified(&name, refusal.transient));
            }
        }
        Ok(())
    }
}
