#!/bin/sh
# Authored transport only: run the real Ffmpeg process path against a FIFO.
# Keep this executable read-only during tests; derive isolated paths from the
# temporary symlink used to invoke it, without process-wide environment changes.
transport_dir=${0%/*}
printf '%s' "$$" > "$transport_dir/pid"
read -r reply < "$transport_dir/release"
if [ "$1" = '-version' ]; then
    printf 'ffmpeg version authored Copyright local\n'
else
    printf 'ffmpeg version authored\nInput #0, authored, from local:\n' >&2
fi
