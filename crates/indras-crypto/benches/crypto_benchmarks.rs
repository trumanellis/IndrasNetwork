//! Cryptographic performance benchmarks
//!
//! Benchmarks for critical crypto operations:
//! - Key generation (PQ and classical)
//! - Encryption/decryption (ChaCha20-Poly1305)
//! - Key encapsulation (ML-KEM-768)
//! - Signing/verification (ML-DSA-65)
//!
//! Run with: cargo bench -p indras-crypto

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use indras_core::InterfaceId;
use indras_crypto::{
    InterfaceKey, KeyDistribution, PQIdentity, PQKemKeyPair,
};

// ============================================================================
// Key Generation Benchmarks
// ============================================================================

fn bench_key_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("key_generation");

    // Interface key generation (uses ChaCha20 RNG)
    group.bench_function("interface_key", |b| {
        let interface_id = InterfaceId::generate();
        b.iter(|| InterfaceKey::generate(black_box(interface_id)))
    });

    // Post-quantum KEM key pair generation (ML-KEM-768)
    group.bench_function("pq_kem_keypair", |b| {
        b.iter(|| PQKemKeyPair::generate())
    });

    // Post-quantum identity generation (ML-DSA-65)
    group.bench_function("pq_identity", |b| {
        b.iter(|| PQIdentity::generate())
    });

    group.finish();
}

// ============================================================================
// Encryption/Decryption Benchmarks (ChaCha20-Poly1305)
// ============================================================================

fn bench_encryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption");

    let interface_id = InterfaceId::generate();
    let key = InterfaceKey::generate(interface_id);

    // Small message (typical chat message)
    let small_msg = vec![0u8; 256];
    group.throughput(Throughput::Bytes(256));
    group.bench_function("encrypt_256b", |b| {
        b.iter(|| key.encrypt(black_box(&small_msg)).unwrap())
    });

    // Medium message (larger payload)
    let medium_msg = vec![0u8; 4096];
    group.throughput(Throughput::Bytes(4096));
    group.bench_function("encrypt_4kb", |b| {
        b.iter(|| key.encrypt(black_box(&medium_msg)).unwrap())
    });

    // Large message (file transfer)
    let large_msg = vec![0u8; 65536];
    group.throughput(Throughput::Bytes(65536));
    group.bench_function("encrypt_64kb", |b| {
        b.iter(|| key.encrypt(black_box(&large_msg)).unwrap())
    });

    group.finish();
}

fn bench_decryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("decryption");

    let interface_id = InterfaceId::generate();
    let key = InterfaceKey::generate(interface_id);

    // Small message
    let small_msg = vec![0u8; 256];
    let small_encrypted = key.encrypt(&small_msg).unwrap();
    group.throughput(Throughput::Bytes(256));
    group.bench_function("decrypt_256b", |b| {
        b.iter(|| key.decrypt(black_box(&small_encrypted)).unwrap())
    });

    // Medium message
    let medium_msg = vec![0u8; 4096];
    let medium_encrypted = key.encrypt(&medium_msg).unwrap();
    group.throughput(Throughput::Bytes(4096));
    group.bench_function("decrypt_4kb", |b| {
        b.iter(|| key.decrypt(black_box(&medium_encrypted)).unwrap())
    });

    // Large message
    let large_msg = vec![0u8; 65536];
    let large_encrypted = key.encrypt(&large_msg).unwrap();
    group.throughput(Throughput::Bytes(65536));
    group.bench_function("decrypt_64kb", |b| {
        b.iter(|| key.decrypt(black_box(&large_encrypted)).unwrap())
    });

    group.finish();
}

// ============================================================================
// Key Encapsulation Benchmarks (ML-KEM-768)
// ============================================================================

fn bench_key_encapsulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("key_encapsulation");

    let interface_id = InterfaceId::generate();
    let interface_key = InterfaceKey::generate(interface_id);
    let recipient_kem = PQKemKeyPair::generate();
    let recipient_pk = recipient_kem.encapsulation_key();

    // Create invite (encapsulate)
    group.bench_function("create_invite", |b| {
        b.iter(|| {
            KeyDistribution::create_invite(black_box(&interface_key), black_box(&recipient_pk))
                .unwrap()
        })
    });

    // Accept invite (decapsulate)
    let invite = KeyDistribution::create_invite(&interface_key, &recipient_pk).unwrap();
    group.bench_function("accept_invite", |b| {
        b.iter(|| {
            KeyDistribution::accept_invite(black_box(&invite), black_box(&recipient_kem)).unwrap()
        })
    });

    group.finish();
}

// ============================================================================
// Signing/Verification Benchmarks (ML-DSA-65)
// ============================================================================

fn bench_signing(c: &mut Criterion) {
    let mut group = c.benchmark_group("signing");

    let identity = PQIdentity::generate();

    // Small message
    let small_msg = vec![0u8; 256];
    group.bench_function("sign_256b", |b| {
        b.iter(|| identity.sign(black_box(&small_msg)))
    });

    // Medium message
    let medium_msg = vec![0u8; 4096];
    group.bench_function("sign_4kb", |b| {
        b.iter(|| identity.sign(black_box(&medium_msg)))
    });

    // Large message
    let large_msg = vec![0u8; 65536];
    group.bench_function("sign_64kb", |b| {
        b.iter(|| identity.sign(black_box(&large_msg)))
    });

    group.finish();
}

fn bench_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("verification");

    let identity = PQIdentity::generate();
    let public_identity = identity.verifying_key();

    // Small message
    let small_msg = vec![0u8; 256];
    let small_sig = identity.sign(&small_msg);
    group.bench_function("verify_256b", |b| {
        b.iter(|| {
            public_identity.verify(black_box(&small_msg), black_box(&small_sig))
        })
    });

    // Medium message
    let medium_msg = vec![0u8; 4096];
    let medium_sig = identity.sign(&medium_msg);
    group.bench_function("verify_4kb", |b| {
        b.iter(|| {
            public_identity.verify(black_box(&medium_msg), black_box(&medium_sig))
        })
    });

    // Large message
    let large_msg = vec![0u8; 65536];
    let large_sig = identity.sign(&large_msg);
    group.bench_function("verify_64kb", |b| {
        b.iter(|| {
            public_identity.verify(black_box(&large_msg), black_box(&large_sig))
        })
    });

    group.finish();
}

// ============================================================================
// Throughput Scaling Benchmarks
// ============================================================================

fn bench_encryption_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption_scaling");

    let interface_id = InterfaceId::generate();
    let key = InterfaceKey::generate(interface_id);

    for size in [64, 256, 1024, 4096, 16384, 65536] {
        let msg = vec![0u8; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &msg, |b, msg| {
            b.iter(|| key.encrypt(black_box(msg)).unwrap())
        });
    }

    group.finish();
}

// ============================================================================
// End-to-End Message Flow Benchmark
// ============================================================================

fn bench_message_flow(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_flow");

    // Simulate full message flow: encrypt + sign
    let interface_id = InterfaceId::generate();
    let key = InterfaceKey::generate(interface_id);
    let sender_identity = PQIdentity::generate();

    let message = vec![0u8; 1024]; // 1KB message

    group.bench_function("encrypt_and_sign_1kb", |b| {
        b.iter(|| {
            let encrypted = key.encrypt(black_box(&message)).unwrap();
            let encrypted_bytes = encrypted.to_bytes();
            let signature = sender_identity.sign(black_box(&encrypted_bytes));
            (encrypted, signature)
        })
    });

    // Full round-trip: encrypt, sign, verify, decrypt
    let receiver_identity = sender_identity.verifying_key();

    group.bench_function("full_roundtrip_1kb", |b| {
        b.iter(|| {
            // Sender side
            let encrypted = key.encrypt(black_box(&message)).unwrap();
            let encrypted_bytes = encrypted.to_bytes();
            let signature = sender_identity.sign(&encrypted_bytes);

            // Receiver side
            let verified = receiver_identity.verify(&encrypted_bytes, &signature);
            assert!(verified);
            let decrypted = key.decrypt(&encrypted).unwrap();
            black_box(decrypted)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_key_generation,
    bench_encryption,
    bench_decryption,
    bench_key_encapsulation,
    bench_signing,
    bench_verification,
    bench_encryption_scaling,
    bench_message_flow,
);

criterion_main!(benches);
