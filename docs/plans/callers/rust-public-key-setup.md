# Engine PublicKey setup

**Change.** Engine address setup (`address_nonce_setup.rs`) gains a PublicKey
sibling for host-answered (external) providers.
- A venue declares `Venue::public_key_setup() -> Option<(AddressSetupSlots,
  Algorithm)>` in place of `public_address_slots`. Declaring both, or an
  algorithm with no public key, refuses `initialize_address_nonce`.
- Each declared slot gets one `[PublicKey {algorithm, key}]` setup task, with
  the same `Action::Sign`, deadline and `RetireAddressSetup` join as
  PublicAddress.
- A result binds only at the algorithm's length (32 for StarkDigest and
  Ed25519, 40 for SchnorrDigest), and only after retirement.
- The venue reads it through `cx.setup_public_key(slot, algorithm)` or
  `Identity::verified_setup_public_key(algorithm)`.
- The sign-meta setup arm adds `{"operation":"publicKey","algorithm":"<name>"}`.
  PublicAddress is unchanged.

**Callers.**
- SDK hosts: the existing `publicAddressOwned` negotiation (#2362
  `prepareAddressSetup`) answers these tasks unchanged. The provider receives
  a one-operation PublicKey SignRequest.
- Venues: Paradex (StarkDigest) and Lighter (SchnorrDigest) adopt it for
  keyless accounts. Local providers keep their sealed key
  (`verified_public_key`).
- NX: none.

**Obligation.** A host that negotiates `publicAddressOwned` must answer a
PublicKey setup task as it does a PublicAddress one.

**Deletion prerequisites.** None.
