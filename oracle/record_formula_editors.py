#!/usr/bin/env python3
"""Drive real HTML/JSON formula and rule panels on the Qt thread.
Records extraction controls, rule descriptions, active/inactive controls,
validation, unprocessed strings and parsed strings after rule/type edits.
The fixture covers all HTML traversal directions and all JSON rule types,
negative indices, attributes, tag-text tests, and newline policy. It also
records rule queue edits/cancel/reorder/delete, fresh formula type changes,
and named formula list add/edit/multiple-edit actions and unique names.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client.gui.parsing import ClientGUIParsingFormulae as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        out = []
        html = '<html><body><div class="post"><a href="one"> first\n link </a><a href="two">second</a><span>after</span></div></body></html>'
        root = session.controller.gui
        # Set actual controls and record GetValue/ToString, including modes
        # whose attributes/index controls must be disabled.
        rule = G.EditHTMLTagRulePanel(root, P.ParseRuleHTML())
        rule._tag_name.setText('a')
        rule._tag_attributes.SetValue({'class': 'image'})
        rule._tag_index.SetValue(-1)
        for label, kind in [('descendants', P.HTML_RULE_TYPE_DESCENDING), ('ancestors', P.HTML_RULE_TYPE_ASCENDING), ('previous', P.HTML_RULE_TYPE_PREV_SIBLINGS), ('next', P.HTML_RULE_TYPE_NEXT_SIBLINGS)]:
            rule._rule_type.SetValue(kind)
            rule._tag_depth.setValue(2)
            rule.EventTypeChanged(0)
            out.append({'case': 'html_rule', 'type': label, 'description': rule.GetValue().ToString(), 'search_enabled': rule._tag_attributes.isEnabled(), 'depth_enabled': rule._tag_depth.isEnabled()})
        rule.deleteLater()
        rules = [P.ParseRuleHTML(tag_name='a')]
        panel = G.EditHTMLFormulaPanel(root, True, P.ParseFormulaHTML(tag_rules=rules), P.ParsingTestData({}, [html]))
        panel._name.setText('links')
        for label, content in [('attribute', P.HTML_CONTENT_ATTRIBUTE), ('string', P.HTML_CONTENT_STRING), ('html', P.HTML_CONTENT_HTML)]:
            panel._content_to_fetch.SetValue(content)
            panel._attribute_to_fetch.setText('href')
            f = panel.GetValue()
            out.append({'case': 'html_formula', 'content': label, 'name': f.GetName(), 'rules': [r.ToString() for r in f.GetTagRules()], 'text': html, 'results': f.Parse({}, html, True)})
        panel._content_to_fetch.SetValue(P.HTML_CONTENT_ATTRIBUTE)
        panel._attribute_to_fetch.setText('')
        try:
            panel.GetValue()
        except Exception as e:
            out.append({'case': 'attribute_veto', 'error': str(e)})
        panel.deleteLater()
        cases = [
            ('key', P.JSON_PARSE_RULE_TYPE_DICT_KEY, S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='posts', example_string='posts'), '{"posts":["a","b"],"other":"c"}', P.JSON_CONTENT_JSON),
            ('all', P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None, '{"z":"z","a":"a"}', P.JSON_CONTENT_STRING),
            ('index', P.JSON_PARSE_RULE_TYPE_INDEXED_ITEM, -1, '["a","b"]', P.JSON_CONTENT_STRING),
            ('filter', P.JSON_PARSE_RULE_TYPE_TEST_STRING_ITEMS, S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='42', example_string='42'), '42', P.JSON_CONTENT_STRING),
            ('ascend', P.JSON_PARSE_RULE_TYPE_ASCEND, 1, '{"posts":"a"}', P.JSON_CONTENT_DICT_KEYS),
            ('deminify', P.JSON_PARSE_RULE_TYPE_DEMINIFY_JSON, 0, '[{"name":1},"samus"]', P.JSON_CONTENT_JSON),
        ]
        for label, kind, data, text, content in cases:
            r = G.EditJSONParsingRulePanel(root, (kind,data))
            value = r.GetValue()
            rules = [value]
            if label == 'ascend':
                rules.insert(0, (P.JSON_PARSE_RULE_TYPE_DICT_KEY, S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='posts', example_string='posts')))
            p = G.EditJSONFormulaPanel(root, True, P.ParseFormulaJSON(parse_rules=rules, content_to_fetch=content), P.ParsingTestData({}, [text]))
            p._name.setText('json test')
            f = p.GetValue()
            out.append({'case': 'json_formula', 'type': label, 'description': P.RenderJSONParseRule(value), 'content': {P.JSON_CONTENT_STRING:'string',P.JSON_CONTENT_JSON:'json',P.JSON_CONTENT_DICT_KEYS:'dictionary keys'}[content], 'text': text, 'results': f.Parse({},text,True)})
            r.deleteLater()
            p.deleteLater()
        text = '[" first\\n second "]'
        p = G.EditJSONFormulaPanel(root, False, P.ParseFormulaJSON(parse_rules=[(P.JSON_PARSE_RULE_TYPE_ALL_ITEMS,None)]), P.ParsingTestData({}, [text]))
        out.append({'case':'json_newlines', 'text':text, 'results':p.GetValue().Parse({},text,False)})
        p.deleteLater()
        # Drive add/edit/up/down/delete through the real queue. Child dialog
        # controls are scripted on acceptance; cancellation returns no edit.
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W, ClientGUIDialogsQuick as D
        answers = []
        class Dialog(QW.QDialog):
            def __init__(self, parent, *args, **kwargs): super().__init__(parent)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self):
                edit = answers.pop(0)
                if edit is None: return QW.QDialog.DialogCode.Rejected
                self.panel._tag_name.setText(edit)
                return QW.QDialog.DialogCode.Accepted
        W.DialogEdit = Dialog
        asked = []
        D.GetYesNo = lambda parent, message, **kwargs: (asked.append(message), QW.QDialog.DialogCode.Accepted)[1]
        p = G.EditHTMLFormulaPanel(root, True, P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='a')]), P.ParsingTestData({}, [html]))
        q = p._tag_rules
        for action, args in [('add','span'), ('edit','div'), ('up',None), ('down',None), ('cancel_add',None), ('delete',None)]:
            if action in ('add', 'edit', 'cancel_add'):
                answers.append(args)
                if action == 'edit':
                    q._listbox.clearSelection()
                    q._listbox.item(1).setSelected(True)
                    q._Edit()
                else: q._Add()
            elif action == 'up': q._Up()
            elif action == 'down': q._Down()
            elif action == 'delete': q._Delete()
            out.append({'case':'queue', 'action':action, 'rules':[r.ToString() for r in q.GetData()], 'selected':[i for i in range(q._listbox.count()) if q._listbox.item(i).isSelected()], 'asked':list(asked)})
        p.deleteLater()
        # The generic formula type chooser resets name/processor, retaining
        # separated extraction when converting an HTML/JSON subtree formula.
        choice = ['change to a new JSON formula', 'change to a new HTML formula']
        def select(parent, title, tuples):
            selected = choice.pop(0)
            return next(v for label,v in tuples if label == selected)
        D.SelectFromList = select
        outer = G.EditFormulaPanel(root, P.ParseFormulaHTML(content_to_fetch=P.HTML_CONTENT_HTML, name='old name'), lambda: P.ParsingTestData({}, ['']))
        for action in ('json','html'):
            outer._ChangeFormulaType()
            f = outer.GetValue()
            out.append({'case':'change_type', 'type':action, 'name':f.GetName(), 'content':f.GetContentToFetch(), 'processor':f.GetStringProcessor().GetProcessingStrings()})
        outer.deleteLater()
        # Named list duplicate handling includes an edited entry's old name.
        from hydrus.client.gui.lists import ClientGUIListBoxes as L
        named = L.AddEditDeleteListBoxUniqueNamedObjects(root, 5, lambda f:f.GetName(), lambda:P.SimpleDownloaderParsingFormula(name='links'), lambda f:P.SimpleDownloaderParsingFormula(name=f.GetName()))
        for action in ('add','add','edit','delete'):
            if action == 'add': named._Add()
            elif action == 'edit':
                named._listbox.clearSelection()
                named._listbox.item(0).setSelected(True)
                named._Edit()
            else: named._Delete()
            out.append({'case':'named_list', 'action':action, 'names':[f.GetName() for f in named.GetData()]})
        named._AddData(P.SimpleDownloaderParsingFormula(name='first'))
        named._listbox.clearSelection()
        for i in range(named._listbox.count()): named._listbox.item(i).setSelected(True)
        before = [f.GetName() for f in named.GetData()]
        named._Edit()
        out.append({'case':'named_multi', 'before':before, 'after':[f.GetName() for f in named.GetData()]})
        named.deleteLater()
        # Real formula test-panel source strings with processor disabled.
        processor = S.StringProcessor()
        processor.SetProcessingSteps([S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT,'!')])])
        f = P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='a')], content_to_fetch=P.HTML_CONTENT_STRING, string_processor=processor)
        p = G.EditHTMLFormulaPanel(root, False, f, P.ParsingTestData({}, [html]))
        p._test_panel._SetExampleData(html)
        out.append({'case':'processor_test', 'text':html, 'before':p._test_panel.GetTestDataForStringProcessor().texts, 'after':p.GetValue().Parse({},html,False)})
        p.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=2)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import tempfile
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'formula_editors.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'formula_editors.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=2)
        f.write('\n')
    print('recorded', len(result), 'formula editor states')

if __name__ == '__main__':
    main()
