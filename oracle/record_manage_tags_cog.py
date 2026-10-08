#!/usr/bin/env python3
"""Real ManageTagsPanel: cog options, typed/double-click entry, remove button, copy.

Drives the actual `_Panel` (local `my tags` page) on three synthetic files:
`AddTags` (typed/suggestion entry, honours `allow_remove_on_manage_tags_input`),
`EnterTags` (list double-click, always may remove), `RemoveTags` and the remove
button (honour `yes_no_on_remove_on_manage_tags`) and the copy button. Dialogs
are replaced with scripted answers that record what they were asked. Nothing is
committed: the staged content-update packages are what is recorded.
"""
import json, os, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'manage_tags_cog.json')


def record(session):
    from hydrus.core import HydrusConstants as HC, HydrusExceptions
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.gui import ClientGUIDialogsQuick
    from qtpy import QtWidgets as QW
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    corpus = [
        ('cog:all', [0, 1, 2]), ('cog:some', [0, 1]), ('cog:one', [0]), ('cog:b10', [2]), ('cog:a2', [2]),
    ] + [('bulk:%02d' % i, [0]) for i in range(1, 13)]
    for tag, files in corpus:
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
            local, [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files}))]))

    def media():
        results = {r.GetHash(): r for r in c.Read('media_results', hashes)}
        return [ClientMediaSingle.MediaSingle(results[h]) for h in hashes]

    def work():
        defaults = {k: c.new_options.GetBoolean(k) for k in ('allow_remove_on_manage_tags_input', 'yes_no_on_remove_on_manage_tags', 'ac_select_first_with_count')}
        c.new_options.SetKey('default_tag_service_tab', local)
        asked, choices_asked, clipboard, notices = [], [], [], []
        answers = {'yes': True, 'choice': None}  # choice: index into choices, or None to cancel
        ClientGUIDialogsQuick.GetYesNo = lambda win, message, **kw: (asked.append(message), QW.QDialog.DialogCode.Accepted if answers['yes'] else QW.QDialog.DialogCode.Rejected)[1]

        def select(win, title, choice_tuples, message=None, **kw):
            choices_asked.append({'title': title, 'message': message, 'choices': [{'text': t[0], 'tooltip': t[2] if len(t) > 2 else None} for t in choice_tuples]})
            if answers['choice'] is None or answers['choice'] >= len(choice_tuples):
                raise HydrusExceptions.CancelledException('cancelled')
            return choice_tuples[answers['choice']][1]
        ClientGUIDialogsQuick.SelectFromListButtons = select
        old_pub = c.pub
        c.pub = lambda topic, *a, **kw: clipboard.append(list(a)) if topic == 'clipboard' else old_pub(topic, *a, **kw)
        holder = {}

        def fresh():
            # a new panel per case: it duplicates its media, so staging never leaks between cases
            if 'p' in holder:
                holder['p'].deleteLater()
            p = ManageTagsPanel(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, media())
            holder['p'] = p
            holder['page'] = next(p._tag_services.widget(i) for i in range(p._tag_services.count()) if p._tag_services.widget(i).GetServiceKey() == local)
            holder['page']._copy_button.ShowMicroNotification = lambda m: notices.append(m)
            return holder['page']
        page = fresh()

        def pairs():
            page = holder['page']
            return sorted([{'action': 'add' if cu.GetAction() == HC.CONTENT_UPDATE_ADD else 'delete', 'tag': cu.GetRow()[0], 'files': sorted(hashes.index(h) for h in cu.GetHashes())}
                    for package in page._pending_content_update_packages for key, updates in package.IterateContentUpdates() for cu in updates], key=lambda r: (r['tag'], r['action']))

        def reset():
            fresh()
            asked.clear(); choices_asked.clear(); clipboard.clear(); notices.clear()
            holder['page']._tags_box._selected_terms = set()

        def select_tags(tags):
            box = holder['page']._tags_box
            box._selected_terms = {t for t in box._ordered_terms if t.GetTag() in tags}

        class Spy:
            # records which panel method a case calls, with its arguments
            def __init__(self, page, calls):
                self._page, self._calls = page, calls

            def __getattr__(self, name):
                target = getattr(self._page, name)

                def call(*args, **kwargs):
                    def plain(v):
                        return sorted(v) if isinstance(v, (set, frozenset)) else v
                    self._calls.append({'method': name, 'args': [plain(a) for a in args], 'kwargs': kwargs})
                    return target(*args, **kwargs)
                return call

        def run(name, allow, confirm, action, yes=True, choice=None, selected=()):
            reset()
            c.new_options.SetBoolean('allow_remove_on_manage_tags_input', allow)
            c.new_options.SetBoolean('yes_no_on_remove_on_manage_tags', confirm)
            answers['yes'], answers['choice'] = yes, choice
            select_tags(set(selected))
            calls = []
            action(Spy(holder['page'], calls))
            return {'name': name, 'calls': calls, 'allow_remove': allow, 'confirm': confirm, 'answer_yes': yes, 'choice_index': choice, 'selected': list(selected),
                    'asked': list(asked), 'choices_asked': list(choices_asked), 'staged': pairs(), 'clipboard': list(clipboard), 'notices': list(notices)}

        cases = []
        for allow in (False, True):
            cases.append(run('add_everywhere', allow, True, lambda page: page.AddTags({'cog:all'})))
            cases.append(run('add_none_have', allow, True, lambda page: page.AddTags({'cog:b99'})))
            cases.append(run('add_some_have_first', allow, True, lambda page: page.AddTags({'cog:some'}), choice=0))
            cases.append(run('add_some_have_second', allow, True, lambda page: page.AddTags({'cog:some'}), choice=1))
            cases.append(run('add_some_have_cancel', allow, True, lambda page: page.AddTags({'cog:some'}), choice=None))
            cases.append(run('add_many', allow, True, lambda page: page.AddTags({'cog:all', 'cog:some', 'cog:b99'}), choice=0))
            cases.append(run('add_many_second', allow, True, lambda page: page.AddTags({'cog:all', 'cog:some', 'cog:b99'}), choice=1))
            cases.append(run('add_many_third', allow, True, lambda page: page.AddTags({'cog:all', 'cog:some', 'cog:b99'}), choice=2))
            cases.append(run('add_only_add_flag', allow, True, lambda page: page.AddTags({'cog:some'}, only_add=True), choice=1))
        cases.append(run('list_activate_all_have', False, True, lambda page: page.EnterTags({'cog:all'})))
        cases.append(run('list_activate_some_have', False, True, lambda page: page.EnterTags({'cog:some'}), choice=1))
        cases.append(run('list_activate_one_file', False, True, lambda page: page.EnterTags({'cog:one'}), choice=1))
        for confirm in (True, False):
            cases.append(run('remove_one', False, confirm, lambda page: page.RemoveTags(['cog:all']), yes=True))
            cases.append(run('remove_one_declined', False, confirm, lambda page: page.RemoveTags(['cog:all']), yes=False))
            cases.append(run('remove_three', False, confirm, lambda page: page.RemoveTags(['cog:all', 'cog:some', 'cog:one'])))
            cases.append(run('remove_twelve', False, confirm, lambda page: page.RemoveTags(['bulk:%02d' % i for i in range(1, 13)])))
            cases.append(run('remove_nine', False, confirm, lambda page: page.RemoveTags(['bulk:%02d' % i for i in range(1, 10)])))
            cases.append(run('remove_ten', False, confirm, lambda page: page.RemoveTags(['bulk:%02d' % i for i in range(1, 11)])))
            cases.append(run('remove_long', False, confirm, lambda page: page.RemoveTags(['x' * 70])))
            cases.append(run('remove_empty', False, confirm, lambda page: page.RemoveTags([])))
        cases.append(run('remove_button_all', False, True, lambda page: page._RemoveTagsButton()))
        cases.append(run('remove_button_selected', False, True, lambda page: page._RemoveTagsButton(), selected=['cog:some', 'bulk:03']))
        cases.append(run('remove_button_selected_declined', False, True, lambda page: page._RemoveTagsButton(), yes=False, selected=['cog:some']))
        cases.append(run('remove_button_selected_unconfirmed', False, False, lambda page: page._RemoveTagsButton(), selected=['cog:some']))
        cases.append(run('copy_all', False, True, lambda page: page._Copy()))
        cases.append(run('copy_selected', False, True, lambda page: page._Copy(), selected=['cog:some', 'cog:b10', 'cog:a2']))

        items = getattr(holder['page']._cog_button, '_menu_template_items', [])
        menu = [{'kind': type(i).__name__, 'title': i.title() if callable(i.title) else i.title, 'description': i.description() if callable(i.description) else i.description} for i in items]
        remove_button = holder['page']._remove_tags.text()
        holder['p'].deleteLater()
        c.pub = old_pub
        return {'files': [h.hex() for h in hashes], 'corpus': corpus, 'defaults': defaults, 'cog_menu': menu, 'remove_button_text': remove_button, 'cases': cases}
    return c.CallBlockingToQt(c.gui, work)


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2]); return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
