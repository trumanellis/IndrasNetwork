//! Certificate document for distributing quorum certificates via CRDT sync.
//!
//! Stores quorum certificates keyed by event hash. Certificates propagate
//! via the standard `Document<T>` CRDT sync path — no dedicated gossip
//! protocol is needed.
//!
//! # CRDT Semantics
//!
//! - Merge strategy: union by event_hash, keeping the certificate with
//!   more signatures if duplicates exist for the same event.

use indras_artifacts::attention::certificate::QuorumCertificate;
use indras_network::document::DocumentSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CRDT document storing quorum certificates.
///
/// Certificates are keyed by event_hash. When merging, if two certificates
/// exist for the same event, the one with more witness signatures wins.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CertificateDocument {
    /// event_hash -> QuorumCertificate.
    certificates: HashMap<[u8; 32], QuorumCertificate>,
}

impl CertificateDocument {
    /// Create a new empty certificate document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a quorum certificate.
    ///
    /// If a certificate already exists for this event hash, merges
    /// witness signatures (union by witness identity) so that
    /// signatures collected by different nodes accumulate.
    pub fn store_certificate(&mut self, cert: QuorumCertificate) {
        let key = cert.event_hash;
        match self.certificates.get_mut(&key) {
            Some(existing) => {
                // Union witness signatures by witness identity
                for ws in cert.witnesses {
                    if !existing.witnesses.iter().any(|w| w.witness == ws.witness) {
                        existing.witnesses.push(ws);
                    }
                }
            }
            None => {
                self.certificates.insert(key, cert);
            }
        }
    }

    /// Get a certificate by event hash.
    pub fn get_certificate(&self, event_hash: &[u8; 32]) -> Option<&QuorumCertificate> {
        self.certificates.get(event_hash)
    }

    /// Check if a certificate exists for an event hash.
    pub fn has_certificate(&self, event_hash: &[u8; 32]) -> bool {
        self.certificates.contains_key(event_hash)
    }

    /// Get all certificates.
    pub fn all_certificates(&self) -> &HashMap<[u8; 32], QuorumCertificate> {
        &self.certificates
    }

    /// Check if a certificate for the given event hash meets the quorum threshold.
    ///
    /// Returns `true` if a certificate exists and has at least `k` witness signatures.
    pub fn has_quorum(&self, event_hash: &[u8; 32], k: usize) -> bool {
        self.certificates
            .get(event_hash)
            .map_or(false, |cert| cert.witnesses.len() >= k)
    }

    /// Number of stored certificates.
    pub fn certificate_count(&self) -> usize {
        self.certificates.len()
    }
}

impl DocumentSchema for CertificateDocument {
    /// Merge: union by event_hash, keep cert with more signatures on conflict.
    fn merge(&mut self, remote: Self) {
        for (_hash, remote_cert) in remote.certificates {
            self.store_certificate(remote_cert);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_artifacts::artifact::ArtifactId;
    use indras_artifacts::attention::certificate::WitnessSignature;

    fn test_cert(event_hash: [u8; 32], num_witnesses: usize) -> QuorumCertificate {
        let scope = ArtifactId::Doc([0xAA; 32]);
        let mut cert = QuorumCertificate::new(event_hash, scope);
        for i in 0..num_witnesses {
            cert.witnesses.push(WitnessSignature {
                witness: [i as u8; 32],
                sig: vec![0u8; 32], // Dummy sig for merge tests
            });
        }
        cert
    }

    #[test]
    fn test_store_and_get() {
        let mut doc = CertificateDocument::new();
        let hash = [0x11; 32];
        let cert = test_cert(hash, 3);

        doc.store_certificate(cert.clone());
        assert!(doc.has_certificate(&hash));
        assert_eq!(doc.get_certificate(&hash).unwrap().witnesses.len(), 3);
    }

    #[test]
    fn test_store_unions_witness_sigs() {
        let mut doc = CertificateDocument::new();
        let hash = [0x11; 32];

        doc.store_certificate(test_cert(hash, 2));
        assert_eq!(doc.get_certificate(&hash).unwrap().witnesses.len(), 2);

        // Storing cert with overlapping + new sigs should union
        // test_cert uses witness IDs [0;32], [1;32], ... so witnesses 0,1 overlap with 2,3,4 new
        doc.store_certificate(test_cert(hash, 5));
        assert_eq!(doc.get_certificate(&hash).unwrap().witnesses.len(), 5);
    }

    #[test]
    fn test_store_unions_disjoint_witness_sets() {
        let mut doc = CertificateDocument::new();
        let hash = [0x11; 32];
        let scope = ArtifactId::Doc([0xAA; 32]);

        // Cert A has witnesses 10, 11
        let mut cert_a = QuorumCertificate::new(hash, scope);
        cert_a.witnesses.push(WitnessSignature { witness: [10; 32], sig: vec![0u8; 32] });
        cert_a.witnesses.push(WitnessSignature { witness: [11; 32], sig: vec![0u8; 32] });
        doc.store_certificate(cert_a);

        // Cert B has witnesses 12, 13
        let mut cert_b = QuorumCertificate::new(hash, scope);
        cert_b.witnesses.push(WitnessSignature { witness: [12; 32], sig: vec![0u8; 32] });
        cert_b.witnesses.push(WitnessSignature { witness: [13; 32], sig: vec![0u8; 32] });
        doc.store_certificate(cert_b);

        // Should have union: 4 witnesses
        assert_eq!(doc.get_certificate(&hash).unwrap().witnesses.len(), 4);
    }

    #[test]
    fn test_get_missing() {
        let doc = CertificateDocument::new();
        assert!(doc.get_certificate(&[0xFF; 32]).is_none());
        assert!(!doc.has_certificate(&[0xFF; 32]));
    }

    #[test]
    fn test_merge_union() {
        let mut doc1 = CertificateDocument::new();
        let mut doc2 = CertificateDocument::new();

        doc1.store_certificate(test_cert([0x11; 32], 3));
        doc2.store_certificate(test_cert([0x22; 32], 2));

        doc1.merge(doc2);
        assert_eq!(doc1.certificate_count(), 2);
        assert!(doc1.has_certificate(&[0x11; 32]));
        assert!(doc1.has_certificate(&[0x22; 32]));
    }

    #[test]
    fn test_merge_unions_witness_sigs() {
        let mut doc1 = CertificateDocument::new();
        let mut doc2 = CertificateDocument::new();
        let hash = [0x11; 32];
        let scope = ArtifactId::Doc([0xAA; 32]);

        // doc1 has witnesses 10, 11
        let mut cert1 = QuorumCertificate::new(hash, scope);
        cert1.witnesses.push(WitnessSignature { witness: [10; 32], sig: vec![0u8; 32] });
        cert1.witnesses.push(WitnessSignature { witness: [11; 32], sig: vec![0u8; 32] });
        doc1.store_certificate(cert1);

        // doc2 has witnesses 11, 12, 13 (11 overlaps)
        let mut cert2 = QuorumCertificate::new(hash, scope);
        cert2.witnesses.push(WitnessSignature { witness: [11; 32], sig: vec![0u8; 32] });
        cert2.witnesses.push(WitnessSignature { witness: [12; 32], sig: vec![0u8; 32] });
        cert2.witnesses.push(WitnessSignature { witness: [13; 32], sig: vec![0u8; 32] });
        doc2.store_certificate(cert2);

        doc1.merge(doc2);
        // Union: 10, 11, 12, 13 = 4 unique witnesses
        assert_eq!(doc1.get_certificate(&hash).unwrap().witnesses.len(), 4);
    }

    #[test]
    fn test_has_quorum() {
        let mut doc = CertificateDocument::new();
        let hash = [0x11; 32];

        doc.store_certificate(test_cert(hash, 3));

        assert!(doc.has_quorum(&hash, 2));  // 3 >= 2
        assert!(doc.has_quorum(&hash, 3));  // 3 >= 3
        assert!(!doc.has_quorum(&hash, 4)); // 3 < 4
        assert!(!doc.has_quorum(&[0xFF; 32], 1)); // no cert
    }

    #[test]
    fn test_all_certificates() {
        let mut doc = CertificateDocument::new();
        doc.store_certificate(test_cert([0x11; 32], 1));
        doc.store_certificate(test_cert([0x22; 32], 2));
        doc.store_certificate(test_cert([0x33; 32], 3));

        assert_eq!(doc.all_certificates().len(), 3);
    }
}
