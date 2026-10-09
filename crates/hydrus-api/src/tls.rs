//! HTTPS for the Client API, as the reference serves it: a cert/key pair in
//! the db directory (`client.crt`, `client.key`), made self-signed on first
//! use, or the user's own pair dropped in its place.
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::rustls::{
    self,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

/// The pair's file names in the db directory.
pub const CERT_FILENAME: &str = "client.crt";
pub const KEY_FILENAME: &str = "client.key";

/// The pair to serve with (`HydrusDB.GetSSLPaths`): the files in `db_dir`
/// when both are there, a new self-signed pair when neither is, and an error
/// when only one is.
pub fn ssl_paths(db_dir: &Path) -> io::Result<(PathBuf, PathBuf)> {
    let cert = db_dir.join(CERT_FILENAME);
    let key = db_dir.join(KEY_FILENAME);
    match (cert.exists(), key.exists()) {
        (true, true) => {}
        (false, false) => {
            tracing::info!("Generating new cert/key files.");
            generate_cert_and_key(&cert, &key)?;
        }
        _ => {
            return Err(io::Error::other(format!(
                "While creating the server database, only one of the paths \"{}\" and \"{}\" existed. You can create a db with these files already in place, but please either delete the existing file (to have hydrus generate its own pair) or find the other in the pair (to use your own).",
                cert.display(),
                key.display()
            )));
        }
    }
    Ok((cert, key))
}

/// Write a self-signed pair as `GenerateOpenSSLCertAndKeyFile` does: RSA 2048,
/// subject and issuer C=HN, O=hydrus network, OU=64 random hex digits, a
/// random serial, valid for ten years from now, for `localhost`, signed with
/// SHA-256; the key unencrypted in the traditional (PKCS#1) form, and both
/// files read-only.
pub fn generate_cert_and_key(cert_path: &Path, key_path: &Path) -> io::Result<()> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType, SerialNumber};
    let fail = |e: rcgen::Error| io::Error::other(format!("Could not make a cert/key pair: {e}"));
    let key = KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256).map_err(fail)?;
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CountryName, "HN");
    name.push(DnType::OrganizationName, "hydrus network");
    name.push(
        DnType::OrganizationalUnitName,
        hex::encode(rand::random::<[u8; 32]>()),
    );
    params.distinguished_name = name;
    params.subject_alt_names = vec![SanType::DnsName("localhost".try_into().map_err(fail)?)];
    // `x509.random_serial_number`: 159 random bits, positive
    let mut serial = rand::random::<[u8; 20]>();
    serial[0] &= 0x7f;
    params.serial_number = Some(SerialNumber::from_slice(&serial));
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(365 * 10);
    let cert = params.self_signed(&key).map_err(fail)?;
    let pkcs1 = pkcs1_from_pkcs8(key.serialized_der())
        .ok_or_else(|| io::Error::other("Could not make a cert/key pair: unexpected key form"))?;
    write_read_only(cert_path, cert.pem().as_bytes())?;
    write_read_only(key_path, pem("RSA PRIVATE KEY", pkcs1).as_bytes())?;
    Ok(())
}

/// Create `path` read-only (0o400 on unix) from the start, so a private key is
/// never readable by others, even briefly; an existing file is an error.
fn write_read_only(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o400);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    drop(file);
    #[cfg(not(unix))]
    {
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn pem(label: &str, der: &[u8]) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for line in encoded.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}

/// One DER element: (tag, contents, what follows).
fn der_element(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = input.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let len = rest[..count]
            .iter()
            .fold(0usize, |len, b| (len << 8) | usize::from(*b));
        (len, &rest[count..])
    };
    (rest.len() >= len).then(|| (tag, &rest[..len], &rest[len..]))
}

/// The RSAPrivateKey inside a PKCS#8 PrivateKeyInfo.
fn pkcs1_from_pkcs8(pkcs8: &[u8]) -> Option<&[u8]> {
    let (0x30, info, _) = der_element(pkcs8)? else {
        return None;
    };
    let (0x02, _, rest) = der_element(info)? else {
        return None;
    };
    let (0x30, _, rest) = der_element(rest)? else {
        return None;
    };
    let (0x04, key, _) = der_element(rest)? else {
        return None;
    };
    Some(key)
}

/// The TLS settings for the pair in `db_dir` (made first if need be).
pub fn server_config(db_dir: &Path) -> io::Result<Arc<rustls::ServerConfig>> {
    let (cert_path, key_path) = ssl_paths(db_dir)?;
    let unreadable = |path: &Path, e: &dyn std::fmt::Display| {
        io::Error::other(format!("{}: {e}", path.display()))
    };
    let certs = CertificateDer::pem_file_iter(&cert_path)
        .map_err(|e| unreadable(&cert_path, &e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| unreadable(&cert_path, &e))?;
    let key = PrivateKeyDer::from_pem_file(&key_path).map_err(|e| unreadable(&key_path, &e))?;
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(io::Error::other)?
    .with_no_client_auth()
    .with_single_cert(certs, key)
    .map_err(io::Error::other)?;
    Ok(Arc::new(config))
}

/// A listener whose connections have finished their TLS handshake. The
/// handshakes run on their own tasks, so a slow or broken client (plain HTTP
/// to this port, say) holds up no one else.
#[derive(Debug)]
pub struct TlsListener {
    ready: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
    local: SocketAddr,
    accepting: tokio::task::JoinHandle<()>,
}

impl TlsListener {
    pub fn new(tcp: TcpListener, config: Arc<rustls::ServerConfig>) -> io::Result<Self> {
        let local = tcp.local_addr()?;
        let acceptor = TlsAcceptor::from(config);
        let (sender, ready) = mpsc::channel(64);
        let accepting = tokio::spawn(async move {
            loop {
                let (stream, addr) = match tcp.accept().await {
                    Ok(accepted) => accepted,
                    Err(e) => {
                        tracing::debug!(error = %e, "accepting a Client API connection failed");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                };
                let acceptor = acceptor.clone();
                let sender = sender.clone();
                tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(30), acceptor.accept(stream))
                        .await
                    {
                        Ok(Ok(tls)) => {
                            let _ = sender.send((tls, addr)).await;
                        }
                        Ok(Err(e)) => tracing::debug!(%addr, error = %e, "TLS handshake failed"),
                        Err(_) => tracing::debug!(%addr, "TLS handshake timed out"),
                    }
                });
            }
        });
        Ok(Self {
            ready,
            local,
            accepting,
        })
    }
}

impl Drop for TlsListener {
    fn drop(&mut self) {
        // closes the port
        self.accepting.abort();
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(ready) => ready,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkcs1_is_unwrapped_from_pkcs8() {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256).unwrap();
        let pkcs1 = pkcs1_from_pkcs8(key.serialized_der()).unwrap();
        // RSAPrivateKey: SEQUENCE { INTEGER 0, modulus, ... }
        let (tag, body, rest) = der_element(pkcs1).unwrap();
        assert_eq!((tag, rest.len()), (0x30, 0));
        assert_eq!(der_element(body).unwrap().1, [0]);
        assert!(matches!(
            PrivateKeyDer::from_pem_slice(pem("RSA PRIVATE KEY", pkcs1).as_bytes()).unwrap(),
            PrivateKeyDer::Pkcs1(_)
        ));
    }
}
