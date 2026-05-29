use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rcgen::generate_simple_self_signed;
use ring::agreement::{EphemeralPrivateKey, ECDH_P256, UnparsedPublicKey, agree_ephemeral};
use ring::rand::SystemRandom;
use ring::digest::{digest, SHA256};

pub fn generate_self_signed_config() -> Result<ServerConfig, anyhow::Error> {
    // Install aws-lc-rs as default crypto provider if not already installed
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let subject_alt_names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "0.0.0.0".to_string(),
        "::1".to_string(),
        "::".to_string(),
    ];

    let cert = generate_simple_self_signed(subject_alt_names)?;
    let cert_der = cert.cert.der().to_vec();
    let key_der = cert.key_pair.serialize_der();

    let certs = vec![CertificateDer::from(cert_der)];
    let key = PrivateKeyDer::Pkcs8(key_der.into());

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(server_config)
}

pub fn generate_ecdh_keypair() -> Result<(EphemeralPrivateKey, Vec<u8>), anyhow::Error> {
    let rng = SystemRandom::new();
    let private_key = EphemeralPrivateKey::generate(&ECDH_P256, &rng)
        .map_err(|_| anyhow::anyhow!("Failed to generate ECDH ephemeral private key"))?;
    let public_key = private_key.compute_public_key()
        .map_err(|_| anyhow::anyhow!("Failed to compute ECDH public key"))?;
    Ok((private_key, public_key.as_ref().to_vec()))
}

pub fn derive_shared_secret(
    private_key: EphemeralPrivateKey,
    peer_public_key_bytes: &[u8],
) -> Result<Vec<u8>, anyhow::Error> {
    let peer_public_key = UnparsedPublicKey::new(&ECDH_P256, peer_public_key_bytes);
    agree_ephemeral(private_key, &peer_public_key, |shared_secret| {
        shared_secret.to_vec()
    })
    .map_err(|_| anyhow::anyhow!("ECDH key agreement failed"))
}

pub fn compute_auth_proof(password: &str, shared_secret: &[u8], salt: &str) -> String {
    let shared_secret_hex: String = shared_secret.iter().map(|b| format!("{:02x}", b)).collect();
    let proof_input = format!("{}:{}:{}:client", password, shared_secret_hex, salt);
    let hash = digest(&SHA256, proof_input.as_bytes());
    hash.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecdh_key_exchange() {
        // Generate key pairs for two parties (Alice and Bob)
        let (alice_priv, alice_pub) = generate_ecdh_keypair().unwrap();
        let (bob_priv, bob_pub) = generate_ecdh_keypair().unwrap();

        // Perform agreement from both sides
        let alice_shared = derive_shared_secret(alice_priv, &bob_pub).unwrap();
        let bob_shared = derive_shared_secret(bob_priv, &alice_pub).unwrap();

        // Verify secrets are identical
        assert_eq!(alice_shared, bob_shared);
        assert_eq!(alice_shared.len(), 32);
    }

    #[test]
    fn test_auth_proof() {
        let password = "my_secure_password";
        let shared_secret = b"derived_shared_diffie_hellman_secret";
        let salt = "random_salt_123";

        let proof = compute_auth_proof(password, shared_secret, salt);
        
        // Re-computing with the same inputs should yield the exact same proof
        let proof_recomputed = compute_auth_proof(password, shared_secret, salt);
        assert_eq!(proof, proof_recomputed);

        // Different password should yield a different proof
        let proof_wrong = compute_auth_proof("wrong_password", shared_secret, salt);
        assert_ne!(proof, proof_wrong);

        // Different salt should yield a different proof
        let proof_diff_salt = compute_auth_proof(password, shared_secret, "different_salt");
        assert_ne!(proof, proof_diff_salt);
    }
}


