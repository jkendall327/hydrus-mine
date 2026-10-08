#!/usr/bin/env bash
# Set up the Python environment that runs the reference client for oracle/
# recorders (~3 minutes on a fresh Ubuntu container). Idempotent.
#   scripts/setup-oracle.sh
#   QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_x.py
set -euo pipefail
cd "$(dirname "$0")/.."
if [ "$(id -u)" = 0 ]; then sudo=""; else sudo="sudo"; fi
if command -v apt-get >/dev/null; then
    $sudo apt-get update -q >/dev/null
    $sudo apt-get install -y -q libegl1 libgl1 libxkbcommon0 libfontconfig1 \
        libdbus-1-3 ffmpeg libmpv2 >/dev/null
fi
[ -x ~/pyenv/bin/python ] || python3 -m venv ~/pyenv
~/pyenv/bin/pip install -q -r oracle/requirements.txt
~/pyenv/bin/python -c "import PySide6.QtWidgets, cv2, mpv"
echo "reference environment ready: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_<x>.py"
