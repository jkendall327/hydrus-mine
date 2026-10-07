# Runner mirror indirection

Cancelled run 37677733321, exact source 2e2a24281, completed diagnostic log
`stalled-mirror-check.log` proves that changing ubuntu.sources alone did not
change the effective package mirror. At 19:53:58 UTC apt read
`file:/etc/apt/apt-mirrors.txt Mirrorlist`; package requests still used
azure.archive.ubuntu.com. The 61.2 MB fonts-noto-cjk download began at
20:23:37 and remained unfinished when cancelled at 20:38:41.

Retry 37682619326 at the identical source got past package installation on the
established validation branch. Keep its source fixed while it validates the
archive repair. After publishing that bounded feature, fix the mirror-list
indirection in a small infrastructure change, with fixture coverage of both
direct URIs and mirror+file URIs, and verify actual apt output on a hosted run.
Preserve signed Ubuntu metadata and package sets. Do not claim the existing
sed command reliably switched mirrors or that a faster retry proves it did.
