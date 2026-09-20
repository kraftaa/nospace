# nospace

`nospace` is a Linux-only CLI that explains a deliberately small set of
filesystem-related failures using syscall and kernel evidence. It returns
`UNKNOWN` instead of guessing when the evidence does not support a diagnosis.

```text
$ nospace /var/lib/app
Target: /var/lib/app
Filesystem: /dev/nvme0n1p2
Mount: /var
Type: ext4

Checks:
  create file      ENOSPC during create
  available space  82.4 GiB
  free inodes      0
  inotify init     OK
  inotify watch    OK
  read-only        no
  mount option ro  no

CONFIRMED: inode exhaustion
```

## Evidence, not likelihood

The initial classifier confirms only:

- Block exhaustion: a create/write/fsync probe returned `ENOSPC` and
  `statvfs` reports zero bytes available to the current user.
- Inode exhaustion: that probe returned `ENOSPC`, the filesystem reports zero
  free inodes, and the filesystem has fixed inode accounting supported by
  `nospace` (currently ext2/3/4).
- Read-only filesystem: the create probe returned `EROFS`.
- Inotify exhaustion: file creation succeeded and `inotify_add_watch` returned
  `ENOSPC`.

Deleted-open files are detected by `st_nlink == 0`, grouped by device/inode,
and measured using allocated blocks rather than apparent file size. They are
reported as contributing evidence only when the filesystem has no available
blocks.

Anything else is `UNKNOWN`.

## Install

The Python distribution is called `nospace-cli`, but it installs the native
`nospace` executable. It does not require Cargo or compile Rust on the user's
machine.

```bash
pipx install nospace-cli
# or
uv tool install nospace-cli
# or, inside a virtual environment
python -m pip install nospace-cli
```

Do not run `pip install nospace`: that PyPI name belongs to an unrelated file
renaming utility.

Prebuilt wheels target glibc- and musl-based Linux on x86-64 and ARM64. Other
platforms fail installation instead of attempting a source build. `nospace` is
Linux-only.

## Usage

```bash
nospace PATH
nospace PATH --json
nospace PATH --verbose
nospace PATH --no-probe
```

The target must exist. If it is a file, `nospace` creates its temporary probe
in the parent directory.

`--no-probe` performs no file creation and adds no inotify watch. It still
reads filesystem statistics and accessible `/proc` information, but usually
cannot produce a confirmed diagnosis.

## What it inspects

- The exact mount ID from `statx(STATX_MNT_ID)`, matched to
  `/proc/self/mountinfo`
- `statvfs` block, inode and read-only state
- A collision-safe `openat(O_CREAT | O_EXCL)` → unlink → write → `fsync` probe
- `inotify_init1` and `inotify_add_watch` separately
- Inotify limits and visible watches belonging to the current UID
- Accessible deleted-open regular files on the target filesystem

`/proc` inspection can be incomplete because of permissions or because a
process exits during the scan. That state is explicit in text and JSON output;
inaccessible processes are never counted as zero usage. Running as root reveals
more process evidence but does not change the classification rules.

## Safety

`nospace` never deletes user files, kills processes, changes limits, remounts a
filesystem, changes permissions or performs cleanup. Its only default
mutations are a uniquely named one-byte temporary file that is immediately
removed and a temporary inotify watch that is closed before exit.

The probe unlinks its random pathname immediately after opening it, then calls
`fsync` so delayed-allocation failures are observed without leaving a named
probe behind. If that early unlink fails, the exact failure is reported.

## Unsupported causes

Version 0.1 intentionally does not diagnose:

- user, group or project quotas (`EDQUOT` or filesystem-specific behavior)
- Btrfs data/metadata allocation
- overlay upper-layer exhaustion
- network-filesystem server limits
- oversized-directory constraints
- filesystem corruption

These remain `UNKNOWN` even when a human might have a good hypothesis.

## Build and test

```bash
cargo build --release
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Safe tests run on any Linux machine. The block, inode, read-only, nested-mount,
inotify and deleted-open contribution tests require a disposable Linux VM with
root access:

```bash
NOSPACE_EPHEMERAL_VM=1 sudo -E tests/privileged.sh
```

The script refuses to run without the explicit marker. It mounts disposable
filesystems under a fresh `/tmp` directory and temporarily lowers an inotify
limit; therefore it must never be run on a developer workstation or shared
host.

## Why not parse `df`, `findmnt`, or `lsof`?

Those tools are useful, but their human-oriented output is not a stable
evidence API. `nospace` calls the relevant interfaces directly and keeps
collection separate from the pure deterministic classifier.
