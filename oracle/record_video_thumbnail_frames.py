#!/usr/bin/env python3
"""Record the frame the reference makes a video's or animation's thumbnail
from under the "Generate video thumbnails this % in" option.

A private basic-fixture client sets `video_thumbnail_percentage_in` to each of
0, 1, 20, 35, 50, 99 and 100 and, at each, regenerates the thumbnails of the
fixture's animated gif and apng, webm and mp4 (its animated webp isn't local) through its real
client files manager (`RegenerateThumbnail`, as file maintenance does). The
stored thumbnail is read back and recorded as its size and a 16x16 signature:
the mean of the RGB values in each block (block edges at `i * size // 16`,
integer division of the sum by the count), so the replay can tell the frames
apart and compare them. Each file's frame count is recorded too.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_video_thumbnail_frames.py
"""
import io
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'video_thumbnail_frames.json')

NAMES = ['gif_animated.gif', 'apng_animated.png', 'video_silent.webm', 'video_with_audio.mp4']
PERCENTAGES = [0, 1, 20, 35, 50, 99, 100]


def signature(data):
    from PIL import Image
    image = Image.open(io.BytesIO(data)).convert('RGB')
    (w, h) = image.size
    pixels = image.load()
    out = []
    for by in range(16):
        for bx in range(16):
            (x0, x1) = (bx * w // 16, (bx + 1) * w // 16)
            (y0, y1) = (by * h // 16, (by + 1) * h // 16)
            total = 0
            count = 0
            for y in range(y0, max(y1, y0 + 1)):
                for x in range(x0, max(x1, x0 + 1)):
                    (r, g, b) = pixels[min(x, w - 1), min(y, h - 1)]
                    total += r + g + b
                    count += 3
            out.append(total // count)
    return out


def record(session):
    controller = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    results = controller.Read('media_results', [by_name[n] for n in NAMES])
    order = {by_name[n]: i for (i, n) in enumerate(NAMES)}
    results.sort(key=lambda m: order[m.GetHash()])
    manager = controller.client_files_manager
    files = []
    for (name, media_result) in zip(NAMES, results):
        files.append({
            'name': name,
            'hash': media_result.GetHash().hex(),
            'mime': media_result.GetMime(),
            'num_frames': media_result.GetNumFrames(),
            'thumbnails': [],
        })
    for percentage in PERCENTAGES:
        controller.new_options.SetInteger('video_thumbnail_percentage_in', percentage)
        for (file, media_result) in zip(files, results):
            manager.RegenerateThumbnail(media_result)
            path = manager.GetThumbnailPath(media_result)
            with open(path, 'rb') as f:
                data = f.read()
            from PIL import Image
            size = Image.open(io.BytesIO(data)).size
            file['thumbnails'].append({'percentage': percentage, 'size': list(size), 'signature': signature(data)})
    controller.new_options.SetInteger('video_thumbnail_percentage_in', 35)
    return {'percentages': PERCENTAGES, 'files': files}


def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
