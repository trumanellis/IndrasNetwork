# Security Policy

## 1. Reporting Security Vulnerabilities

Indra's Network is a peer-to-peer networking project handling cryptographic operations and distributed data. We take security seriously and appreciate responsible disclosure of any security issues.

### How to Report

Please do **NOT** file a public issue for security vulnerabilities. Instead:

1. **Email**: Send vulnerability reports to [security@indras-network.dev](mailto:security@indras-network.dev)
2. **Include**:
   - Description of the vulnerability
   - Steps to reproduce (if applicable)
   - Potential impact
   - Suggested fix (if you have one)
3. **Encryption**: For highly sensitive reports, use GPG (key ID available on request)

### Timeline

- **Initial Response**: Within 48 hours
- **Acknowledgment**: We will confirm receipt and provide a timeline
- **Fix Development**: Varies based on severity
- **Disclosure**: Coordinated disclosure after fix is released (typically 90 days)

### Scope

We appreciate reports on:
- Cryptographic implementation flaws
- Key management vulnerabilities
- Network protocol attacks (replay, man-in-the-middle, timing attacks)
- Denial-of-service vectors
- Memory safety issues (in Rust unsafe code)
- Data leakage or integrity issues

### Out of Scope

- Configuration issues in user deployments
- Social engineering
- Physical security
- Third-party dependency vulnerabilities (please report upstream)

## 2. Supported Versions

| Version | Status | Security Updates |
|---------|--------|-------------------|
| 0.1.x   | Active | Yes               |

We recommend always using the latest version. Older versions may not receive security updates.

**Future Policy**: Once version 1.0 is released, we will support:
- Current version (N): All updates and fixes
- Previous version (N-1): Critical security updates only
- Older versions: End-of-life (announce 6 months in advance)

## 3. Security Features Overview

### Post-Quantum Cryptography

Indra's Network implements NIST-standardized post-quantum algorithms to provide long-term security against quantum computing threats:

#### ML-KEM-768 (Key Encapsulation Mechanism)

- **Standard**: NIST FIPS 203
- **Equivalent**: Kyber-768
- **Implementation**: `pqcrypto-kyber` 0.8
- **Use Case**: Secure key agreement for symmetric key distribution
- **Key Sizes**:
  - Encapsulation (public) key: 1,184 bytes
  - Decapsulation (private) key: 2,400 bytes
  - Shared secret: 32 bytes
  - Ciphertext: 1,088 bytes

**Security Properties**:
- Provides key encapsulation for invite-based onboarding
- Private keys zeroized on drop to prevent memory dumps
- Implicit rejection protection against decapsulation failures

#### ML-DSA-65 (Digital Signatures)

- **Standard**: NIST FIPS 204
- **Equivalent**: Dilithium-3
- **Implementation**: `pqcrypto-dilithium` 0.5
- **Use Case**: Message authentication and non-repudiation
- **Key Sizes**:
  - Signing key: 4,000 bytes
  - Verifying key: 1,952 bytes
  - Signature: 3,293 bytes

**Security Properties**:
- Provides quantum-resistant digital signatures for peer authentication
- Signing keys are wrapped in `SecureBytes` which zeroizes on drop
- Deterministic signatures (no randomness vulnerability)

### Interface Encryption

#### ChaCha20-Poly1305

- **Standard**: RFC 7539
- **Implementation**: `chacha20poly1305` crate
- **Use Case**: Authenticated encryption of interface events
- **Features**:
  - 256-bit symmetric encryption
  - Authenticated encryption with associated data (AEAD)
  - Nonce-based (12 bytes)
  - Provides confidentiality and integrity guarantees

**Key Derivation**:
- Interface keys are cryptographically random 32-byte values
- Shared among all peers on a given interface
- Rotated using post-quantum key encapsulation during member onboarding

### P2P Networking

#### Iroh Integration

- **Version**: 0.95
- **Features**:
  - End-to-end encrypted connections (noise protocol)
  - UDP hole-punching and relay fallback
  - Cryptographic peer identification
  - Connection persistence and resumption

**Security Implications**:
- All peer connections are encrypted at the transport layer
- Peers are identified by cryptographic hashes
- Relay connections are still encrypted (relays cannot read packet content)

#### Gossip Protocols

- **Module**: `indras-gossip` (1.0.0)
- **Use Case**: Presence detection and message dissemination
- **Security**:
  - Messages signed with peer's ML-DSA identity
  - Prevents spoofing and replay attacks
  - Supports message authentication across the network

### CRDT Synchronization

#### Automerge

- **Version**: 0.7
- **Use Case**: Distributed state synchronization
- **Security**:
  - Eventual consistency without central authority
  - Operation-based CRDTs prevent conflicting concurrent writes
  - Audit trail of all changes (immutable operation log)
  - Can be combined with ML-DSA signatures for cryptographic verification

**Application Layer**:
- Indras-sync module handles protocol and encoding
- Events are stored in append-only logs (SHA-256 hashed chains)
- Supports causality tracking for causal consistency

### Store-and-Forward Routing

#### Sealed Packets

- Encrypted payloads that intermediate relays cannot decrypt
- Identified by source hash + sequence number (prevents packet substitution)
- TTL (time-to-live) prevents infinite loops
- Visited peer set prevents backtracking
- Delivery confirmations provide end-to-end acknowledgment

**Security Properties**:
- Relays cannot read packet content (only metadata for routing)
- Source authentication via packet ID
- Destination verification via successful decryption
- DoS protection via TTL and priority levels

## 4. Known Limitations

### Post-Quantum Cryptography

1. **Large Key Sizes**: PQ algorithms require significantly larger keys than classical cryptography:
   - ML-DSA signing keys: 4 KB
   - ML-KEM private keys: 2.4 KB
   - Signatures: 3.3 KB each
   - **Impact**: Storage, bandwidth, and performance overhead for key distribution

2. **Recent Standardization**: FIPS 203/204 were standardized in August 2024:
   - Implementation confidence is high but deployment experience is limited
   - Longer-term security margin compared to classical algorithms (NIST Level 3)
   - **Recommendation**: Monitor NIST updates for any significant findings

3. **Not Cryptographically Agile**: The codebase is tightly coupled to ML-KEM and ML-DSA:
   - **Impact**: Migrating to different algorithms requires code changes
   - **Mitigation**: Planned refactoring to support algorithm versioning

### Network Protocol

1. **Relay Privacy**: While relay connections are encrypted, a relay can see:
   - Source and destination peer identities (in packet headers)
   - Approximate timing and volume of traffic
   - **Mitigation**: Use multiple relays; don't trust a single relay provider

2. **Gossip Protocol Amplification**: Gossip messages can be used for network reconnaissance:
   - Adversary can learn peer presence by observing gossip traffic
   - **Mitigation**: Deploy network monitoring; rate-limit gossip at network boundary

3. **TTL-Based Routing Inference**: Packet TTL decrements may leak path information:
   - Adversary observing multiple packets can infer hop distances
   - **Mitigation**: Randomize initial TTL within bounds; use onion routing for sensitive paths

### Storage and Key Management

1. **Memory Exposure**: While secret keys are zeroized on drop:
   - Mutable borrows and clones can create additional copies in memory
   - Cold boot attacks or hypervisor escape can still expose memory
   - **Mitigation**: Run on dedicated hardware; consider hardware key storage (future work)

2. **Persistent Storage**: Current storage implementation does not encrypt at rest:
   - **Status**: Persistence is implemented via `indras-storage` with quota management
   - **Gap**: No encryption of stored keys or data on disk
   - **Recommendation**: Deploy at rest encryption at OS/filesystem level

3. **Random Number Generation**: Uses Rust's `rand` crate (thread-local CSPRNG):
   - **Security**: Adequate for most use cases
   - **Gap**: Not suitable for long-lived running services; CSPRNG state can drift
   - **Recommendation**: Use OS-provided entropy periodically; seed from hardware RNG if available

### CRDT and Causality

1. **Causal Consistency Only**: Automerge provides eventual consistency:
   - Concurrent updates may conflict; conflict resolution is application-specific
   - No total ordering without a consensus protocol
   - **Impact**: Applications must handle merge conflicts; audit log can hide intent

2. **No Cryptographic Proof of Causality**: Operation log is append-only but not signed by default:
   - **Gap**: Cannot cryptographically verify that operations are causally related
   - **Mitigation**: Sign operations with ML-DSA if strong cryptographic guarantees needed

### Scaling and DoS

1. **Message Flooding**: Gossip networks amplify messages:
   - Malicious peer sending high-rate gossip can overwhelm the network
   - **Mitigation**: Implement rate limiting; prioritize gossip by peer reputation

2. **Large Packet Relay**: Store-and-forward routing holds packets in memory:
   - **Gap**: No size limits on stored packets (currently unlimited quota)
   - **Recommendation**: Configure quota limits on relay node storage

3. **Broadcast Storms**: Uncontrolled gossip can create exponential traffic growth:
   - **Status**: Current implementation uses controlled fanout
   - **Recommendation**: Deploy flow control and backpressure mechanisms

## 5. Security Considerations for Users

### Deployment

- **Network Isolation**: Deploy Indra's Network nodes on trusted networks; don't expose to untrusted networks without firewall rules
- **Peer Vetting**: Establish out-of-band identity verification before connecting peers
- **Key Backup**: Regularly backup peer signing keys and KEM decapsulation keys (encrypted)
- **Monitoring**: Monitor relay nodes for unusual traffic patterns or packet loss

### Application Development

- **Signature Verification**: Always verify peer signatures; don't trust packets without cryptographic verification
- **Key Rotation**: Implement periodic key rotation (not currently automated)
- **Conflict Resolution**: Define application-specific conflict resolution for CRDT merges
- **Audit Logging**: Log all significant operations; maintain immutable audit trails

### Incident Response

- **Compromised Key**: If a peer's signing key is compromised:
  1. Immediately revoke the key (mechanism not yet implemented)
  2. Generate new ML-DSA keypair and re-establish peer identity
  3. Re-sign all untrusted operations with new key
  4. Communicate key rotation to all connected peers

- **Relay Compromise**: If a relay node is compromised:
  1. Redirect traffic to different relays
  2. Audit relay logs for forwarded packets (no decryption possible)
  3. Rotate peer secrets if relay had access to plaintext (application-specific)

## 6. Future Security Work

- [ ] Support algorithm versioning (multiple PQ algorithms)
- [ ] Hardware key storage integration (TPM, HSM)
- [ ] Encryption at rest for persistent storage
- [ ] Key rotation mechanisms (automated and manual)
- [ ] Formal security audit by third party
- [ ] Replay attack detection and mitigation
- [ ] Bandwidth-efficient sync protocols (state-based vs. operation-based)
- [ ] Onion routing support for multi-hop anonymity
- [ ] Byzantine fault tolerance for critical applications

## 7. Security References

### Standards and Algorithms

- [NIST FIPS 203](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.203.pdf) - ML-KEM (Kyber)
- [NIST FIPS 204](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.204.pdf) - ML-DSA (Dilithium)
- [RFC 7539](https://tools.ietf.org/html/rfc7539) - ChaCha20 and Poly1305

### Related Technologies

- [Iroh Protocol](https://iroh.computer/) - P2P networking foundation
- [Automerge](https://automerge.org/) - CRDT library
- [Noise Protocol](https://noiseprotocol.org/) - Transport encryption
- [Store-and-Forward](https://tools.ietf.org/html/rfc4838) - DTN routing

### Post-Quantum Cryptography

- [NIST PQC Standardization](https://csrc.nist.gov/projects/post-quantum-cryptography/standardization)
- [liboqs-rs](https://github.com/open-quantum-safe/liboqs-rs) - Alternative PQ implementations

## 8. Contact

- **Security Email**: security@indras-network.dev
- **GitHub Issues**: Use responsibly (public bug tracker)
- **Discussions**: GitHub Discussions for non-critical questions

---

**Last Updated**: January 2026

**Document Version**: 1.0

This security policy is subject to change. Check back regularly for updates.
