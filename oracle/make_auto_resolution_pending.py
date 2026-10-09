#!/usr/bin/env python3
"""Make `legacy_db/auto_resolution_pending.tar.gz`: the auto-resolution
database (`legacy_db/auto_resolution.tar.gz`) with eight more pairs waiting
for approval in the semi-automatic rule that already has two, so the review
window has ten pending pairs (the window asks before approving or denying
more than five, works in chunks of four, and samples up to a fetch limit).

The eight pairs are the rule's first eight pairs that did not match its search
(ordered by media ids), moved to its pending actions as the reference's own
tables hold them (a pair's media ids, and its two files' hash ids with the
first the smaller media's king), with the rule's cached counts moved too.
The pairs need not truly pass the rule's tests: the review window only lists
and acts on them.

Usage: python3 oracle/make_auto_resolution_pending.py
"""
import os, shutil, sqlite3, tarfile, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, 'fixtures', 'legacy_db', 'auto_resolution.tar.gz')
OUT = os.path.join(HERE, 'fixtures', 'legacy_db', 'auto_resolution_pending.tar.gz')
MORE = 8

def main():
    with tempfile.TemporaryDirectory() as work:
        with tarfile.open(SRC) as tar:
            names = tar.getnames()
            tar.extractall(work, filter='data')
        c = sqlite3.connect(os.path.join(work, 'client.db'))
        # the rule that has pending actions already
        (rule,) = c.execute("SELECT rule_id FROM duplicates_files_auto_resolution_rule_count_cache WHERE status = 5 AND status_count > 0 ORDER BY rule_id LIMIT 1").fetchone()
        rows = c.execute(f"SELECT smaller_media_id, larger_media_id FROM duplicate_files_auto_resolution_pair_decisions_{rule}_0 ORDER BY smaller_media_id, larger_media_id LIMIT {MORE}").fetchall()
        assert len(rows) == MORE
        king = dict(c.execute("SELECT media_id, king_hash_id FROM duplicate_files"))
        for (a, b) in rows:
            c.execute(f"DELETE FROM duplicate_files_auto_resolution_pair_decisions_{rule}_0 WHERE smaller_media_id = ? AND larger_media_id = ?", (a, b))
            c.execute(f"INSERT INTO duplicate_files_auto_resolution_pending_actions_{rule} VALUES (?, ?, ?, ?)", (a, b, king[a], king[b]))
        c.execute("UPDATE duplicates_files_auto_resolution_rule_count_cache SET status_count = status_count - ? WHERE rule_id = ? AND status = 0", (MORE, rule))
        c.execute("UPDATE duplicates_files_auto_resolution_rule_count_cache SET status_count = status_count + ? WHERE rule_id = ? AND status = 5", (MORE, rule))
        c.commit()
        c.close()
        with tarfile.open(OUT, 'w:gz') as tar:
            for name in names:
                if name:
                    tar.add(os.path.join(work, name), arcname=name, recursive=False)
    print('wrote', OUT)

if __name__ == '__main__':
    main()
