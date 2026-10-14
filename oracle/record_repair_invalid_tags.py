#!/usr/bin/env python3
"""Record the reference's Database > check and repair > fix invalid tags.

The basic fixture's master database is given some invalid tags before the client boots
(uppercase, stray spaces, one that cleans to nothing, one that collides once cleaned, a bad
namespace); `ClientGUI._RepairInvalidTags` is then run with its question answered, as is the
reference's own `repair_invalid_tags`, and the popup's text and the tags afterwards are
recorded.
"""
import json
import os
import sqlite3
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

# (namespace, subtag), as stored
BAD = [('', 'Mixed Case'), ('', 'trailing space '), ('', 'safe'), ('', 'SAFE'), ('', ' '), ('Creator', 'Foo'), ('', 'a  b')]


def seed(db_dir):
    conn = sqlite3.connect(os.path.join(db_dir, 'client.master.db'))
    for namespace, subtag in BAD:
        row = conn.execute('SELECT namespace_id FROM namespaces WHERE namespace = ?', (namespace,)).fetchone()
        namespace_id = row[0] if row else conn.execute('INSERT INTO namespaces (namespace) VALUES (?)', (namespace,)).lastrowid
        row = conn.execute('SELECT subtag_id FROM subtags WHERE subtag = ?', (subtag,)).fetchone()
        subtag_id = row[0] if row else conn.execute('INSERT INTO subtags (subtag) VALUES (?)', (subtag,)).lastrowid
        if not conn.execute('SELECT 1 FROM tags WHERE namespace_id = ? AND subtag_id = ?', (namespace_id, subtag_id)).fetchone():
            conn.execute('INSERT INTO tags (namespace_id, subtag_id) VALUES (?, ?)', (namespace_id, subtag_id))
    conn.commit()
    conn.close()


def tags(db_dir):
    conn = sqlite3.connect(os.path.join(db_dir, 'client.master.db'))
    out = sorted((tag_id, namespace, subtag) for tag_id, namespace, subtag in conn.execute(
        'SELECT t.tag_id, n.namespace, s.subtag FROM tags t JOIN namespaces n USING (namespace_id) JOIN subtags s USING (subtag_id)'))
    conn.close()
    return out


def record(session, db_dir):
    c = session.controller

    def work():
        from qtpy import QtTest as QT, QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        steps = []
        asked = []
        jobs = []
        answers = []
        old = (Q.GetYesNo, c.pub)

        def yesno(parent, text, yes_label='yes', no_label='no', *a, **k):
            asked.append(['question', text, yes_label, no_label])
            return QW.QDialog.DialogCode.Accepted if answers.pop(0) else QW.QDialog.DialogCode.Rejected

        def pub(topic, *args, **k):
            if topic == 'message' and args and args[0].GetStatusTitle() == 'repairing invalid tags':
                jobs.append(args[0])
                return None
            return old[1](topic, *args, **k)
        Q.GetYesNo, c.pub = yesno, pub
        try:
            before = tags(db_dir)
            for name, answer in (('forget it', False), ('do it', True)):
                answers[:] = [answer]
                asked.clear()
                jobs.clear()
                c.gui._RepairInvalidTags()
                start = time.monotonic()
                texts = []
                while time.monotonic() - start < 60:
                    QT.QTest.qWait(50)
                    if jobs:
                        text = jobs[0].GetStatusText(1) or ''
                        if not texts or texts[-1] != text:
                            texts.append(text)
                        if jobs[0].IsDone():
                            break
                    elif not answer:
                        break
                QT.QTest.qWait(500)
                steps.append(dict(step=name, asked=list(asked), statuses=texts, tags=tags(db_dir)))
        finally:
            Q.GetYesNo, c.pub = old
        return dict(before=before, bad=BAD, steps=steps)
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        db_dir = record_api.unpack_fixture('basic')
        seed(db_dir)
        value = hydrus_driver.run_client(db_dir, lambda session: record(session, db_dir))
        value['after_shutdown'] = tags(db_dir)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'repair_invalid_tags.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded fixing invalid tags')


if __name__ == '__main__':
    main()
