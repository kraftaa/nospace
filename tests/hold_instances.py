#!/usr/bin/env python3
"""Privileged integration-test helper. Run only inside an ephemeral VM."""

import ctypes
import os
import pathlib
import sys
import time


def main() -> None:
    ready = pathlib.Path(sys.argv[1])
    count = int(sys.argv[2])
    libc = ctypes.CDLL(None, use_errno=True)
    descriptors = []
    for _ in range(count):
        descriptor = libc.inotify_init1(os.O_CLOEXEC)
        if descriptor < 0:
            raise OSError(ctypes.get_errno(), "inotify_init1")
        descriptors.append(descriptor)
    ready.write_text("ready", encoding="utf-8")
    while True:
        time.sleep(1)


if __name__ == "__main__":
    main()
