#!/usr/bin/env bash
set -euo pipefail

if [[ "${NOSPACE_EPHEMERAL_VM:-}" != "1" ]]; then
  echo "refusing to run: set NOSPACE_EPHEMERAL_VM=1 only inside a disposable Linux VM" >&2
  exit 2
fi
if [[ "$(id -u)" != "0" ]]; then
  echo "privileged integration tests must run as root inside the disposable VM" >&2
  exit 2
fi

project_dir=$(cd "$(dirname "$0")/.." && pwd)
binary=${NOSPACE_BIN:-"$project_dir/target/release/nospace"}
test_root=$(mktemp -d /tmp/nospace-vm-tests.XXXXXX)
chmod 0755 "$test_root"
mounted=()
holder_pid=""
old_watches=$(cat /proc/sys/fs/inotify/max_user_watches)
old_instances=$(cat /proc/sys/fs/inotify/max_user_instances)

cleanup() {
  if [[ -n "$holder_pid" ]]; then
    kill "$holder_pid" 2>/dev/null || true
    wait "$holder_pid" 2>/dev/null || true
  fi
  sysctl -q -w "fs.inotify.max_user_watches=$old_watches" >/dev/null 2>&1 || true
  sysctl -q -w "fs.inotify.max_user_instances=$old_instances" >/dev/null 2>&1 || true
  for ((index=${#mounted[@]}-1; index>=0; index--)); do
    umount "${mounted[$index]}" 2>/dev/null || true
  done
  userdel nospace-test 2>/dev/null || true
  rm -rf -- "$test_root"
}
trap cleanup EXIT

assert_cause() {
  python3 - "$1" "$2" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    report = json.load(stream)
assert report["diagnosis"]["status"] == "confirmed", report
assert sys.argv[2] in report["diagnosis"]["causes"], report
PY
}

if [[ ! -x "$binary" ]]; then
  cargo build --manifest-path "$project_dir/Cargo.toml" --release
fi

echo "[1/8] block exhaustion"
block_image="$test_root/block.img"
block_mount="$test_root/block"
truncate -s 32M "$block_image"
mkfs.ext4 -q -F -m 0 "$block_image"
mkdir "$block_mount"
mount -o loop "$block_image" "$block_mount"
mounted+=("$block_mount")
dd if=/dev/zero of="$block_mount/fill" bs=1M status=none 2>/dev/null || true
# The 1 MiB writes may leave a partial block group free, enough for the
# one-byte probe. Fill the remainder at the filesystem block granularity.
dd if=/dev/zero of="$block_mount/fill" bs=4K oflag=append conv=notrunc status=none 2>/dev/null || true
dd if=/dev/zero of="$block_mount/fill" bs=1K oflag=append conv=notrunc status=none 2>/dev/null || true
"$binary" "$block_mount" --json > "$test_root/block.json"
assert_cause "$test_root/block.json" block_exhaustion
umount "$block_mount"
mounted=()

echo "[2/8] inode exhaustion"
inode_image="$test_root/inode.img"
inode_mount="$test_root/inode"
truncate -s 32M "$inode_image"
mkfs.ext4 -q -F -m 0 -N 128 "$inode_image"
mkdir "$inode_mount"
mount -o loop "$inode_image" "$inode_mount"
mounted+=("$inode_mount")
index=0
while touch "$inode_mount/item-$index" 2>/dev/null; do
  index=$((index + 1))
done
"$binary" "$inode_mount" --json > "$test_root/inode.json"
assert_cause "$test_root/inode.json" inode_exhaustion

echo "[3/8] read-only filesystem"
mount -o remount,ro "$inode_mount"
"$binary" "$inode_mount" --json > "$test_root/readonly.json"
assert_cause "$test_root/readonly.json" read_only_filesystem
umount "$inode_mount"
mounted=()

echo "[4/8] nested mount resolution"
outer="$test_root/outer"
inner="$outer/inner"
mkdir "$outer"
mount -t tmpfs -o size=8m nospace-outer "$outer"
mounted+=("$outer")
mkdir "$inner"
mount -t tmpfs -o size=4m nospace-inner "$inner"
mounted+=("$inner")
"$binary" "$inner" --json > "$test_root/nested.json"
python3 - "$test_root/nested.json" "$inner" <<'PY'
import json, os, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    report = json.load(stream)
assert report["mount"]["mount_point"] == os.path.realpath(sys.argv[2]), report["mount"]
PY
umount "$inner"
umount "$outer"
mounted=()

echo "[5/8] unknown result from an unsupported syscall failure"
mkdir "$test_root/unknown"
bash -c "ulimit -n 4; exec '$binary' '$test_root/unknown' --json" > "$test_root/unknown.json"
python3 - "$test_root/unknown.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    report = json.load(stream)
assert report["diagnosis"]["status"] == "unknown", report["diagnosis"]
PY

echo "[6/8] inotify watch exhaustion"
useradd --system --no-create-home nospace-test
watch_root="$test_root/watches"
mkdir "$watch_root"
chown -R nospace-test "$watch_root"
ready="$watch_root/ready"
sysctl -q -w fs.inotify.max_user_watches=16
# GitHub-hosted runners keep the checkout beneath a non-traversable home
# directory. Feed the helper over stdin and copy the binary into our accessible
# test root so the unprivileged test user can execute both safely.
inotify_binary="$test_root/nospace"
install -m 0755 "$binary" "$inotify_binary"
runuser -u nospace-test -- python3 - "$watch_root" "$ready" 16 < "$project_dir/tests/hold_watches.py" &
holder_pid=$!
for _ in $(seq 1 100); do
  [[ -f "$ready" ]] && break
  sleep 0.05
done
[[ -f "$ready" ]]
runuser -u nospace-test -- "$inotify_binary" "$watch_root" --json > "$test_root/inotify.json"
assert_cause "$test_root/inotify.json" inotify_exhaustion
kill "$holder_pid"
wait "$holder_pid" 2>/dev/null || true
holder_pid=""

echo "[7/8] inotify instance exhaustion"
instance_ready="$watch_root/instances-ready"
sysctl -q -w fs.inotify.max_user_instances=8
runuser -u nospace-test -- python3 - "$instance_ready" 8 < "$project_dir/tests/hold_instances.py" &
holder_pid=$!
for _ in $(seq 1 100); do
  [[ -f "$instance_ready" ]] && break
  sleep 0.05
done
[[ -f "$instance_ready" ]]
runuser -u nospace-test -- "$inotify_binary" "$watch_root" --json > "$test_root/instances.json"
assert_cause "$test_root/instances.json" inotify_instance_exhaustion
kill "$holder_pid"
wait "$holder_pid" 2>/dev/null || true
holder_pid=""

echo "[8/8] deleted-open blocks as contributing evidence"
deleted_image="$test_root/deleted.img"
deleted_mount="$test_root/deleted"
deleted_ready="$test_root/deleted-ready"
truncate -s 32M "$deleted_image"
mkfs.ext4 -q -F -m 0 "$deleted_image"
mkdir "$deleted_mount"
mount -o loop "$deleted_image" "$deleted_mount"
mounted+=("$deleted_mount")
python3 "$project_dir/tests/hold_deleted.py" "$deleted_mount/held" "$deleted_ready" &
holder_pid=$!
for _ in $(seq 1 200); do
  [[ -f "$deleted_ready" ]] && break
  sleep 0.05
done
[[ -f "$deleted_ready" ]]
"$binary" "$deleted_mount" --json > "$test_root/deleted.json"
assert_cause "$test_root/deleted.json" block_exhaustion
python3 - "$test_root/deleted.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    report = json.load(stream)
assert any(
    item["kind"] == "deleted_open_files" and item["bytes"] > 0
    for item in report["diagnosis"]["contributing"]
), report
PY

echo "all privileged integration tests passed"
