#!/usr/bin/env python3
"""Record the preparation tab's "resync potential pairs to local physical
storage" (`PreparationPanel._ResyncPotentialPairsToHydrusLocalFileStorage`,
the database's `ResyncPotentialPairsToHydrusLocalFileStorage`).

On the database `record_auto_resolution.py` left, before the client starts,
three files that have potential pairs (the two of the first pair and one more)
are taken out of the local file tables (hydrus local file storage, all local
media and "local files"): files whose pairs the reference once forgot to delist.
Then, with time held still, the real preparation panel's button is pressed
twice, the question answered no and yes. The record has the orphan files,
the question, the potential pairs by king hash before and after each press,
and the popup job's title and final text.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_potential_pairs_resync.py
       (writes fixtures/potential_pairs_resync.json)
"""
import json, os, sqlite3, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'potential_pairs_resync.json')


def orphan_kings(db_dir):
    """The kings (hash ids) to orphan: both files of the first pair, and the
    smaller one of the fifth pair, and their hex."""
    c = sqlite3.connect(os.path.join(db_dir, 'client.db'))
    c.execute("ATTACH ? AS m", (os.path.join(db_dir, 'client.master.db'),))
    pairs = c.execute("SELECT smaller_media_id, larger_media_id FROM potential_duplicate_pairs ORDER BY smaller_media_id, larger_media_id").fetchall()
    media = [pairs[0][0], pairs[0][1], pairs[4][0]]
    media = list(dict.fromkeys(media))
    kings = [c.execute("SELECT king_hash_id FROM duplicate_files WHERE media_id = ?", (m,)).fetchone()[0] for m in media]
    hexes = [c.execute("SELECT hex(hash) FROM m.hashes WHERE hash_id = ?", (k,)).fetchone()[0].lower() for k in kings]
    c.close()
    return kings, hexes


def orphan(db_dir, kings):
    c = sqlite3.connect(os.path.join(db_dir, 'client.db'))
    for table in ('current_files_4', 'current_files_5', 'current_files_6'):
        for k in kings:
            c.execute(f"DELETE FROM {table} WHERE hash_id = ?", (k,))
    c.commit()
    c.close()


def record(session, db_dir, hexes):
    c = session.controller
    gui = c.gui
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: 1900000000
    HydrusTime.GetNowMS = lambda: 1900000000000
    manager = c.duplicates_auto_resolution_manager
    manager._AbleToWorkIdleNormal = lambda: False
    manager._AbleToWorkActiveNormal = lambda: False
    asked = []
    answers = []
    jobs = []

    def setup():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsQuick

        def yes_no(win, message, **kwargs):
            wanted = answers.pop(0)
            asked.append({'message': message, 'pressed': wanted})
            return QW.QDialog.DialogCode.Accepted if wanted == 'yes' else QW.QDialog.DialogCode.Rejected

        ClientGUIDialogsQuick.GetYesNo = yes_no

    c.CallBlockingToQt(gui, setup)
    real_pub = c.pub

    def pub(topic, *args, **kwargs):
        if topic == 'message':
            jobs.append(args[0])
        return real_pub(topic, *args, **kwargs)

    c.pub = pub

    def pairs():
        db = sqlite3.connect('file:' + os.path.join(db_dir, 'client.db') + '?mode=ro', uri=True)
        db.execute("ATTACH ? AS m", (os.path.join(db_dir, 'client.master.db'),))
        rows = db.execute(
            "SELECT hex(h1.hash), hex(h2.hash) FROM potential_duplicate_pairs p "
            "JOIN duplicate_files f1 ON f1.media_id = p.smaller_media_id JOIN m.hashes h1 ON h1.hash_id = f1.king_hash_id "
            "JOIN duplicate_files f2 ON f2.media_id = p.larger_media_id JOIN m.hashes h2 ON h2.hash_id = f2.king_hash_id").fetchall()
        db.close()
        return sorted([a.lower(), b.lower()] for (a, b) in rows)

    from hydrus.client.gui.pages import ClientGUISidebarDuplicates as S
    panel = c.CallBlockingToQt(gui, lambda: S.PreparationPanel(gui))
    out = {'orphans': hexes, 'before': pairs(), 'presses': []}
    for answer in ('no', 'yes'):
        del asked[:]
        del jobs[:]
        answers[:] = [answer]
        c.CallBlockingToQt(gui, panel._ResyncPotentialPairsToHydrusLocalFileStorage)
        time.sleep(3)
        c.db.ForceACommit()
        time.sleep(2)
        out['presses'].append({
            'answer': answer, 'asked': list(asked), 'pairs': pairs(),
            'jobs': [{'title': j.GetStatusTitle(), 'text': j.GetStatusText()} for j in jobs],
        })
    return out


def main():
    import hydrus_driver, record_api, tempfile
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        db_dir = record_api.unpack_fixture('auto_resolution')
        kings, hexes = orphan_kings(db_dir)
        orphan(db_dir, kings)
        result = hydrus_driver.run_client(db_dir, lambda s: record(s, db_dir, hexes))
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    os.environ['TZ'] = 'UTC'
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(len(result['before']), [(p['answer'], len(p['pairs']), p['jobs']) for p in result['presses']])


if __name__ == '__main__':
    main()
