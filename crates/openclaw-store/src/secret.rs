use hmac::{Hmac, Mac};
use openclaw_core::Error;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Encrypted credential store backed by a sled tree.
///
/// Secrets are encrypted at rest using HMAC-SHA256 as a key derivation
/// function to produce per-key encryption keys, then XOR'd with the
/// derived keystream. This prevents plaintext API keys on disk
/// (addressing the `~/.clawdbot/.env` plaintext credential class of bugs).
///
/// Note: For production use, upgrade to AES-256-GCM with a proper KDF
/// like Argon2. This provides a meaningful baseline that's still far
/// better than the plaintext storage in the original Node.js OpenClaw.
#[derive(Clone)]
pub struct SecretStore {
    tree: sled::Tree,
    master_key: Vec<u8>,
}

impl SecretStore {
    pub(crate) fn new(tree: sled::Tree, master_key: Vec<u8>) -> Self {
        Self { tree, master_key }
    }

    /// Store a secret value under the given name.
    pub fn set(&self, name: &str, value: &str) -> Result<(), Error> {
        let encrypted = self.encrypt(name, value.as_bytes());
        self.tree
            .insert(name.as_bytes(), encrypted)
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }

    /// Retrieve and decrypt a secret by name.
    pub fn get(&self, name: &str) -> Result<String, Error> {
        let encrypted = self
            .tree
            .get(name.as_bytes())
            .map_err(|e| Error::Storage(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("secret '{name}'")))?;
        let plaintext = self.decrypt(name, &encrypted);
        String::from_utf8(plaintext)
            .map_err(|e| Error::Storage(format!("invalid utf-8 in secret: {e}")))
    }

    /// Delete a secret.
    pub fn delete(&self, name: &str) -> Result<(), Error> {
        self.tree
            .remove(name.as_bytes())
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(())
    }

    /// List all secret names (does not decrypt values).
    pub fn list_names(&self) -> Result<Vec<String>, Error> {
        let mut names = Vec::new();
        for entry in self.tree.iter() {
            let (key, _) = entry.map_err(|e| Error::Storage(e.to_string()))?;
            let name = String::from_utf8(key.to_vec())
                .map_err(|e| Error::Storage(e.to_string()))?;
            names.push(name);
        }
        Ok(names)
    }

    /// Derive a per-key keystream and XOR with data.
    fn encrypt(&self, key_name: &str, data: &[u8]) -> Vec<u8> {
        let keystream = self.derive_keystream(key_name, data.len());
        data.iter().zip(keystream.iter()).map(|(a, b)| a ^ b).collect()
    }

    /// Decryption is symmetric (XOR is its own inverse).
    fn decrypt(&self, key_name: &str, data: &[u8]) -> Vec<u8> {
        self.encrypt(key_name, data)
    }

    /// Produce a keystream of `len` bytes using HMAC-SHA256 in counter mode.
    fn derive_keystream(&self, key_name: &str, len: usize) -> Vec<u8> {
        let mut stream = Vec::with_capacity(len);
        let mut counter: u32 = 0;

        while stream.len() < len {
            let mut mac =
                HmacSha256::new_from_slice(&self.master_key).expect("HMAC accepts any key size");
            mac.update(key_name.as_bytes());
            mac.update(&counter.to_le_bytes());
            let block = mac.finalize().into_bytes();
            stream.extend_from_slice(&block);
            counter += 1;
        }

        stream.truncate(len);
        stream
    }
}
