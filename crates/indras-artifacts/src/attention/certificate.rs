//! Quorum certificates for Byzantine fault tolerance.
//!
//! Witnesses co-sign attention events to produce quorum certificates,
//! creating a two-tier finality model (observed vs. final). Only
//! certified events are treated as final — this prevents equivocating
//! authors from causing damage.
//!
//! # Key types
//!
//! - [`WitnessSignature`]: A single witness's PQ signature over an event.
//! - [`QuorumCertificate`]: A collection of witness signatures forming a quorum.
//! - [`CertificateError`]: Errors during certificate validation.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::artifact::{ArtifactId, PlayerId};

/// A single witness's PQ signature over (event_hash || intention_scope).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessSignature {
    /// The witness who produced this signature.
    pub witness: PlayerId,
    /// PQ signature bytes over `signable_bytes(event_hash, intention_scope)`.
    pub sig: Vec<u8>,
}

impl WitnessSignature {
    /// Create a witness signature by signing the canonical bytes.
    pub fn sign(
        event_hash: &[u8; 32],
        intention_scope: &ArtifactId,
        identity: &indras_crypto::PQIdentity,
        witness: PlayerId,
    ) -> Self {
        let msg = signable_bytes(event_hash, intention_scope);
        let signature = identity.sign(&msg);
        Self {
            witness,
            sig: signature.to_bytes().to_vec(),
        }
    }

    /// Verify this signature against a public key.
    pub fn verify(
        &self,
        event_hash: &[u8; 32],
        intention_scope: &ArtifactId,
        public_key: &indras_crypto::PQPublicIdentity,
    ) -> bool {
        let msg = signable_bytes(event_hash, intention_scope);
        let Ok(signature) = indras_crypto::PQSignature::from_bytes(self.sig.clone()) else {
            return false;
        };
        public_key.verify(&msg, &signature)
    }
}

/// A quorum certificate: a collection of witness signatures over an event.
///
/// A certificate is valid when it contains at least `k` valid signatures
/// from witnesses in the roster, where `k = floor(|roster|/2) + 1`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuorumCertificate {
    /// Protocol version (currently 1).
    pub version: u16,
    /// BLAKE3 hash of the event being certified.
    pub event_hash: [u8; 32],
    /// The intention (artifact) scope this certificate covers.
    pub intention_scope: ArtifactId,
    /// Witness signatures forming the quorum.
    pub witnesses: Vec<WitnessSignature>,
}

impl QuorumCertificate {
    /// Create a new empty certificate for an event.
    pub fn new(event_hash: [u8; 32], intention_scope: ArtifactId) -> Self {
        Self {
            version: 1,
            event_hash,
            intention_scope,
            witnesses: Vec::new(),
        }
    }

    /// Add a witness signature to this certificate.
    pub fn add_witness(&mut self, sig: WitnessSignature) {
        // Deduplicate by witness identity
        if !self.witnesses.iter().any(|w| w.witness == sig.witness) {
            self.witnesses.push(sig);
        }
    }

    /// Verify that this certificate has a valid quorum.
    ///
    /// Checks that at least `k` signatures come from eligible witnesses
    /// in the roster and that each signature verifies against the
    /// corresponding public key.
    pub fn verify(
        &self,
        roster: &[PlayerId],
        k: usize,
        public_keys: &HashMap<PlayerId, indras_crypto::PQPublicIdentity>,
    ) -> Result<(), CertificateError> {
        validate_certificate(self, roster, k, public_keys)
    }
}

/// Canonical bytes that witnesses sign: `postcard(event_hash || intention_scope)`.
pub fn signable_bytes(event_hash: &[u8; 32], intention_scope: &ArtifactId) -> Vec<u8> {
    #[derive(Serialize)]
    struct SignableFields<'a> {
        event_hash: &'a [u8; 32],
        intention_scope: &'a ArtifactId,
    }
    let fields = SignableFields {
        event_hash,
        intention_scope,
    };
    postcard::to_allocvec(&fields).expect("serialization cannot fail for fixed-schema types")
}

/// Validate a quorum certificate against a roster and public keys.
///
/// The quorum threshold `k` is computed via BFT: `k = n - f` where
/// `f = floor((n-1)/3)`, ensuring two quorums overlap by at least `f+1` nodes.
///
/// Checks:
/// 1. Certificate has >= `k` signatures (BFT threshold).
/// 2. Each signer is in the roster.
/// 3. Each PQ signature verifies against `signable_bytes(event_hash, intention_scope)`.
pub fn validate_certificate(
    cert: &QuorumCertificate,
    roster: &[PlayerId],
    k: usize,
    public_keys: &HashMap<PlayerId, indras_crypto::PQPublicIdentity>,
) -> Result<(), CertificateError> {
    // Enforce that k matches the BFT threshold for this roster.
    let (_f, expected_k) = super::witness::bft_quorum_threshold(roster.len());
    assert!(
        k == expected_k,
        "quorum threshold k={k} does not match expected BFT threshold k={expected_k} for roster.len()={}",
        roster.len(),
    );

    if cert.witnesses.len() < k {
        return Err(CertificateError::InsufficientSignatures {
            have: cert.witnesses.len(),
            need: k,
        });
    }

    // Reject certificates with duplicate witness IDs (prevents count inflation)
    let mut seen = HashSet::new();
    for ws in &cert.witnesses {
        if !seen.insert(ws.witness) {
            return Err(CertificateError::DuplicateWitness {
                witness: ws.witness,
            });
        }
    }

    for ws in &cert.witnesses {
        // Check signer is in roster
        if !roster.contains(&ws.witness) {
            return Err(CertificateError::SignerNotInRoster {
                signer: ws.witness,
            });
        }

        // Look up public key
        let Some(pubkey) = public_keys.get(&ws.witness) else {
            return Err(CertificateError::MissingPublicKey {
                witness: ws.witness,
            });
        };

        // Verify signature
        if !ws.verify(&cert.event_hash, &cert.intention_scope, pubkey) {
            return Err(CertificateError::InvalidSignature {
                witness: ws.witness,
            });
        }
    }

    Ok(())
}

/// Errors during certificate validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateError {
    /// Not enough valid signatures to form a quorum.
    InsufficientSignatures { have: usize, need: usize },
    /// A signer is not in the witness roster.
    SignerNotInRoster { signer: PlayerId },
    /// Public key not found for a witness.
    MissingPublicKey { witness: PlayerId },
    /// A witness signature failed verification.
    InvalidSignature { witness: PlayerId },
    /// A witness appears more than once in the certificate.
    DuplicateWitness { witness: PlayerId },
}

impl std::fmt::Display for CertificateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CertificateError::InsufficientSignatures { have, need } => {
                write!(f, "insufficient signatures: have {have}, need {need}")
            }
            CertificateError::SignerNotInRoster { .. } => {
                write!(f, "signer not in witness roster")
            }
            CertificateError::MissingPublicKey { .. } => {
                write!(f, "missing public key for witness")
            }
            CertificateError::InvalidSignature { .. } => {
                write!(f, "invalid witness signature")
            }
            CertificateError::DuplicateWitness { .. } => {
                write!(f, "duplicate witness in certificate")
            }
        }
    }
}

impl std::error::Error for CertificateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_crypto::PQIdentity;

    fn test_player(n: u8) -> PlayerId {
        [n; 32]
    }

    fn test_artifact() -> ArtifactId {
        ArtifactId::Doc([0xAA; 32])
    }

    #[test]
    fn test_signable_bytes_deterministic() {
        let hash = [0x11; 32];
        let scope = test_artifact();
        let a = signable_bytes(&hash, &scope);
        let b = signable_bytes(&hash, &scope);
        assert_eq!(a, b);
    }

    #[test]
    fn test_signable_bytes_differ_on_different_input() {
        let scope = test_artifact();
        let a = signable_bytes(&[0x11; 32], &scope);
        let b = signable_bytes(&[0x22; 32], &scope);
        assert_ne!(a, b);
    }

    #[test]
    fn test_witness_signature_sign_verify() {
        let identity = PQIdentity::generate();
        let witness = test_player(1);
        let event_hash = [0xBB; 32];
        let scope = test_artifact();

        let ws = WitnessSignature::sign(&event_hash, &scope, &identity, witness);
        assert_eq!(ws.witness, witness);
        assert!(ws.verify(&event_hash, &scope, &identity.verifying_key()));
    }

    #[test]
    fn test_witness_signature_wrong_key_fails() {
        let identity1 = PQIdentity::generate();
        let identity2 = PQIdentity::generate();
        let witness = test_player(1);
        let event_hash = [0xBB; 32];
        let scope = test_artifact();

        let ws = WitnessSignature::sign(&event_hash, &scope, &identity1, witness);
        assert!(!ws.verify(&event_hash, &scope, &identity2.verifying_key()));
    }

    #[test]
    fn test_witness_signature_wrong_hash_fails() {
        let identity = PQIdentity::generate();
        let witness = test_player(1);
        let scope = test_artifact();

        let ws = WitnessSignature::sign(&[0xBB; 32], &scope, &identity, witness);
        assert!(!ws.verify(&[0xCC; 32], &scope, &identity.verifying_key()));
    }

    #[test]
    fn test_quorum_certificate_creation() {
        let event_hash = [0xAA; 32];
        let scope = test_artifact();
        let cert = QuorumCertificate::new(event_hash, scope);
        assert_eq!(cert.version, 1);
        assert_eq!(cert.event_hash, event_hash);
        assert!(cert.witnesses.is_empty());
    }

    #[test]
    fn test_quorum_certificate_add_witness_deduplicates() {
        let identity = PQIdentity::generate();
        let witness = test_player(1);
        let event_hash = [0xAA; 32];
        let scope = test_artifact();

        let mut cert = QuorumCertificate::new(event_hash, scope);
        let ws = WitnessSignature::sign(&event_hash, &scope, &identity, witness);
        cert.add_witness(ws.clone());
        cert.add_witness(ws);
        assert_eq!(cert.witnesses.len(), 1);
    }

    #[test]
    fn test_validate_certificate_valid() {
        let event_hash = [0xAA; 32];
        let scope = test_artifact();

        // Create 3 witnesses
        let ids: Vec<PQIdentity> = (0..3).map(|_| PQIdentity::generate()).collect();
        let players: Vec<PlayerId> = (1..=3).map(|n| test_player(n)).collect();

        let mut cert = QuorumCertificate::new(event_hash, scope);
        let mut pubkeys = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            let ws = WitnessSignature::sign(&event_hash, &scope, id, players[i]);
            cert.add_witness(ws);
            pubkeys.insert(players[i], id.verifying_key());
        }

        // BFT: n=3, f=0, k=3 (all must sign)
        let result = validate_certificate(&cert, &players, 3, &pubkeys);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_certificate_insufficient_sigs() {
        let event_hash = [0xAA; 32];
        let scope = test_artifact();
        let identity = PQIdentity::generate();
        let witness = test_player(1);

        let mut cert = QuorumCertificate::new(event_hash, scope);
        let ws = WitnessSignature::sign(&event_hash, &scope, &identity, witness);
        cert.add_witness(ws);

        let mut pubkeys = HashMap::new();
        pubkeys.insert(witness, identity.verifying_key());

        // BFT: roster of 3 → f=0, k=3, but cert only has 1 signature
        let roster = [witness, test_player(2), test_player(3)];
        let (_, k) = crate::attention::witness::bft_quorum_threshold(roster.len()); // k=3
        let result = validate_certificate(&cert, &roster, k, &pubkeys);
        assert!(matches!(
            result,
            Err(CertificateError::InsufficientSignatures { have: 1, need: 3 })
        ));
    }

    #[test]
    fn test_validate_certificate_signer_not_in_roster() {
        let event_hash = [0xAA; 32];
        let scope = test_artifact();
        let identity = PQIdentity::generate();
        let witness = test_player(1);
        let other = test_player(2);

        let mut cert = QuorumCertificate::new(event_hash, scope);
        let ws = WitnessSignature::sign(&event_hash, &scope, &identity, witness);
        cert.add_witness(ws);

        let mut pubkeys = HashMap::new();
        pubkeys.insert(witness, identity.verifying_key());

        // Roster only contains `other`, not `witness`
        let result = validate_certificate(&cert, &[other], 1, &pubkeys);
        assert!(matches!(
            result,
            Err(CertificateError::SignerNotInRoster { .. })
        ));
    }

    #[test]
    fn test_validate_certificate_duplicate_witness() {
        let event_hash = [0xAA; 32];
        let scope = test_artifact();
        let identity = PQIdentity::generate();
        let witness = test_player(1);

        let mut cert = QuorumCertificate::new(event_hash, scope);
        let ws = WitnessSignature::sign(&event_hash, &scope, &identity, witness);
        // Manually push the same witness twice (bypassing add_witness dedup)
        cert.witnesses.push(ws.clone());
        cert.witnesses.push(ws);

        let mut pubkeys = HashMap::new();
        pubkeys.insert(witness, identity.verifying_key());

        let result = validate_certificate(&cert, &[witness], 1, &pubkeys);
        assert!(matches!(
            result,
            Err(CertificateError::DuplicateWitness { .. })
        ));
    }

    #[test]
    fn test_validate_certificate_invalid_signature() {
        let event_hash = [0xAA; 32];
        let scope = test_artifact();
        let identity1 = PQIdentity::generate();
        let identity2 = PQIdentity::generate();
        let witness = test_player(1);

        let mut cert = QuorumCertificate::new(event_hash, scope);
        // Sign with identity1 but register identity2's pubkey
        let ws = WitnessSignature::sign(&event_hash, &scope, &identity1, witness);
        cert.add_witness(ws);

        let mut pubkeys = HashMap::new();
        pubkeys.insert(witness, identity2.verifying_key());

        let result = validate_certificate(&cert, &[witness], 1, &pubkeys);
        assert!(matches!(
            result,
            Err(CertificateError::InvalidSignature { .. })
        ));
    }
}
