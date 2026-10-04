#!/usr/bin/env python3
"""Record real subscription clipboard, overwrite and favourites dialogs and tuples.

The v688 menu labels route to the actual callback methods, including its paste
label/method mismatch. Dictionary entries are sorted only for JSON comparison;
the copied text itself is retained verbatim. All inputs use synthetic tags.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def normalise(value):
    if isinstance(value, list):
        value = [normalise(item) for item in value]
        if len(value) == 3 and value[:2] == [21, 2]:
            value[2].sort(key=lambda item: json.dumps(item[0]))
        if len(value) == 3 and value[:2] == [44, 1]:
            value[2].sort(key=lambda item: item[0])
        if len(value) == 3 and value[:2] == [65, 4]:
            value[2][2].sort()
        if len(value) == 3 and value[:2] == [103, 1]:
            for keys in value[2]:
                keys.sort()
        if len(value) == 3 and value[:2] == [14, 8] and value[2][0] == 17:
            value[2][1].sort()
    return value


def record(session):
    def qt():
        from qtpy import QtWidgets as W
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientLocation
        from hydrus.client.gui import ClientGUISubscriptions as G
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.importing import ClientGUIImportOptionsContainer as O
        from hydrus.client.importing.options import ImportOptionsManager as M
        from hydrus.client.importing import ClientImportSubscriptions as Subs
        from hydrus.client.importing.options import ImportOptionsConstants as IOC
        from hydrus.client.importing.options import ImportOptionsContainer as C
        from hydrus.client.importing.options import PrefetchImportOptions as PF
        from hydrus.client.importing.options import FileFilteringImportOptions as FF
        from hydrus.client.importing.options import TagFilteringImportOptions as TF
        from hydrus.client.importing.options import LocationImportOptions as L
        from hydrus.client.importing.options import TagImportOptions as T
        from hydrus.client.importing.options import NoteImportOptions as N
        from hydrus.client.importing.options import PresentationImportOptions as PR
        from hydrus.client.importing.options import ExternalProgramsImportOptions as E

        constructors = [PF.PrefetchImportOptions, FF.FileFilteringImportOptions,
                        TF.TagFilteringImportOptions, L.LocationImportOptions,
                        T.TagImportOptions, N.NoteImportOptions,
                        PR.PresentationImportOptions, E.ExternalProgramsImportOptions]
        full = C.ImportOptionsContainer()
        for constructor in constructors:
            full.SetImportOptions(constructor())
        variants = [full.GetSerialisableTuple()]
        varied = full.Duplicate()
        prefetch = PF.PrefetchImportOptions()
        prefetch._preimport_hash_check_type = 0
        prefetch._fetch_metadata_even_if_url_recognised_and_file_already_in_db = True
        varied.SetImportOptions(prefetch)
        file_filter = FF.FileFilteringImportOptions()
        file_filter._min_size = 1234
        file_filter._max_resolution = (2048, 4096)
        varied.SetImportOptions(file_filter)
        tags = T.ServiceTagImportOptions()
        tags._get_tags = True
        tags._additional_tags = {'series:synthetic', 'plain'}
        tags._additional_tags_overwrite_deleted = True
        tag_import = T.TagImportOptions()
        local_tag_key = session.controller.services_manager.GetServiceKeys((HC.LOCAL_TAG,))[0]
        tag_import._service_keys_to_service_tag_import_options = {local_tag_key: tags}
        varied.SetImportOptions(tag_import)
        filtering = TF.TagFilteringImportOptions()
        filtering._tag_whitelist = ['series:synthetic', 'plain']
        varied.SetImportOptions(filtering)
        notes = N.NoteImportOptions()
        notes._get_notes = False
        notes._conflict_resolution = 0
        notes._all_name_override = 'synthetic'
        notes._names_to_name_overrides = {'old': 'new'}
        varied.SetImportOptions(notes)
        presentation = PR.PresentationImportOptions()
        presentation._presentation_status = 2
        presentation._presentation_inbox = 1
        varied.SetImportOptions(presentation)
        variants.append(varied.GetSerialisableTuple())
        with_deleted = varied.Duplicate()
        deleted_context = ClientLocation.LocationContext(current_service_keys=[], deleted_service_keys=list(L.LocationImportOptions().GetDestinationLocationContext().current_service_keys))
        deleted_presentation = presentation.Duplicate()
        deleted_presentation._location_context = deleted_context
        with_deleted.SetImportOptions(deleted_presentation)
        deleted_locations = L.LocationImportOptions()
        deleted_locations.SetDestinationLocationContext(deleted_context)
        with_deleted.SetImportOptions(deleted_locations)
        variants.append(with_deleted.GetSerialisableTuple())

        existing = C.ImportOptionsContainer()
        existing.SetImportOptions(PF.PrefetchImportOptions())
        existing.SetImportOptions(notes)
        incoming = C.ImportOptionsContainer()
        incoming.SetImportOptions(prefetch)
        incoming.SetImportOptions(presentation)
        subscriptions = []
        for name in ['alpha', 'beta']:
            subscription = Subs.Subscription(name)
            subscription.SetImportOptionsContainer(existing)
            subscriptions.append(subscription)
        panel = G.EditSubscriptionsPanel(session.controller.gui, subscriptions)
        clipboard = {'text': ''}
        exports, questions, errors = [], [], []
        old_pub = session.controller.pub
        def pub(*args, **kwargs):
            if args[:2] == ('clipboard', 'text'):
                clipboard['text'] = args[2]
                exports.append(args[2])
            else:
                old_pub(*args, **kwargs)
        session.controller.pub = pub
        session.controller.GetClipboardText = lambda: clipboard['text']
        Q.PresentClipboardParseError = lambda parent, raw, expected, error: errors.append({'raw': raw, 'expected': expected})
        answers = []
        def ask(parent, message, *args, **kwargs):
            questions.append(message)
            return W.QDialog.DialogCode.Accepted if answers.pop(0) else W.QDialog.DialogCode.Rejected
        Q.GetYesNo = ask
        def rows():
            return [{'name': subscription.GetName(),
                     'summary': subscription.GetImportOptionsContainer().GetSummary(IOC.IMPORT_OPTIONS_CALLER_TYPE_SUBSCRIPTION),
                     'options': normalise(json.loads(subscription.GetImportOptionsContainer().DumpToString()))}
                    for subscription in panel._subscriptions.GetData()]
        selected = panel._subscriptions.GetData()
        panel._subscriptions.SelectDatas([selected[0]])
        panel._CopyImportOptionsContainer()
        out = {'tuples': [normalise(json.loads(json.dumps(v))) for v in variants],
               'existing': normalise(json.loads(existing.DumpToString())),
               'incoming': normalise(json.loads(incoming.DumpToString())),
               'copied_text': exports[0], 'steps': []}
        panel._subscriptions.SelectDatas(selected)
        for label, method in [('merge-paste', panel._PasteImportOptionsContainers),
                              ('fill-in-gaps-paste', panel._PasteImportOptionsContainersMerge),
                              ('replace-paste', panel._PasteImportOptionsContainersFillIn),
                              ('custom paste: choose what you want', panel._PasteImportOptionsContainerCustom)]:
            for subscription in selected:
                subscription.SetImportOptionsContainer(existing)
            clipboard['text'] = incoming.DumpToString()
            method()
            out['steps'].append({'action': label, 'rows': rows()})
        clipboard['text'] = 'not json'
        panel._PasteImportOptionsContainers()
        out['steps'].append({'action': 'invalid paste', 'rows': rows()})
        for answer in [False, True]:
            answers.append(answer)
            panel._ClearImportOptionsContainers()
            out['steps'].append({'action': 'clear', 'accepted': answer, 'rows': rows()})
        out.update(questions=questions, errors=errors)
        overwrite = O.EditImportOptionsOverwritePanel(session.controller.gui,
            IOC.IMPORT_OPTIONS_CALLER_TYPE_SPECIFIC_IMPORTER,
            IOC.IMPORT_OPTIONS_TYPES_CANONICAL_ORDER, existing, incoming)
        def overwrite_state(action):
            return {'action': action,
                    'current': overwrite._current_import_options_container_checklist_box.GetValue(),
                    'pasted': overwrite._pasted_import_options_container_checklist_box.GetValue(),
                    'left_labels': [overwrite._current_import_options_container_checklist_box.item(i).text() for i in range(8)],
                    'pasted_labels': [overwrite._pasted_import_options_container_checklist_box.item(i).text() for i in range(8)],
                    'result_labels': [overwrite._result_listbox.item(i).text() for i in range(overwrite._result_listbox.count())],
                    'options': normalise(json.loads(overwrite.GetValue().DumpToString()))}
        out['overwrite'] = [overwrite_state('initial')]
        for action, mode in [('merge', O.PASTE_MERGE), ('fill in', O.PASTE_FILL_IN), ('replace', O.PASTE_REPLACE)]:
            overwrite._SetUpOverwrite(mode)
            out['overwrite'].append(overwrite_state(action))
        overwrite._current_import_options_container_checklist_box.SetValue([IOC.IMPORT_OPTIONS_TYPE_PREFETCH])
        overwrite._pasted_import_options_container_checklist_box.SetValue([IOC.IMPORT_OPTIONS_TYPE_NOTES])
        overwrite._UpdateResultList()
        out['overwrite'].append(overwrite_state('manual clear notes'))
        overwrite.deleteLater()
        manager = M.ImportOptionsManager()
        out['favourites'] = []
        for name, value in [('profile', existing), ('profile', incoming), ('profile (1)', varied)]:
            actual = manager.AddFavourite(name, value)
            out['favourites'].append({'action': 'add', 'requested': name, 'actual': actual})
        actual = manager.EditFavourite('profile', 'profile (1)', full)
        out['favourites'].append({'action': 'edit', 'original': 'profile', 'requested': 'profile (1)', 'actual': actual})
        manager.DeleteFavourite('profile (1)')
        out['favourites'].append({'action': 'delete', 'name': 'profile (1)'})
        out['favourite_rows'] = [{'name': name, 'options': normalise(json.loads(value.DumpToString()))}
                                for name, value in sorted(manager.GetFavouriteImportOptionContainers().items())]
        ui_manager = M.ImportOptionsManager()
        ui_manager.SetDefaultImportOptionsContainerForCallerType(IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL, full)
        ui_manager.AddFavourite('profile 10', incoming)
        ui_manager.AddFavourite('profile 2', existing)
        ui_manager.AddFavourite('empty', C.ImportOptionsContainer())
        button = O.ImportOptionsContainerFavouritesButton(session.controller.gui, ui_manager,
            current_value_callable=lambda: existing.Duplicate())
        def ui_rows():
            return [{'name': name, 'options': normalise(json.loads(value.DumpToString()))}
                for name, value in sorted(ui_manager.GetFavouriteImportOptionContainers().items())]
        ui_record = {'initial': ui_rows(), 'menus': [], 'steps': [], 'dialogs': []}
        def menu_items(menu):
            return [{'label': action.text(), 'children': menu_items(action.menu()) if action.menu() else []}
                for action in menu.actions() if not action.isSeparator()]
        old_popup = O.CGC.core().PopupMenu
        O.CGC.core().PopupMenu = lambda widget, menu: ui_record['menus'].append(menu_items(menu))
        button._ShowMenu()
        old_enter = Q.EnterText
        def enter(parent, message, **kwargs):
            ui_record['save_question'] = {'message': message, **kwargs}
            return 'profile 2'
        Q.EnterText = enter
        button._SaveCurrentValueAsNew()
        ui_record['steps'].append({'action': 'save current', 'rows': ui_rows()})
        Q.EnterText = old_enter
        old_dialog = O.ClientGUITopLevelWindowsPanels.DialogEdit
        dialog_answers = [False, True, True]
        class FavouriteDialog(W.QDialog):
            def __init__(self, parent, title):
                super().__init__(parent)
                self.title = title
            def __enter__(self):
                return self
            def __exit__(self, *args):
                if hasattr(self, 'panel'):
                    self.panel.deleteLater()
            def SetPanel(self, panel):
                self.panel = panel
                ui_record['dialogs'].append({'title': self.title,
                    'description': panel._description_label.text(), 'name': panel.GetName(),
                    'name_visible': not panel._name_edit_panel.isHidden(),
                    'options': normalise(json.loads(panel.GetValue().DumpToString()))})
            def exec(self):
                self.panel._name_edit.setText('profile 2')
                self.panel.SetValue(incoming.Duplicate())
                return W.QDialog.DialogCode.Accepted if dialog_answers.pop(0) else W.QDialog.DialogCode.Rejected
        O.ClientGUITopLevelWindowsPanels.DialogEdit = FavouriteDialog
        for action in ['add cancel', 'add accept', 'edit accept']:
            if action == 'edit accept':
                button._Edit('profile 10', incoming)
            else:
                button._Add()
            ui_record['steps'].append({'action': action, 'rows': ui_rows()})
        O.ClientGUITopLevelWindowsPanels.DialogEdit = old_dialog
        for accepted in [False, True]:
            def delete_answer(parent, message, **kwargs):
                ui_record['delete_question'] = message
                return W.QDialog.DialogCode.Accepted if accepted else W.QDialog.DialogCode.Rejected
            Q.GetYesNo = delete_answer
            button._Delete('profile 2')
            ui_record['steps'].append({'action': 'delete', 'accepted': accepted, 'rows': ui_rows()})
        Q.GetYesNo = ask
        O.CGC.core().PopupMenu = old_popup
        template = O.EditImportOptionsContainerPanel(session.controller.gui, ui_manager,
            IOC.IMPORT_OPTIONS_CALLER_TYPE_FAVOURITES, C.ImportOptionsContainer(), favourites_name='')
        ui_record['blank_name'] = template.GetName()
        template.deleteLater()
        out['ui_favourites'] = ui_record
        button.deleteLater()
        panel.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(path):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(path, 'w') as stream:
        json.dump(result, stream, indent=2)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as temporary:
        path = os.path.join(temporary, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    with open(os.path.join(HERE, 'fixtures/subscription_import_options.json'), 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('recorded subscription option exchange and reference containers')


if __name__ == '__main__':
    main()
