//! Shared data-plane AEAD primitive.
//!
//! Every encrypted proxy protocol (VMess, Trojan, Shadowsocks-2022, REALITY)
//! ultimately seals/opens a stream of AEAD chunks with one of three suites:
//! AES-128-GCM, AES-256-GCM, or ChaCha20-Poly1305, all with a 12-byte nonce and
//! a 16-byte tag. This module centralises that operation so the whole data plane
//! can share a single, fast, well-tested implementation.
//!
//! # Backend
//!
//! By default the active backend is **aws-lc-rs** (BoringSSL assembly, with
//! aggregated GHASH / VAES on capable CPUs), which is materially faster on bulk
//! AES-GCM than the pure-Rust path. The `aead-rustcrypto` feature flips the
//! active backend to RustCrypto as a fallback. Both backends are always
//! compiled, and the test module asserts they produce **byte-identical**
//! ciphertext and tags so swapping the backend can never change wire behaviour.

/// AEAD authentication tag length (bytes). All supported suites use 16.
pub const TAG_LEN: usize = 16;

/// AEAD suites used by the data plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AeadAlgorithm {
    /// AES-128-GCM (16-byte key).
    Aes128Gcm,
    /// AES-256-GCM (32-byte key).
    Aes256Gcm,
    /// ChaCha20-Poly1305 (32-byte key).
    ChaCha20Poly1305,
}

/// Opaque AEAD failure (bad key length, or authentication failure on open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AeadError;

impl std::fmt::Display for AeadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AEAD operation failed")
    }
}

impl std::error::Error for AeadError {}

// ── aws-lc-rs backend ──────────────────────────────────────────────────────────

// Both backends are always compiled (the cross-validation tests need both);
// whichever one is not the active data-plane backend is only used by tests, so
// allow it to read as dead code in a normal build.
#[cfg_attr(feature = "aead-rustcrypto", allow(dead_code))]
mod backend_awslc {
    use super::{AeadAlgorithm, AeadError, TAG_LEN};
    use aws_lc_rs::aead::{
        Aad, LessSafeKey, Nonce, UnboundKey, AES_128_GCM, AES_256_GCM, CHACHA20_POLY1305,
    };

    /// AEAD key with its schedule expanded once, backed by aws-lc-rs.
    pub struct AwsAeadKey {
        key: LessSafeKey,
    }

    impl AwsAeadKey {
        /// Expand `key` into a ready-to-use AEAD key for `alg`.
        ///
        /// Fails if `key` has the wrong length for the chosen algorithm.
        pub fn new(alg: AeadAlgorithm, key: &[u8]) -> Result<Self, AeadError> {
            let algorithm = match alg {
                AeadAlgorithm::Aes128Gcm => &AES_128_GCM,
                AeadAlgorithm::Aes256Gcm => &AES_256_GCM,
                AeadAlgorithm::ChaCha20Poly1305 => &CHACHA20_POLY1305,
            };
            let unbound = UnboundKey::new(algorithm, key).map_err(|_| AeadError)?;
            Ok(Self {
                key: LessSafeKey::new(unbound),
            })
        }

        /// Seal `in_out` in place and return the detached authentication tag.
        ///
        /// `in_out` holds the plaintext on entry and the ciphertext (same
        /// length) on return; the tag is appended by the caller as needed.
        #[inline]
        pub fn seal_detached(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            in_out: &mut [u8],
        ) -> [u8; TAG_LEN] {
            let tag = self
                .key
                .seal_in_place_separate_tag(
                    Nonce::assume_unique_for_key(*nonce),
                    Aad::from(aad),
                    in_out,
                )
                .expect("AEAD seal cannot fail for valid plaintext lengths");
            let mut out = [0u8; TAG_LEN];
            out.copy_from_slice(tag.as_ref());
            out
        }

        /// Open an `in_out` buffer holding `ciphertext || tag` in place.
        ///
        /// Returns the plaintext length on success; the caller truncates
        /// `in_out` to that length to drop the consumed tag. Fails on
        /// authentication failure.
        #[inline]
        pub fn open_combined(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            in_out: &mut [u8],
        ) -> Result<usize, AeadError> {
            let plaintext = self
                .key
                .open_in_place(Nonce::assume_unique_for_key(*nonce), Aad::from(aad), in_out)
                .map_err(|_| AeadError)?;
            Ok(plaintext.len())
        }

        /// Open `in_out` (ciphertext only, same length as the plaintext) in
        /// place against a detached `tag`. Fails on authentication failure.
        #[inline]
        pub fn open_detached(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            tag: &[u8],
            in_out: &mut [u8],
        ) -> Result<(), AeadError> {
            self.key
                .open_in_place_separate_tag(
                    Nonce::assume_unique_for_key(*nonce),
                    Aad::from(aad),
                    tag,
                    in_out,
                )
                .map_err(|_| AeadError)?;
            Ok(())
        }
    }
}

// ── RustCrypto backend (fallback) ───────────────────────────────────────────────

#[cfg_attr(not(feature = "aead-rustcrypto"), allow(dead_code))]
mod backend_rustcrypto {
    use super::{AeadAlgorithm, AeadError, TAG_LEN};
    use aes_gcm::aead::generic_array::GenericArray;
    use aes_gcm::aead::AeadInPlace;
    use aes_gcm::{Aes128Gcm, Aes256Gcm, KeyInit};
    use chacha20poly1305::ChaCha20Poly1305;

    enum Inner {
        Aes128(Box<Aes128Gcm>),
        Aes256(Box<Aes256Gcm>),
        ChaCha(Box<ChaCha20Poly1305>),
    }

    /// AEAD key with its schedule expanded once, backed by RustCrypto.
    pub struct RcAeadKey {
        inner: Inner,
    }

    impl RcAeadKey {
        /// Expand `key` into a ready-to-use AEAD key for `alg`.
        ///
        /// Fails if `key` has the wrong length for the chosen algorithm.
        pub fn new(alg: AeadAlgorithm, key: &[u8]) -> Result<Self, AeadError> {
            let inner = match alg {
                AeadAlgorithm::Aes128Gcm => Inner::Aes128(Box::new(
                    Aes128Gcm::new_from_slice(key).map_err(|_| AeadError)?,
                )),
                AeadAlgorithm::Aes256Gcm => Inner::Aes256(Box::new(
                    Aes256Gcm::new_from_slice(key).map_err(|_| AeadError)?,
                )),
                AeadAlgorithm::ChaCha20Poly1305 => Inner::ChaCha(Box::new(
                    ChaCha20Poly1305::new_from_slice(key).map_err(|_| AeadError)?,
                )),
            };
            Ok(Self { inner })
        }

        /// Seal `in_out` in place and return the detached authentication tag.
        ///
        /// `in_out` holds the plaintext on entry and the ciphertext (same
        /// length) on return; the tag is appended by the caller as needed.
        #[inline]
        pub fn seal_detached(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            in_out: &mut [u8],
        ) -> [u8; TAG_LEN] {
            let n = GenericArray::from_slice(nonce);
            let tag = match &self.inner {
                Inner::Aes128(c) => c.encrypt_in_place_detached(n, aad, in_out),
                Inner::Aes256(c) => c.encrypt_in_place_detached(n, aad, in_out),
                Inner::ChaCha(c) => c.encrypt_in_place_detached(n, aad, in_out),
            }
            .expect("AEAD seal cannot fail for valid plaintext lengths");
            let mut out = [0u8; TAG_LEN];
            out.copy_from_slice(tag.as_slice());
            out
        }

        /// Open an `in_out` buffer holding `ciphertext || tag` in place.
        ///
        /// Returns the plaintext length on success; the caller truncates
        /// `in_out` to that length to drop the consumed tag. Fails on
        /// authentication failure.
        #[inline]
        pub fn open_combined(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            in_out: &mut [u8],
        ) -> Result<usize, AeadError> {
            let split = in_out.len().checked_sub(TAG_LEN).ok_or(AeadError)?;
            let (ciphertext, tag) = in_out.split_at_mut(split);
            self.open_split(nonce, aad, ciphertext, tag)?;
            Ok(ciphertext.len())
        }

        /// Open `in_out` (ciphertext only, same length as the plaintext) in
        /// place against a detached `tag`. Fails on authentication failure.
        #[inline]
        pub fn open_detached(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            tag: &[u8],
            in_out: &mut [u8],
        ) -> Result<(), AeadError> {
            self.open_split(nonce, aad, in_out, tag)
        }

        #[inline]
        fn open_split(
            &self,
            nonce: &[u8; 12],
            aad: &[u8],
            ciphertext: &mut [u8],
            tag: &[u8],
        ) -> Result<(), AeadError> {
            let n = GenericArray::from_slice(nonce);
            let tag = GenericArray::from_slice(tag);
            match &self.inner {
                Inner::Aes128(c) => c.decrypt_in_place_detached(n, aad, ciphertext, tag),
                Inner::Aes256(c) => c.decrypt_in_place_detached(n, aad, ciphertext, tag),
                Inner::ChaCha(c) => c.decrypt_in_place_detached(n, aad, ciphertext, tag),
            }
            .map_err(|_| AeadError)
        }
    }
}

// ── Active backend selection ────────────────────────────────────────────────────

#[cfg(not(feature = "aead-rustcrypto"))]
pub use backend_awslc::AwsAeadKey as AeadKey;

#[cfg(feature = "aead-rustcrypto")]
pub use backend_rustcrypto::RcAeadKey as AeadKey;

#[cfg(test)]
mod tests {
    use super::backend_awslc::AwsAeadKey;
    use super::backend_rustcrypto::RcAeadKey;
    use super::*;

    fn key_for(alg: AeadAlgorithm) -> Vec<u8> {
        let len = match alg {
            AeadAlgorithm::Aes128Gcm => 16,
            AeadAlgorithm::Aes256Gcm | AeadAlgorithm::ChaCha20Poly1305 => 32,
        };
        (0..len as u8).collect()
    }

    const ALGS: [AeadAlgorithm; 3] = [
        AeadAlgorithm::Aes128Gcm,
        AeadAlgorithm::Aes256Gcm,
        AeadAlgorithm::ChaCha20Poly1305,
    ];

    /// The two backends must produce byte-identical ciphertext and tags for the
    /// same key/nonce/AAD/plaintext. This is the wire-compatibility guarantee
    /// that lets us swap the active backend without affecting interop.
    #[test]
    fn backends_produce_identical_ciphertext_and_tag() {
        let nonce = [7u8; 12];
        let aad = b"5-byte-ish header bytes";
        let plaintext: Vec<u8> = (0..1000).map(|i| (i % 251) as u8).collect();

        for alg in ALGS {
            let key = key_for(alg);
            let aws = AwsAeadKey::new(alg, &key).unwrap();
            let rc = RcAeadKey::new(alg, &key).unwrap();

            let mut a = plaintext.clone();
            let mut b = plaintext.clone();
            let tag_a = aws.seal_detached(&nonce, aad, &mut a);
            let tag_b = rc.seal_detached(&nonce, aad, &mut b);

            assert_eq!(a, b, "{alg:?}: ciphertext differs between backends");
            assert_eq!(tag_a, tag_b, "{alg:?}: tag differs between backends");
        }
    }

    /// Ciphertext sealed by one backend must open under the other (both ways),
    /// for both the detached-tag and combined (ct||tag) layouts.
    #[test]
    fn cross_backend_open_roundtrips() {
        let nonce = [9u8; 12];
        let aad: &[u8] = b"";
        let plaintext = b"the quick brown fox jumps over the lazy dog".to_vec();

        for alg in ALGS {
            let key = key_for(alg);
            let aws = AwsAeadKey::new(alg, &key).unwrap();
            let rc = RcAeadKey::new(alg, &key).unwrap();

            // Seal with aws-lc, open with RustCrypto (detached + combined).
            let mut buf = plaintext.clone();
            let tag = aws.seal_detached(&nonce, aad, &mut buf);
            let mut detached = buf.clone();
            rc.open_detached(&nonce, aad, &tag, &mut detached).unwrap();
            assert_eq!(detached, plaintext, "{alg:?}: aws->rc detached");

            let mut combined = buf.clone();
            combined.extend_from_slice(&tag);
            let n = rc.open_combined(&nonce, aad, &mut combined).unwrap();
            assert_eq!(&combined[..n], &plaintext[..], "{alg:?}: aws->rc combined");

            // Seal with RustCrypto, open with aws-lc.
            let mut buf2 = plaintext.clone();
            let tag2 = rc.seal_detached(&nonce, aad, &mut buf2);
            let mut detached2 = buf2.clone();
            aws.open_detached(&nonce, aad, &tag2, &mut detached2)
                .unwrap();
            assert_eq!(detached2, plaintext, "{alg:?}: rc->aws detached");

            let mut combined2 = buf2;
            combined2.extend_from_slice(&tag2);
            let n2 = aws.open_combined(&nonce, aad, &mut combined2).unwrap();
            assert_eq!(
                &combined2[..n2],
                &plaintext[..],
                "{alg:?}: rc->aws combined"
            );
        }
    }

    /// A corrupted tag must fail authentication on both backends.
    #[test]
    fn tampered_tag_fails_to_open() {
        let nonce = [1u8; 12];
        let aad: &[u8] = b"aad";
        for alg in ALGS {
            let key = key_for(alg);
            let aws = AwsAeadKey::new(alg, &key).unwrap();
            let rc = RcAeadKey::new(alg, &key).unwrap();
            let mut buf = b"secret payload".to_vec();
            let mut tag = aws.seal_detached(&nonce, aad, &mut buf);
            tag[0] ^= 0xff;
            assert_eq!(
                aws.open_detached(&nonce, aad, &tag, &mut buf.clone()),
                Err(AeadError)
            );
            assert_eq!(
                rc.open_detached(&nonce, aad, &tag, &mut buf.clone()),
                Err(AeadError)
            );
        }
    }
}
