#!/usr/bin/env python3
"""Record "create new" on the reference's network session browser.

The real `ReviewNetworkSessionsPanel` (on a fresh `NetworkSessionManager`) has
its `_Add` pressed with the edit dialog replaced by a script that records the
dialog's title and the panel inside it (its type choices, the note under
them, the starting domain, the services it offers, whether each data control
is enabled) and then sets values and accepts. Cases: the dialog accepted as
it opens; a domain typed (`Example.com`, `not a domain!!`); a hydrus service
chosen; the hydrus service type with no service to choose; and the dialog
cancelled. Recorded: what the dialog showed and the contexts the manager holds
afterwards (as `[type, data]`), and the list's rows.

The same on the `basic` fixture (no repositories) and the `repositories` fixture
(two), under those keys.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_session_create.py
       (writes fixtures/session_create.json)
"""
import json, os, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    controller = session.controller

    def qt():
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.networking import ClientGUINetwork as N
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.networking import ClientNetworkingSessions
        from qtpy import QtWidgets as QW
        root = controller.gui

        cases = [
            ('accepted as it opens', None),
            ('domain typed', ('domain', 'Example.com')),
            ('domain that is not one', ('domain', 'not a domain!!')),
            ('hydrus service chosen', ('service', 0)),
            ('hydrus service, none to choose', ('service', None)),
            ('cancelled', 'cancel'),
        ]
        out = {'cases': []}
        for name, action in cases:
            shown = {}

            class FakeDialog(QW.QDialog):
                def __init__(self, parent, title, **kwargs):
                    super().__init__(parent)
                    shown['title'] = title

                def __enter__(self):
                    return self

                def __exit__(self, *a):
                    return False

                def SetPanel(self, panel):
                    self.panel = panel

                def exec(self):
                    p = self.panel
                    shown['types'] = [p._context_type.itemText(i) for i in range(p._context_type.count())]
                    shown['type_info'] = p._context_type_info.text()
                    shown['domain'] = p._context_data_text.text()
                    shown['services'] = [p._context_data_services.itemText(i) for i in range(p._context_data_services.count())]
                    shown['enabled'] = {'domain': p._context_data_text.isEnabled(), 'service': p._context_data_services.isEnabled(), 'default box visible': p._context_data_none.isVisibleTo(p)}
                    if action == 'cancel':
                        return QW.QDialog.DialogCode.Rejected
                    if action is not None:
                        kind, value = action
                        if kind == 'domain':
                            p._context_data_text.setText(value)
                        else:
                            p._context_type.SetValue(CC.NETWORK_CONTEXT_HYDRUS)
                            p._Update()
                            shown['type_info_hydrus'] = p._context_type_info.text()
                            shown['enabled_hydrus'] = {'domain': p._context_data_text.isEnabled(), 'service': p._context_data_services.isEnabled()}
                    try:
                        shown['value'] = [p.GetValue().context_type, _data(p.GetValue().context_data)]
                    except Exception as e:
                        shown['value_error'] = str(e)
                    return QW.QDialog.DialogCode.Accepted

            def _data(d):
                return d.hex() if isinstance(d, bytes) else d

            manager = ClientNetworkingSessions.NetworkSessionManager()
            panel = N.ReviewNetworkSessionsPanel(root, manager)
            real = T.DialogEdit
            T.DialogEdit = FakeDialog
            try:
                try:
                    panel._Add()
                except Exception as e:
                    shown['error'] = f'{type(e).__name__}: {e}'
            finally:
                T.DialogEdit = real
            shown['contexts'] = sorted([c.context_type, _data(c.context_data)] for c in manager.GetNetworkContexts()) if True else []
            shown['rows'] = [list(panel._ConvertNetworkContextToDisplayTuple(c)) for c in manager.GetNetworkContexts()]
            shown['show_empty'] = panel._show_empty.isChecked()
            out['cases'].append(dict(name=name, **shown))
            panel.deleteLater()
        return out
    return controller.CallBlockingToQt(controller.gui, qt)


def child(out, fixture):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture(fixture), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2], sys.argv[3])
        return
    import hydrus_driver
    result = {}
    for fixture in ('basic', 'repositories'):
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, 'r.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path, fixture)
            with open(path) as f:
                result[fixture] = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'session_create.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
        f.write('\n')
    print(json.dumps(result, indent=1)[:4000])


if __name__ == '__main__':
    main()
