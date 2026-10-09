#!/usr/bin/env python3
"""Record the reference's "Set a new mpv.conf on dialog ok?" option.

A private basic-fixture client builds the real `MediaPlaybackPanel`, sets its
mpv.conf path box, runs `UpdateOptions` (as the options dialog's OK does), and
records what is at the database's `mpv.conf` afterwards: its bytes (as hex),
whether its modification time (whole seconds) is the source's, and whether it
is read-only. Cases: no mpv.conf there yet; one with other content; an empty
source; bytes that are not text; CRLF text; a large source; a read-only
mpv.conf; a destination with the source's size and time but other content (the
mirror leaves it alone); one with the source's size but another time (it is
replaced); a path that is a folder, that does not exist, that is blank or only
spaces; and a cancelled dialog (the panel never told to update).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_mpv_conf.py
"""
import json
import os
import stat
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'mpv_conf.json')

SOURCE_TIME = 1700000000
OTHER_TIME = 1600000000

# ( name, source bytes or a special string, existing mpv.conf bytes or None, how the destination is prepared )
CASES = [
    ('fresh', b'volume-max=150\n', None, 'plain'),
    ('replaces', b'volume-max=150\n', b'something else\n', 'plain'),
    ('empty source', b'', b'was here\n', 'plain'),
    ('not text', bytes(range(256)), b'was here\n', 'plain'),
    ('crlf', b'a=1\r\nb=2\r\n', None, 'plain'),
    ('large', (b'# comment line of the mpv conf\n' * 4000), b'x', 'plain'),
    ('read only', b'volume-max=150\n', b'old\n', 'readonly'),
    ('same size and time', b'volume-max=150\n', b'volume-max=999\n', 'same_time'),
    ('same size other time', b'volume-max=150\n', b'volume-max=999\n', 'other_time'),
    ('a folder', 'folder', b'was here\n', 'plain'),
    ('missing', 'missing', b'was here\n', 'plain'),
    ('blank', 'blank', b'was here\n', 'plain'),
    ('spaces', 'spaces', b'was here\n', 'plain'),
    ('cancelled', b'volume-max=150\n', b'was here\n', 'cancel'),
]


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel

    dest = controller.GetMPVConfPath()
    work = tempfile.mkdtemp(prefix='mpv-conf-')
    results = []
    for (name, source, existing, how) in CASES:
        if os.path.exists(dest):
            os.chmod(dest, 0o644)
            os.remove(dest)
        if existing is not None:
            with open(dest, 'wb') as f:
                f.write(existing)
        path = os.path.join(work, 'source-{}.conf'.format(len(results)))
        if isinstance(source, bytes):
            with open(path, 'wb') as f:
                f.write(source)
            os.utime(path, (SOURCE_TIME, SOURCE_TIME))
        elif source == 'folder':
            os.mkdir(path)
        elif source == 'missing':
            pass
        elif source == 'blank':
            path = ''
        elif source == 'spaces':
            path = '   '
        if how == 'readonly':
            os.chmod(dest, 0o444)
        elif how == 'same_time':
            os.utime(dest, (SOURCE_TIME, SOURCE_TIME))
        elif how == 'other_time':
            os.utime(dest, (OTHER_TIME, OTHER_TIME))

        def run():
            panel = MediaPlaybackPanel(gui)
            panel._mpv_conf_path.SetPath(path)
            if how != 'cancel':
                panel.UpdateOptions()
            panel.deleteLater()

        qt(run)
        if os.path.exists(dest):
            st = os.stat(dest)
            with open(dest, 'rb') as f:
                data = f.read()
            outcome = {'exists': True, 'hex': data.hex(), 'source_time': int(st.st_mtime) == SOURCE_TIME, 'readonly': not (st.st_mode & stat.S_IWUSR)}
        else:
            outcome = {'exists': False}
        results.append({
            'name': name,
            'source': source.hex() if isinstance(source, bytes) else source,
            'existing': existing.hex() if existing is not None else None,
            'how': how,
            'outcome': outcome,
        })
        if os.path.exists(dest):
            os.chmod(dest, 0o644)
    return {'cases': results, 'source_time': SOURCE_TIME, 'other_time': OTHER_TIME}


def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
