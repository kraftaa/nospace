#!/usr/bin/env python3
"""Privileged integration-test helper. Run only inside an ephemeral VM."""

import ctypes
import os
import pathlib
import sys
import time


def main() -> None:
    root = pathlib.Path(sys.argv[1])
    ready = pathlib.Path(sys.argv[2])
    count = int(sys.argv[3])
    libc = ctypes.CDLL(None, use_errno=True)
    fd = libc.inotify_init1(os.O_CLOEXEC)
    if fd < 0:
        raise OSError(ctypes.get_errno(), "inotify_init1")
    for index in range(count):
        watched = root / f"watch-{index}"
        watched.mkdir()
        result = libc.inotify_add_watch(fd, os.fsencode(watched), 0x00000004)  # IN_ATTRIB
        if result < 0:
            raise OSError(ctypes.get_errno(), f"inotify_add_watch({watched})")
    ready.write_text("ready", encoding="utf-8")
    while True:
        time.sleep(1)


if __name__ == "__main__":
    main()
