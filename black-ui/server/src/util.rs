use rand::{distr::Alphanumeric, RngExt};
use sha2::{Digest, Sha256};

pub fn random_token(len: usize) -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub fn hash_password(password: &str, salt: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(password.as_bytes());
    hex::encode(h.finalize())
}
