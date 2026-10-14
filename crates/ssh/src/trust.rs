use myssh_core::trust::HostKey;
use russh::keys::{HashAlg, PublicKey};

/// Fingerprint no formato do OpenSSH (`SHA256:...`, base64 sem padding).
pub fn fingerprint_sha256(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Converte a chave pública do russh para o tipo serializável do core.
pub fn host_key_from_public(key: &PublicKey) -> Option<HostKey> {
    let line = key.to_openssh().ok()?;
    let mut it = line.split_whitespace();
    let (algorithm, blob) = (it.next()?, it.next()?);
    Some(HostKey {
        algorithm: algorithm.to_string(),
        key_base64: blob.to_string(),
        fingerprint_sha256: fingerprint_sha256(key),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKwq2Hgymay7rPv/rivXVOg6uvyh77ygJKMrdrKKhItj";

    #[test]
    fn converte_chave() {
        let k = PublicKey::from_openssh(K).unwrap();
        let hk = host_key_from_public(&k).unwrap();
        assert_eq!(hk.algorithm, "ssh-ed25519");
        assert_eq!(hk.key_base64, K.split(' ').nth(1).unwrap());
        assert!(hk.fingerprint_sha256.starts_with("SHA256:"));
        assert!(!hk.fingerprint_sha256.ends_with('='));
    }
}
