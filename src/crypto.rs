use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rcgen::generate_simple_self_signed;

pub fn generate_self_signed_config() -> Result<ServerConfig, anyhow::Error> {
    // Install aws-lc-rs as default crypto provider if not already installed
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let subject_alt_names = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "0.0.0.0".to_string(),
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
