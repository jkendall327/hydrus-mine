#!/usr/bin/env python3
"""Probe real Qt image-policy controls and PIL pixels/errors on authored images."""
import hashlib
import io
import json
import struct
import sys
import tempfile
import zlib
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
FIXTURES = HERE / 'fixtures' / 'image_decoder_policies'


def record(session):
    from PIL import Image, PngImagePlugin
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC, HydrusSerialisable
    from hydrus.core.files.images import HydrusImageHandling as H, HydrusImageNormalisation as N
    from hydrus.client import ClientOptions
    from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel

    FIXTURES.mkdir(exist_ok=True)
    image = Image.new('RGB', (8, 8))
    image.putdata([(x * 24 + 8, y * 24 + 8, (x + y) * 12 + 8) for y in range(8) for x in range(8)])
    chroma = (0.3127, 0.3290, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06)
    profile_image = Image.new('RGB', (1, 1))
    profile_image.format = 'PNG'
    profile_image.info = {'gamma': 1.0, 'chromaticity': chroma}
    profile = N.GenerateICCProfileBytesFromGammaAndChromaticityPNG(profile_image)
    image.save(FIXTURES / 'embedded-linear.png', icc_profile=profile)
    image.save(FIXTURES / 'embedded-linear.jpg', quality=95, subsampling=0, icc_profile=profile)
    image.save(FIXTURES / 'embedded-linear.webp', lossless=True, icc_profile=profile)
    alternate = image.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    image.save(FIXTURES / 'embedded-animation.webp', lossless=True, icc_profile=profile, save_all=True, append_images=[alternate], duration=[100, 150], loop=0)
    with zipfile.ZipFile(FIXTURES / 'embedded-ugoira.zip', 'w') as archive:
        archive.writestr('000000.png', (FIXTURES / 'embedded-linear.png').read_bytes())
        buffer = io.BytesIO(); alternate.save(buffer, format='PNG', icc_profile=profile)
        archive.writestr('000001.png', buffer.getvalue())
        archive.writestr('animation.json', json.dumps([{'file':'000000.png','delay':100},{'file':'000001.png','delay':150}]))
    gamma = PngImagePlugin.PngInfo()
    gamma.add(b'gAMA', struct.pack('>I', 100000))
    gamma.add(b'cHRM', struct.pack('>8I', *[round(v * 100000) for v in chroma]))
    image.save(FIXTURES / 'gamma-linear.png', pnginfo=gamma)
    # Embedded sRGB-like gamma plus independent linear PNG tags exposes the
    # reference's fallback when the embedded-profile policy is switched off.
    profile_image.info['gamma'] = 0.45455
    regular_profile = N.GenerateICCProfileBytesFromGammaAndChromaticityPNG(profile_image)
    image.save(FIXTURES / 'embedded-and-gamma.png', icc_profile=regular_profile, pnginfo=gamma)
    image.save(FIXTURES / 'plain.png')
    image.save(FIXTURES / 'plain.jpg', quality=95, subsampling=0)
    image.save(FIXTURES / 'plain.gif')
    jpeg = (FIXTURES / 'plain.jpg').read_bytes()
    (FIXTURES / 'jpeg-no-eoi.jpg').write_bytes(jpeg[:-2])
    (FIXTURES / 'jpeg-short-scan.jpg').write_bytes(jpeg[:-32])
    gif = (FIXTURES / 'plain.gif').read_bytes()
    (FIXTURES / 'gif-short-data.gif').write_bytes(gif[:-10])
    png = (FIXTURES / 'plain.png').read_bytes()
    output = bytearray(png[:8])
    offset = 8
    while offset < len(png):
        length = struct.unpack('>I', png[offset:offset + 4])[0]
        kind = png[offset + 4:offset + 8]
        data = png[offset + 8:offset + 8 + length]
        if kind == b'IDAT':
            data = data[:-8]
        output += struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
        offset += length + 12
    (FIXTURES / 'png-short-data.png').write_bytes(output)
    # More cut-off files: each format cut part-way through its pixel data.
    # (GIF and PNG are cut at several depths so the fill shows rows, not
    # only the last few bytes.)
    (FIXTURES / 'jpeg-half.jpg').write_bytes(jpeg[:len(jpeg) * 2 // 3])
    (FIXTURES / 'gif-half.gif').write_bytes(gif[:len(gif) - 30])
    half_png = bytearray(png[:8])
    offset = 8
    while offset < len(png):
        length = struct.unpack('>I', png[offset:offset + 4])[0]
        kind = png[offset + 4:offset + 8]
        data = png[offset + 8:offset + 8 + length]
        if kind == b'IDAT':
            data = data[:len(data) // 2]
        half_png += struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
        offset += length + 12
    (FIXTURES / 'png-half-data.png').write_bytes(bytes(half_png))
    big = Image.new('RGB', (64, 64))
    big.putdata([((x * 4 + y) % 256, (y * 4 + x * 3) % 256, (x * y) % 256) for y in range(64) for x in range(64)])
    for lossless, tag in ((True, 'lossless'), (False, 'lossy')):
        buffer = io.BytesIO(); big.save(buffer, format='WEBP', lossless=lossless, quality=90)
        data = buffer.getvalue()
        (FIXTURES / f'webp-{tag}-whole.webp').write_bytes(data)
        (FIXTURES / f'webp-{tag}-half.webp').write_bytes(data[:len(data) // 2])
    # 64x64 files cut part-way through the pixel data, so that the part
    # that did load and the part that did not are both big enough to see
    # (an 8x8 file is one JPEG block or a few dozen bytes of zlib).
    for name, fmt, kwargs, cut in (
        ('jpeg-big-cut.jpg', 'JPEG', dict(quality=90, subsampling=0), 0.75),
        ('png-big-cut.png', 'PNG', dict(), 0.6),
        ('gif-big-cut.gif', 'GIF', dict(), 0.6),
        ('bmp-big-cut.bmp', 'BMP', dict(), 0.6),
    ):
        buffer = io.BytesIO(); big.save(buffer, format=fmt, **kwargs)
        data = buffer.getvalue()
        if fmt == 'PNG':
            cut_png = bytearray(data[:8]); offset = 8
            while offset < len(data):
                length = struct.unpack('>I', data[offset:offset + 4])[0]
                kind = data[offset + 4:offset + 8]
                chunk = data[offset + 8:offset + 8 + length]
                if kind == b'IDAT':
                    chunk = chunk[:int(len(chunk) * cut)]
                cut_png += struct.pack('>I', len(chunk)) + kind + chunk + struct.pack('>I', zlib.crc32(kind + chunk))
                offset += length + 12
            data = bytes(cut_png)
        else:
            data = data[:int(len(data) * cut)]
        (FIXTURES / name).write_bytes(data)
    # a PNG cut at a byte, as a download that stopped, so that it ends in
    # the middle of its image data chunk with no checksum or end chunk
    buffer = io.BytesIO(); big.save(buffer, format='PNG')
    (FIXTURES / 'png-raw-cut.png').write_bytes(buffer.getvalue()[:int(len(buffer.getvalue()) * 0.6)])
    buffer = io.BytesIO(); image.save(buffer, format='BMP')
    bmp = buffer.getvalue()
    (FIXTURES / 'bmp-whole.bmp').write_bytes(bmp)
    (FIXTURES / 'bmp-short-data.bmp').write_bytes(bmp[:-50])
    (FIXTURES / 'bmp-half.bmp').write_bytes(bmp[:len(bmp) // 2])
    inputs = sorted(p.name for p in FIXTURES.iterdir() if p.suffix != '.zip')
    c = session.controller

    def drive():
        names = ['do_icc_profile_normalisation', 'enable_truncated_images_pil']
        original = [c.new_options.GetBoolean(k) for k in names]
        loaded = list(original)
        dialog = QW.QDialog(c.gui)
        layout = QW.QVBoxLayout(dialog)
        panel = MediaPlaybackPanel(dialog)
        layout.addWidget(panel)
        controls = [panel._do_icc_profile_normalisation, panel._enable_truncated_images_pil]
        for control in controls:
            control.setChecked(False)
        cancel_staged = [c.new_options.GetBoolean(k) for k in names]
        panel.deleteLater()
        panel = MediaPlaybackPanel(dialog)
        layout.addWidget(panel)
        controls = [panel._do_icc_profile_normalisation, panel._enable_truncated_images_pil]
        events = []
        pub = c.pub

        def capture(topic, *args, **kwargs):
            if topic in ('clear_image_cache', 'clear_image_tile_cache'):
                events.append(topic)
            return pub(topic, *args, **kwargs)

        c.pub = capture
        cases = []
        legacy = None
        try:
            for icc, truncated in [(True, True), (False, True), (False, False), (True, False)]:
                before = [c.new_options.GetBoolean(k) for k in names]
                for control, value in zip(controls, (icc, truncated)):
                    control.setChecked(value)
                staged = [c.new_options.GetBoolean(k) for k in names]
                panel.UpdateOptions()
                saved = [c.new_options.GetBoolean(k) for k in names]
                if not icc: legacy = c.new_options.GetSerialisableTuple()
                copy = HydrusSerialisable.CreateFromString(c.new_options.DumpToString())
                events.clear()
                H.SetEnableLoadTruncatedImages(truncated)
                N.SetDoICCProfileNormalisation(icc)
                rows = []
                for name in inputs:
                    path = FIXTURES / name
                    mime = HC.IMAGE_JPEG if name.endswith('.jpg') else HC.IMAGE_GIF if name.endswith('.gif') else HC.IMAGE_WEBP if name.endswith('.webp') else HC.IMAGE_BMP if name.endswith('.bmp') else HC.IMAGE_PNG
                    try:
                        pixels = H.GenerateNumPyImage(str(path), mime, force_pil=True)
                        rows.append(dict(file=name, ok=True, shape=list(pixels.shape), pixels=list(pixels.tobytes()), pixel_sha256=hashlib.sha256(pixels.tobytes()).hexdigest()))
                    except Exception as error:
                        rows.append(dict(file=name, ok=False, error_type=type(error).__name__, error=str(error)))
                frames = []
                with Image.open(FIXTURES / 'embedded-animation.webp') as animation:
                    for index in range(animation.n_frames):
                        animation.seek(index)
                        current = animation.copy()
                        current.info['icc_profile'] = profile
                        converted = N.DequantizePILImage(current)
                        pixels = H.GenerateNumPyImageFromPILImage(converted, strip_useless_alpha=False)
                        frames.append(dict(index=index, shape=list(pixels.shape), pixels=list(pixels.tobytes())))
                pixels = H.GenerateNumPyImage(str(FIXTURES / 'embedded-linear.png'), HC.IMAGE_PNG, force_pil=True)
                thumb_bytes = H.GenerateThumbnailBytesFromNumPy(pixels)
                with Image.open(io.BytesIO(thumb_bytes)) as thumbnail:
                    thumb = dict(format=thumbnail.format, embedded_icc=bool(thumbnail.info.get('icc_profile')), bytes_sha256=hashlib.sha256(thumb_bytes).hexdigest())
                cases.append(dict(icc=icc, truncated=truncated, before=before, staged=staged, saved=saved, reopened=[copy.GetBoolean(k) for k in names], cache_events=list(events), decode=rows, frames=frames, thumbnail=thumb))
            dialog.resize(1000, 1100)
            dialog.show()
            QW.QApplication.processEvents()
            dialog.grab().save(str(HERE / 'fixtures' / 'image_decoder_policies_reference.png'))
        finally:
            c.pub = pub
            for name, value in zip(names, original):
                c.new_options.SetBoolean(name, value)
            H.SetEnableLoadTruncatedImages(original[1])
            N.SetDoICCProfileNormalisation(original[0])
            dialog.deleteLater()
        return dict(factory=[ClientOptions.ClientOptions().GetBoolean(k) for k in names], loaded=loaded, cancel=cancel_staged, cases=cases, legacy=legacy, files={name: hashlib.sha256((FIXTURES / name).read_bytes()).hexdigest() for name in inputs})

    return c.CallBlockingToQt(c.gui, drive)


def child(path):
    import hydrus_driver, record_api
    value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    Path(path).write_text(json.dumps(value))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        out = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        value = json.loads(out.read_text())
    path = HERE / 'fixtures' / 'image_decoder_policies.json'
    path.write_text(json.dumps(value, indent=2) + '\n')
    print(f'wrote {path}')


if __name__ == '__main__':
    main()
