# Sessions: Frictionless Recovery

## 2026-04-21 — plan drafted
- Session focus: reinvent the Shamir/steward flow around the user's "frictionless" vision.
- Deep-reasoned the architectural shift: logical account + pluggable devices, AccountRoot + DeviceCertificates, stewards-as-human-verifiers, erasure-coded personal data.
- Three-plan arc:
  - **Plan A**: invitation/release UX over DM realms. Keep current crypto (pass-story subkey). Deliver the visible UX win first.
  - **Plan B**: AccountRoot + DeviceCertificates. Replace subkey as the Shamir secret; pass-story becomes optional.
  - **Plan C**: Reed-Solomon personal-data backup + cross-device re-hydration.
- Slice-level decomposition written into `./plans/frictionless-recovery.md` with code map, verification strategy, out-of-scope, open questions.
- Predecessor plan (`steward-backup-completion.md`) is complete upstream (commits `a84131e…45972d56`); this plan rewrites the UX parts end-to-end and leaves the crypto primitives (`shamir.rs`, `steward_share.rs`) in place.
- Next: Slice A.1 — draft the invitation/response CRDT module with unit tests.
