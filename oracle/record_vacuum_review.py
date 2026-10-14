#!/usr/bin/env python3
"""Record the reference's review vacuum data panel (`ReviewVacuumData`).

The real panel is made with synthetic database sizes (so the numbers do not depend on the
machine): each row's cells, whether vacuuming it is allowed and why not, the question for
one row and for several, and what is written when it is accepted. The free space of the
disk the files are said to be on is recorded, so a replay can be given the same.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusPaths, HydrusTime
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview as R
        root = tempfile.mkdtemp()
        now = HydrusTime.GetNow()
        free = HydrusPaths.GetFreeSpace(root)
        data = {
            'client.db': dict(path=os.path.join(root, 'client.db'), page_size=4096, page_count=1000, freelist_count=100, last_vacuumed_ms=None),
            'client.mappings.db': dict(path=os.path.join(root, 'client.mappings.db'), page_size=4096, page_count=5_000_000, freelist_count=50_000, last_vacuumed_ms=(now - 3 * 86400) * 1000),
            'client.caches.db': dict(path=os.path.join(root, 'client.caches.db'), page_size=16384, page_count=1000, freelist_count=0, last_vacuumed_ms=0),
            'client.master.db': dict(path=os.path.join(root, 'client.master.db'), page_size=4096, page_count=10 ** 10, freelist_count=0, last_vacuumed_ms=(now - 7200) * 1000),
        }
        names_to_pretty = {}
        log = []
        answers = []
        writes = []
        old = (Q.GetYesNo, c.Write)

        def yesno(parent, text, yes_label='yes', no_label='no', *a, **k):
            log.append(['question', text, yes_label, no_label])
            return QW.QDialog.DialogCode.Accepted if answers.pop(0) else QW.QDialog.DialogCode.Rejected

        def write(name, *args, **k):
            if name == 'vacuum':
                writes.append(['vacuum', sorted(args[0])])
                return None
            return old[1](name, *args, **k)
        Q.GetYesNo, c.Write = yesno, write
        try:
            panel = R.ReviewVacuumData(c.gui, c, data)
            panel._OKParent = lambda: log.append(['closed'])
            texts = sorted({l.text() for l in panel.findChildren(QW.QLabel) if l.text()})
            rows = {}
            for name in data:
                rows[name] = list(panel._ConvertNameToDisplayTuple(name))
            listctrl = panel._vacuum_listctrl
            steps = []

            def select(*names):
                listctrl.SelectDatas([n for n in listctrl.GetData() if n in names], deselect_others=True)

            def snap(step, names, can=None):
                steps.append(dict(step=step, selected=list(names), can_vacuum=panel._CanVacuum(), dialogs=list(log), writes=list(writes)))
                log.clear()
                writes.clear()
            select()
            snap('nothing selected', [])
            for name in data:
                select(name)
                snap(f'select {name}', [name])
            select('client.db')
            answers[:] = [False]
            panel._Vacuum()
            snap('vacuum client.db, forget it', ['client.db'])
            answers[:] = [True]
            panel._Vacuum()
            snap('vacuum client.db, do it', ['client.db'])
            select('client.db', 'client.caches.db')
            answers[:] = [True]
            panel._Vacuum()
            snap('vacuum two files, do it', ['client.db', 'client.caches.db'])
            select('client.mappings.db')
            answers[:] = [True]
            panel._Vacuum()
            snap('vacuum the 19 GB file, do it', ['client.mappings.db'])
            select('client.master.db')
            snap('select the one too big for the disk', ['client.master.db'])
        finally:
            Q.GetYesNo, c.Write = old
        return dict(now=now, free=free, data={k: {**v, 'path': 'DIR/' + os.path.basename(v['path'])} for k, v in data.items()}, texts=texts, rows=rows, steps=steps)
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'vacuum_review.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the vacuum review')


if __name__ == '__main__':
    main()
