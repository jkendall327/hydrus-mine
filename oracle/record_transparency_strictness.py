#!/usr/bin/env python3
"""What the import job calls "having transparency", at each strictness level.

Writes small PNGs to `fixtures/transparency/` (no alpha channel; an alpha
channel fully opaque, fully clear, with three faint pixels, with a clear
half). Then, in the reference client, chooses each level in the real
"media playback" options panel, applies it as the options dialog's OK does
(`UpdateOptions`, then `ReinitGlobalSettings`), and runs the real import job
(`ClientImportFiles.FileImportJob.GenerateInfo`) on each file, recording
`HasTransparency()`. Writes `fixtures/transparency_strictness.json`.

Run: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_transparency_strictness.py
"""
import json
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

IMAGES = HERE / 'fixtures' / 'transparency'


def generate():
    from PIL import Image
    IMAGES.mkdir(parents=True, exist_ok=True)

    def rgba(alpha_of):
        image = Image.new('RGBA', (100, 100))
        image.putdata([(200, 100, 50, alpha_of(i)) for i in range(100 * 100)])
        return image

    files = {
        'rgb.png': Image.new('RGB', (100, 100), (200, 100, 50)),
        'opaque.png': rgba(lambda i: 255),
        'clear.png': rgba(lambda i: 0),
        'few_faint.png': rgba(lambda i: 128 if i < 3 else 255),
        'half_clear.png': rgba(lambda i: 0 if i % 100 < 50 else 255),
    }
    for (name, image) in files.items():
        image.save(IMAGES / name)
    return sorted(files)


def record(session, names):
    def qt():
        from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel
        from hydrus.client.importing import ClientImportFiles
        from hydrus.client.importing.options import ImportOptionsConstants
        from hydrus.client.importing.options import ImportOptionsContainer

        c = session.controller
        original = c.new_options.Duplicate()
        out = {'files': names, 'levels': []}
        try:
            panel = MediaPlaybackPanel(c.gui)
            choice = panel._file_has_transparency_strictness
            out['choices'] = [choice.itemText(i) for i in range(choice.count())]
            out['default_index'] = choice.currentIndex()
            for index in range(choice.count()):
                panel = MediaPlaybackPanel(c.gui)
                panel._file_has_transparency_strictness.setCurrentIndex(index)
                panel.UpdateOptions()
                c.ReinitGlobalSettings()
                # what a local import's job is given
                container = c.import_options_manager.GenerateFullImportOptionsContainer(ImportOptionsContainer.ImportOptionsContainer(), ImportOptionsConstants.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT)
                judged = {}
                for name in names:
                    with tempfile.TemporaryDirectory() as directory:
                        path = str(Path(directory) / name)
                        shutil.copy(IMAGES / name, path)
                        job = ClientImportFiles.FileImportJob(path, container)
                        job.GeneratePreImportHashAndStatus()
                        job.GenerateInfo()
                        judged[name] = job.HasTransparency()
                out['levels'].append({
                    'index': index,
                    'saved': c.new_options.GetInteger('file_has_transparency_strictness'),
                    'has_transparency': judged,
                })
            return out
        finally:
            c.new_options = original
            c.ReinitGlobalSettings()

    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = Path(sys.argv[2])
        names = sorted(p.name for p in IMAGES.glob('*.png'))
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), lambda s: record(s, names))
        output.write_text(json.dumps(result))
        return
    generate()
    with tempfile.TemporaryDirectory() as directory:
        result = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        out = json.loads(result.read_text())
    path = HERE / 'fixtures/transparency_strictness.json'
    path.write_text(json.dumps(out, indent=1) + '\n')
    print('wrote ' + str(path))


if __name__ == '__main__':
    main()
