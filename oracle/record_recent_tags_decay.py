#!/usr/bin/env python3
"""Record the reference's recent tags: newest first, and decayed on read.

Five tags are pushed (in order, a moment apart) to a local tag service. With
`num_recent_tags` 3 the read returns the newest three and deletes the rest; the
count is then raised to 10 (the deleted two do not come back) and set to none
(the read keeps 20). Clearing (`push_recent_tags` with None) empties the list.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'recent_tags_decay.json')

def record(session):
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    tags = ['recent:a', 'recent:b', 'recent:c', 'recent:d', 'recent:e']
    for tag in tags:
        c.WriteSynchronous('push_recent_tags', local, [tag])
        time.sleep(0.05)
    steps = []
    def read(label, count):
        c.new_options.SetNoneableInteger('num_recent_tags', count)
        steps.append(dict(step=label, count=count, tags=list(c.Read('recent_tags', local))))
    read('three', 3)
    read('ten after three', 10)
    read('none', None)
    c.WriteSynchronous('push_recent_tags', local, None)
    read('after clear', 10)
    return dict(pushed=tags, steps=steps)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
