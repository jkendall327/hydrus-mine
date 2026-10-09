//! The Client API's root page and HTTPS, replayed against the reference's
//! recording (`oracle/record_client_api_https.py`): both welcome pages, the
//! self-signed pair made in the db directory, a pair the user dropped in, and
//! half a pair.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_store::services::{ServerConfig, ServiceKind, update_config};
use hydrus_store::settings::{self, ClientApiState, ClientApiStatus};
use serde_json::Value;

const HYDRUS: &str = env!("CARGO_BIN_EXE_hydrus");

struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn store() -> (tempfile::TempDir, PathBuf) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("store");
    let imported = Command::new(HYDRUS)
        .arg("import-legacy")
        .arg(legacy.path())
        .arg(&dir)
        .args(["--files", "copy"])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(imported.success());
    (parent, dir)
}

fn unused_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// (status line, headers, body) of `GET /` over `stream`.
fn get(
    mut stream: impl std::io::Read + std::io::Write,
) -> std::io::Result<(String, String, String)> {
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut bytes = Vec::new();
    // a peer that closes without TLS's close_notify still sent everything
    if let Err(e) = stream.read_to_end(&mut bytes)
        && (bytes.is_empty() || e.kind() != std::io::ErrorKind::UnexpectedEof)
    {
        return Err(e);
    }
    if bytes.is_empty() {
        return Err(std::io::Error::other("no answer"));
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let Some((head, body)) = text
        .split_once("\r\n\r\n")
        .filter(|_| text.starts_with("HTTP/"))
    else {
        return Err(std::io::Error::other("not an HTTP answer"));
    };
    let (status, headers) = head.split_once("\r\n").unwrap();
    Ok((status.into(), headers.to_lowercase(), body.into()))
}

fn http(port: u16) -> std::io::Result<(String, String, String)> {
    get(TcpStream::connect(("127.0.0.1", port))?)
}

/// `GET /` over TLS, trusting only `cert` and checking it names `localhost`.
fn https(port: u16, cert: &Path) -> std::io::Result<(String, String, String)> {
    use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_file(cert).unwrap())
        .unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connection =
        rustls::ClientConnection::new(Arc::new(config), ServerName::try_from("localhost").unwrap())
            .unwrap();
    let tcp = TcpStream::connect(("127.0.0.1", port))?;
    tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
    get(rustls::StreamOwned::new(connection, tcp))
}

fn content_type(headers: &str) -> String {
    headers
        .lines()
        .find_map(|l| l.strip_prefix("content-type: "))
        .unwrap()
        .into()
}

/// The recorded `describe_cert` of a PEM file.
fn describe(cert: &Path) -> Value {
    use x509_parser::prelude::*;
    use x509_parser::public_key::PublicKey;
    let pem = std::fs::read(cert).unwrap();
    let (_, pem) = parse_x509_pem(&pem).unwrap();
    let cert = pem.parse_x509().unwrap();
    let subject = cert.subject();
    let first = |it: &mut dyn Iterator<Item = &AttributeTypeAndValue<'_>>| {
        it.next().unwrap().as_str().unwrap().to_owned()
    };
    let unit = first(&mut subject.iter_organizational_unit());
    let san = cert.subject_alternative_name().unwrap().unwrap();
    let names: Vec<String> = san
        .value
        .general_names
        .iter()
        .filter_map(|n| match n {
            GeneralName::DNSName(name) => Some((*name).to_owned()),
            _ => None,
        })
        .collect();
    let validity = cert.validity();
    let PublicKey::RSA(rsa) = cert.public_key().parsed().unwrap() else {
        panic!("RSA")
    };
    serde_json::json!({
        "country": first(&mut subject.iter_country()),
        "organisation": first(&mut subject.iter_organization()),
        "unit_hex_length": unit.len(),
        "unit_is_hex": unit.chars().all(|c| "0123456789abcdef".contains(c)),
        "self_issued": cert.issuer() == cert.subject(),
        "dns_names": names,
        "san_critical": san.critical,
        "valid_days": (validity.not_after.timestamp() - validity.not_before.timestamp()) / 86_400,
        "key_bits": rsa.key_size(),
        "public_exponent": rsa.try_exponent().unwrap(),
        "signature_hash": match cert.signature_algorithm.algorithm.to_id_string().as_str() {
            "1.2.840.113549.1.1.11" => "sha256",
            other => panic!("signed with {other}"),
        },
    })
}

fn mode(path: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!(
        "0o{:o}",
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
    )
}

fn pair(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n == "client.crt" || n == "client.key")
        .collect();
    names.sort();
    names
}

fn remove(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::remove_file(path).unwrap();
}

// leaf: audit-media-services-missing-listener-unsupported
#[test]
fn welcome_pages_and_https_serve_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("client_api_https.json");
    assert_eq!(
        recorded["versions"]["software"],
        hydrus_core::REFERENCE_VERSION
    );
    assert_eq!(
        recorded["versions"]["client_api"],
        hydrus_core::CLIENT_API_VERSION
    );
    let (_parent, dir) = store();
    let editor = hydrus_store::Store::open(&dir).unwrap();
    let api = editor
        .snapshot()
        .services
        .of_type(hydrus_core::ServiceType::ClientApiService)
        .next()
        .unwrap()
        .clone();
    let update = |config: &ServerConfig| {
        let (id, config) = (api.id, config.clone());
        editor
            .write(move |ctx| update_config(ctx.conn(), id, &ServiceKind::ClientApi(config)))
            .unwrap();
    };
    let mut config = ServerConfig {
        port: Some(unused_port()),
        ..ServerConfig::default()
    };
    update(&config);
    let mut serving = Serving(
        Command::new(HYDRUS)
            .arg("serve")
            .arg(&dir)
            .arg("--attached")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let pid = serving.0.id();
    // apply `config` on a new port, and wait for the daemon to report it
    let apply = |config: &mut ServerConfig| -> ClientApiState {
        config.port = Some(unused_port());
        update(config);
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status: ClientApiStatus = editor.read(settings::get).unwrap();
            if status.pid == pid {
                match &status.state {
                    ClientApiState::Listening(addr)
                        if addr.ends_with(&format!(":{}", config.port.unwrap())) =>
                    {
                        return status.state;
                    }
                    ClientApiState::Failed(_) => return status.state,
                    _ => {}
                }
            }
            assert!(Instant::now() < deadline, "listener state: {status:?}");
            std::thread::sleep(Duration::from_millis(40));
        }
    };
    let page = |which: &str, got: (String, String, String)| {
        let expected = &recorded[which];
        assert_eq!(got.0, "HTTP/1.1 200 OK", "{which}");
        assert_eq!(
            content_type(&got.1),
            expected["content_type"].as_str().unwrap().to_lowercase(),
            "{which}"
        );
        assert!(got.1.contains("content-disposition: inline"), "{which}");
        assert_eq!(got.2, expected["body"].as_str().unwrap(), "{which}");
    };
    for (which, non_local, normie) in [
        ("eris", false, false),
        ("normie_eris", false, true),
        ("eris_non_local", true, false),
        ("normie_eris_non_local", true, true),
    ] {
        config.allow_non_local_connections = non_local;
        config.use_normie_eris = normie;
        apply(&mut config);
        page(which, http(config.port.unwrap()).unwrap());
    }
    // no pair until HTTPS asks for one
    assert_eq!(
        serde_json::json!(pair(&dir)),
        recorded["files_before_https"]
    );
    config.allow_non_local_connections = false;
    config.use_normie_eris = false;
    config.use_https = true;
    assert!(matches!(apply(&mut config), ClientApiState::Listening(_)));
    let port = config.port.unwrap();
    let (cert, key) = (dir.join("client.crt"), dir.join("client.key"));
    let generated = &recorded["generated"];
    assert_eq!(serde_json::json!(pair(&dir)), generated["files"]);
    assert_eq!(describe(&cert), generated["cert"]);
    assert_eq!(
        std::fs::read_to_string(&key)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
        generated["key_pem_header"]
    );
    assert_eq!(mode(&cert), generated["cert_mode"]);
    assert_eq!(mode(&key), generated["key_mode"]);
    // served with the generated cert: a client trusting only it connects
    assert_eq!(generated["served_is_generated"], true);
    page("https_eris", https(port, &cert).unwrap());
    // plain HTTP to the HTTPS port gets no answer
    assert_eq!(
        recorded["http_to_https_port"]["error"],
        "RemoteDisconnected"
    );
    let plain = http(port);
    assert!(
        plain
            .as_ref()
            .map_or(true, |(status, ..)| !status.starts_with("HTTP/")),
        "{plain:?}"
    );
    // a pair the user drops in is served as it is
    let other = tempfile::tempdir().unwrap();
    hydrus_api::tls::generate_cert_and_key(
        &other.path().join("client.crt"),
        &other.path().join("client.key"),
    )
    .unwrap();
    let generated_cert = std::fs::read(&cert).unwrap();
    remove(&cert);
    remove(&key);
    std::fs::copy(other.path().join("client.crt"), &cert).unwrap();
    std::fs::copy(other.path().join("client.key"), &key).unwrap();
    assert_eq!(recorded["dropped_in"]["page"], 200);
    assert!(matches!(apply(&mut config), ClientApiState::Listening(_)));
    let port = config.port.unwrap();
    page("https_eris", https(port, &cert).unwrap());
    let stale = other.path().join("generated.crt");
    std::fs::write(&stale, generated_cert).unwrap();
    assert!(
        https(port, &stale).is_err(),
        "the dropped-in cert replaced the generated one"
    );
    assert_eq!(pair(&dir), ["client.crt", "client.key"]);
    // half a pair: the Client API doesn't start, says why, and makes nothing
    remove(&key);
    let half = &recorded["half_pair"];
    let ClientApiState::Failed(why) = apply(&mut config) else {
        panic!("started with half a pair")
    };
    let shown = half["shown"].as_array().unwrap();
    assert_eq!(
        why,
        format!(
            "{} {}",
            shown[0].as_str().unwrap(),
            shown[1]
                .as_str()
                .unwrap()
                .replace("{DB_DIR}", dir.to_str().unwrap())
        )
    );
    assert_eq!(serde_json::json!(pair(&dir)), half["files"]);
    drop(serving.0.stdin.take());
    let started = Instant::now();
    while serving.0.try_wait().unwrap().is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "daemon did not stop"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
