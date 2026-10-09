#!/usr/bin/env python3
"""Record the reference Client API's root page (both welcome pages), its HTTPS
serving (the generated cert/key pair, a dropped-in pair, half a pair), and the
service editor's https / normie-Eris / external-override controls.

Run with: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_client_api_https.py
Writes oracle/fixtures/client_api_https.json."""
import json, os, ssl, stat, sys, tempfile, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
PORT = 45917


def fetch(scheme, path='/'):
    context = None
    if scheme == 'https':
        context = ssl.create_default_context()
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
    deadline = time.time() + 30
    while True:
        try:
            with urllib.request.urlopen(f'{scheme}://127.0.0.1:{PORT}{path}', timeout=10, context=context) as r:
                return {'status': r.status, 'content_type': r.headers.get('Content-Type'), 'headers': sorted(k for k in r.headers.keys() if k != 'Date'), 'body': r.read().decode('utf-8')}
        except OSError:
            if time.time() > deadline:
                raise
            time.sleep(0.2)


def served_cert():
    deadline = time.time() + 30
    while True:
        try:
            return ssl.get_server_certificate(('127.0.0.1', PORT))
        except OSError:
            if time.time() > deadline:
                raise
            time.sleep(0.2)


def describe_cert(pem):
    from cryptography import x509
    from cryptography.x509.oid import NameOID
    cert = x509.load_pem_x509_certificate(pem.encode() if isinstance(pem, str) else pem)
    ou = cert.subject.get_attributes_for_oid(NameOID.ORGANIZATIONAL_UNIT_NAME)[0].value
    san = cert.extensions.get_extension_for_class(x509.SubjectAlternativeName)
    return {
        'country': cert.subject.get_attributes_for_oid(NameOID.COUNTRY_NAME)[0].value,
        'organisation': cert.subject.get_attributes_for_oid(NameOID.ORGANIZATION_NAME)[0].value,
        'unit_hex_length': len(ou),
        'unit_is_hex': all(ch in '0123456789abcdef' for ch in ou),
        'self_issued': cert.issuer == cert.subject,
        'dns_names': san.value.get_values_for_type(x509.DNSName),
        'san_critical': san.critical,
        'valid_days': (cert.not_valid_after_utc - cert.not_valid_before_utc).days,
        'key_bits': cert.public_key().key_size,
        'public_exponent': cert.public_key().public_numbers().e,
        'signature_hash': cert.signature_hash_algorithm.name,
    }


def record(session):
    c = session.controller
    from hydrus.client import ClientConstants as CC, ClientServices
    from hydrus.core import HydrusData

    def set_service(**changes):
        services = []
        for service in c.services_manager.GetServices():
            if service.GetServiceKey() == CC.CLIENT_API_SERVICE_KEY:
                (key, service_type, name, dictionary) = service.ToTuple()
                dictionary.update(changes)
                dictionary['port'] = PORT
                service = ClientServices.GenerateService(key, service_type, name, dictionary)
            services.append(service)
        c.WriteSynchronous('update_services', services)
        deadline = time.time() + 30
        while c.services_manager.GetService(CC.CLIENT_API_SERVICE_KEY).GetPort() != PORT or any(getattr(c.services_manager.GetService(CC.CLIENT_API_SERVICE_KEY), {'use_https': 'UseHTTPS', 'use_normie_eris': 'UseNormieEris', 'allow_non_local_connections': 'AllowsNonLocalConnections'}[k])() != v for (k, v) in changes.items()):
            if time.time() > deadline:
                raise Exception('services manager never saw the change')
            time.sleep(0.1)
        c.RestartClientServerServices()
        time.sleep(1.0)

    db_dir = c.db_dir
    out = {}
    from hydrus.core import HydrusConstants as HC
    out['versions'] = {'software': HC.SOFTWARE_VERSION, 'client_api': HC.CLIENT_API_VERSION}
    set_service(use_https=False, use_normie_eris=False, allow_non_local_connections=False)
    out['eris'] = fetch('http')
    out['files_before_https'] = sorted(n for n in os.listdir(db_dir) if n.startswith('client.') and n.split('.')[-1] in ('crt', 'key'))
    set_service(use_normie_eris=True)
    out['normie_eris'] = fetch('http')
    set_service(use_normie_eris=False, allow_non_local_connections=True)
    out['eris_non_local'] = fetch('http')
    set_service(use_normie_eris=True)
    out['normie_eris_non_local'] = fetch('http')
    set_service(use_normie_eris=False, allow_non_local_connections=False, use_https=True)
    out['https_eris'] = fetch('https')
    cert_path = os.path.join(db_dir, 'client.crt')
    key_path = os.path.join(db_dir, 'client.key')
    with open(key_path) as f:
        key_pem = f.read()
    with open(cert_path) as f:
        cert_pem = f.read()
    out['generated'] = {
        'files': sorted(n for n in os.listdir(db_dir) if n.startswith('client.') and n.split('.')[-1] in ('crt', 'key')),
        'cert': describe_cert(cert_pem),
        'key_pem_header': key_pem.splitlines()[0],
        'cert_mode': oct(stat.S_IMODE(os.stat(cert_path).st_mode)),
        'key_mode': oct(stat.S_IMODE(os.stat(key_path).st_mode)),
        'served_is_generated': served_cert().strip() == cert_pem.strip(),
    }
    # plain http to an https port is refused
    try:
        fetch_plain = urllib.request.urlopen(f'http://127.0.0.1:{PORT}/', timeout=5)
        out['http_to_https_port'] = {'status': fetch_plain.status}
    except Exception as e:
        out['http_to_https_port'] = {'error': type(e).__name__}
    # a pair the user dropped in is used as it is
    from cryptography import x509
    from cryptography.x509.oid import NameOID
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import rsa
    import datetime
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'dropped.example')])
    now = datetime.datetime.now(datetime.timezone.utc)
    dropped = x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key()).serial_number(1).not_valid_before(now).not_valid_after(now + datetime.timedelta(days=30)).sign(key, hashes.SHA256())
    for path in (cert_path, key_path):
        os.chmod(path, 0o600)
        os.remove(path)
    with open(cert_path, 'wb') as f:
        f.write(dropped.public_bytes(serialization.Encoding.PEM))
    with open(key_path, 'wb') as f:
        f.write(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    c.RestartClientServerServices()
    time.sleep(1.0)
    served = x509.load_pem_x509_certificate(served_cert().encode())
    out['dropped_in'] = {'served_common_name': served.subject.get_attributes_for_oid(NameOID.COMMON_NAME)[0].value, 'page': fetch('https')['status']}
    # half a pair: the service does not start, and says why
    shown = []
    old_text, old_exception = HydrusData.ShowText, HydrusData.ShowException
    HydrusData.ShowText = lambda text, *a, **k: shown.append(str(text))
    HydrusData.ShowException = lambda e, *a, **k: shown.append(str(e))
    os.remove(key_path)
    try:
        c.RestartClientServerServices()
        time.sleep(2.0)
    finally:
        HydrusData.ShowText, HydrusData.ShowException = old_text, old_exception
    out['half_pair'] = {'shown': [s.replace(db_dir, '{DB_DIR}') for s in shown], 'files': sorted(n for n in os.listdir(db_dir) if n.startswith('client.') and n.split('.')[-1] in ('crt', 'key'))}
    os.remove(cert_path)

    def editor():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.services import ClientGUIClientsideServices as S
        service = c.services_manager.GetService(CC.CLIENT_API_SERVICE_KEY)
        result = {}
        for (label, changes) in (('off', {'port': None}), ('on', {'port': 45869, 'use_https': True, 'use_normie_eris': True, 'external_scheme_override': 'https', 'external_host_override': 'example.com', 'external_port_override': ''})):
            dictionary = service.GetSerialisableDictionary()
            dictionary.update(changes)
            panel = S.EditServiceClientServerSubPanel(c.gui, HC.CLIENT_API_SERVICE, dictionary)
            box = panel._client_server_options_panel
            grid = box.layout()
            labels = [w.text() for w in box.findChildren(QW.QLabel) if w.text()]
            controls = {
                'run': panel._run_the_service, 'port': panel._port, 'non_local': panel._allow_non_local_connections,
                'https': panel._use_https, 'cors': panel._support_cors, 'logs': panel._log_requests,
                'normie': panel._use_normie_eris, 'scheme': panel._external_scheme_override,
                'host': panel._external_host_override, 'external_port': panel._external_port_override,
            }
            state = {k: {'enabled': w.isEnabled(), 'hidden': w.isHidden(), 'tooltip': w.toolTip()} for (k, w) in controls.items()}
            for k in ('scheme', 'host', 'external_port'):
                w = controls[k]
                state[k].update({'text': w._text.text(), 'none_checked': w._checkbox.isChecked(), 'none_phrase': w._checkbox.text(), 'text_enabled': w._text.isEnabled(), 'value': w.GetValue()})
            panel._run_the_service.setChecked(label == 'off')
            panel._UpdateControls()
            toggled = {k: w.isEnabled() for (k, w) in controls.items()}
            value = panel.GetValue()
            value.pop('bandwidth_rules')
            result[label] = {'labels': labels, 'controls': state, 'enabled_after_toggle': toggled, 'value': value}
            panel.deleteLater()
        return result

    out['editor'] = c.CallBlockingToQt(c.gui, editor)
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/client_api_https.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote client_api_https.json')


if __name__ == '__main__':
    main()
