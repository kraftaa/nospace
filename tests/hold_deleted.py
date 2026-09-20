#!/usr/bin/env python3
"""Fill a test filesystem, unlink the file, and keep its descriptor open."""

import os
import pathlib
import signal
import sys
import time


def main() -> None:
    path = pathlib.Path(sys.argv[1])
    ready = pathlib.Path(sys.argv[2])
    descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    block = b"\0" * 1024
    try:
        while True:
            try:
                os.write(descriptor, block)
            except OSError:
                break
        try:
            os.fsync(descriptor)
        except OSError:
            pass
        os.unlink(path)
        ready.touch()
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        while True:
            time.sleep(60)
    finally:
        os.close(descriptor)


if __name__ == "__main__":
    main()
