#!/usr/bin/env python3
"""Record real Qt recursive subsidiary add/edit/cancel, child documents and parsing.
Dialog results are scripted, but subsidiary defaults, controls, parent list
callbacks, inherited converter/separator data and runtime outputs are reference
code. Inputs use generated HTML and reserved synthetic domains only.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W, ClientGUIDialogsQuick as D
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        from hydrus.core import HydrusConstants as H
        root = session.controller.gui
        context = {'url': 'https://children.example/post', 'post_index': '7', 'token': 'preserved'}
        raw = '<div class="thumb"><p>first\n\nnote</p></div><div class="thumb"><p>second note</p></div>'
        converter = S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT, '<!-- parent converted -->')])
        page = P.PageParser(name='parent', string_converter=converter)
        parent = G.EditPageParserPanel(root, page, test_data=P.ParsingTestData(context, [raw]))
        parent._test_panel._SetExampleData(raw)
        out = []
        answers = []
        original_panel = G.EditSubsidiaryPageParserPanel
        class RecordedPanel(original_panel):
            def __init__(self, owner, parser, test_data=None):
                super().__init__(owner, parser, test_data=test_data)
                self.inherited = test_data
                # The reference schedules initial text after construction. Drive
                # that same setter now, after every derived field exists.
                self._test_panel._SetExampleData(test_data.texts[0] if test_data.texts else '')
        G.EditSubsidiaryPageParserPanel = RecordedPanel
        def content(kind, text):
            if kind == 'note':
                return P.ContentParser(name='child note', content_type=H.CONTENT_TYPE_NOTES,
                    formula=P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')], content_to_fetch=P.HTML_CONTENT_STRING), additional_info='note')
            return P.ContentParser(name='child tag', content_type=H.CONTENT_TYPE_MAPPINGS,
                formula=P.ParseFormulaStatic(static_text=text), additional_info='series')
        def texts(parser, text):
            return [[c.parsed_text for c in post.parsed_contents] for post in parser.Parse(dict(context), text)]
        class Dialog(QW.QDialog):
            def __init__(self, owner, *args, **kwargs): super().__init__(owner)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel):
                self.panel = panel
                data = panel.inherited
                panel._test_panel._SetExampleData(data.texts[0] if data.texts else '')
                out.append({'case': 'open_child', 'name': panel.GetValue().GetPageParser().GetName(),
                    'sort': panel.GetValue().GetSortPostsBySourceTime(), 'collapse': panel._formula._collapse_newlines,
                    'formula': panel.GetValue().GetFormula().GetSerialisableTuple(),
                    'texts': list(data.texts), 'context': dict(data.parsing_context)})
            def exec(self):
                answer = answers.pop(0)
                if answer is None: return QW.QDialog.DialogCode.Rejected
                panel = self.panel
                panel._name.setText(answer['name'])
                panel._sort_posts_by_source_time.setChecked(answer.get('sort', False))
                if 'separator' in answer: panel._formula._current_formula = P.ParseFormulaStatic(static_text=answer['separator'])
                if 'content' in answer: panel._content_parsers.AddDatas([content(*answer['content'])])
                panel._test_panel._RefreshDataPreviews()
                data = panel._test_panel.GetTestDataForChild()
                out.append({'case': 'child_data', 'name': answer['name'], 'texts': list(data.texts), 'context': dict(data.parsing_context)})
                if 'nested' in answer:
                    answers.insert(0, answer['nested'])
                    panel._AddSubPageParser()
                panel.resize(1200, 900); panel.show(); QW.QApplication.processEvents()
                panel.grab().save(os.path.join(HERE, 'fixtures', 'parser_child_editor.png'))
                return QW.QDialog.DialogCode.Accepted
        original_dialog, original_yes = W.DialogEdit, D.GetYesNo
        W.DialogEdit = Dialog
        D.GetYesNo = lambda *args, **kwargs: QW.QDialog.DialogCode.Accepted
        try:
            default = P.SubsidiaryPageParser()
            out.append({'case': 'defaults', 'name': default.GetPageParser().GetName(),
                'sort': default.GetSortPostsBySourceTime(), 'formula': default.GetFormula().GetSerialisableTuple(),
                'separated': default.GetFormula().Parse(dict(context), raw, False)})
            for action, answer in [('cancel_add', None), ('add', {'name': 'notes', 'content': ('note', '')}),
                    ('add_nested', {'name': 'nested', 'separator': '<p>parent note</p>', 'content': ('note', ''),
                        'nested': {'name': 'nested tags', 'separator': '<p>leaf</p>', 'content': ('tag', 'leaf')}})]:
                answers.append(answer)
                parent._AddSubPageParser()
                value = parent.GetValue()
                out.append({'case': 'parent_action', 'action': action,
                    'names': [s.GetPageParser().GetName() for s in parent._subsidiary_page_parsers.GetData()],
                    'outputs': texts(value, raw), 'tuple': value.GetSerialisableTuple()})
            selected = next(s for s in parent._subsidiary_page_parsers.GetData() if s.GetPageParser().GetName() == 'notes')
            for action, answer in [('cancel_edit', None), ('edit', {'name': 'edited notes', 'sort': True, 'separator': '<p>edited\n\nnote</p>'})]:
                parent._subsidiary_page_parsers.SelectDatas([selected], deselect_others=True)
                answers.append(answer)
                parent._EditSubPageParser()
                value = parent.GetValue()
                out.append({'case': 'parent_action', 'action': action,
                    'names': [s.GetPageParser().GetName() for s in parent._subsidiary_page_parsers.GetData()],
                    'outputs': texts(value, raw), 'tuple': value.GetSerialisableTuple()})
            parent.resize(1200, 900); parent.show(); QW.QApplication.processEvents()
            parent.grab().save(os.path.join(HERE, 'fixtures', 'parser_children.png'))
        finally:
            W.DialogEdit, D.GetYesNo = original_dialog, original_yes
            G.EditSubsidiaryPageParserPanel = original_panel
            parent.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(path):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(path, 'w') as f: json.dump(result, f, indent=2, ensure_ascii=False)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child': child(sys.argv[2]); return
    import tempfile
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'children.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'parser_children.json'), 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('recorded', len(result), 'recursive subsidiary states')


if __name__ == '__main__': main()
