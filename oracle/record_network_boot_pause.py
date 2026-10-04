#!/usr/bin/env python3
"""Record the real pause menu, saved option round-trip, and reference boot branch."""
import ast
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    from hydrus.core import HydrusGlobals as HG, HydrusSerialisable as S
    from hydrus.client import ClientGlobals as CG
    source = os.path.join(HERE, '..', 'hydrus', 'client', 'ClientController.py')
    tree = ast.parse(open(source).read())
    branch = next(n for n in ast.walk(tree) if isinstance(n, ast.If) and 'boot_with_network_traffic_paused_command_line' in ast.unparse(n.test))
    code = compile(ast.fix_missing_locations(ast.Module(body=[branch], type_ignores=[])), source, 'exec')
    saved = {k: c.new_options.GetBoolean(k) for k in ('advanced_mode', 'boot_with_network_traffic_paused', 'pause_all_new_network_traffic')}
    command = HG.boot_with_network_traffic_paused_command_line
    out = {'menus': [], 'steps': [], 'boot': []}
    def snapshot(name):
        out['steps'].append({'action': name, 'preference': c.new_options.GetBoolean('boot_with_network_traffic_paused'), 'paused': c.new_options.GetBoolean('pause_all_new_network_traffic')})
    def actions():
        menu, _ = c.gui._InitialiseMenuInfoNetwork()
        pause = next(a.menu() for a in menu.actions() if a.text() == 'pause')
        boot = next(a for a in pause.actions() if a.text() == 'always boot the client with paused network traffic')
        live = next(a for a in pause.actions() if a.text() == 'all new network traffic')
        return menu, boot, live
    def run():
        try:
            HG.boot_with_network_traffic_paused_command_line = False
            c.new_options.SetBoolean('boot_with_network_traffic_paused', False)
            c.new_options.SetBoolean('pause_all_new_network_traffic', False)
            for advanced in (False, True):
                c.new_options.SetBoolean('advanced_mode', advanced)
                menu, boot, _ = actions()
                out['menus'].append({'advanced': advanced, 'label': boot.text(), 'checkable': boot.isCheckable(), 'checked': boot.isChecked(), 'enabled': boot.isEnabled()})
                menu.deleteLater()
            menu, boot, _ = actions(); boot.trigger(); snapshot('enable boot preference'); menu.deleteLater()
            reopened = S.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
            out['round_trip_preference'] = reopened.GetBoolean('boot_with_network_traffic_paused')
            exec(code, {'CG': CG, 'HG': HG, 'self': c}); snapshot('boot')
            menu, boot, live = actions()
            out['reopened_checked'] = boot.isChecked()
            live.trigger(); snapshot('resume traffic'); menu.deleteLater()
            menu, boot, _ = actions(); boot.trigger(); snapshot('disable boot preference'); menu.deleteLater()
            for preference, paused in ((False, False), (False, True), (True, False), (True, True)):
                c.new_options.SetBoolean('boot_with_network_traffic_paused', preference)
                c.new_options.SetBoolean('pause_all_new_network_traffic', paused)
                exec(code, {'CG': CG, 'HG': HG, 'self': c})
                out['boot'].append({'preference': preference, 'before': paused, 'after': c.new_options.GetBoolean('pause_all_new_network_traffic')})
        finally:
            for k, v in saved.items(): c.new_options.SetBoolean(k, v)
            HG.boot_with_network_traffic_paused_command_line = command
    c.CallBlockingToQt(c.gui, run)
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f: json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'network_boot_pause.json'), 'w') as f:
        json.dump(value, f, indent=1); f.write('\n')
    print('recorded actual network pause menu and executed reference boot branch')


if __name__ == '__main__': main()
