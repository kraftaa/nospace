use crate::model::MountInfo;
use std::ffi::CString;
use std::fs;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub fn parse_mountinfo(input: &str) -> Result<Vec<MountInfo>, String> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| {
            parse_line(line).map_err(|error| format!("mountinfo line {}: {error}", index + 1))
        })
        .collect()
}

fn parse_line(line: &str) -> Result<MountInfo, String> {
    let fields: Vec<&str> = line.split_ascii_whitespace().collect();
    let separator = fields
        .iter()
        .position(|field| *field == "-")
        .ok_or_else(|| "missing separator".to_owned())?;
    if separator < 6 || fields.len() < separator + 4 {
        return Err("too few fields".to_owned());
    }

    let mount_id = fields[0]
        .parse::<u64>()
        .map_err(|_| "invalid mount ID".to_owned())?;
    let mount_point = PathBuf::from(unescape_mount_field(fields[4])?);
    let fs_type = fields[separator + 1].to_owned();
    let raw_source = unescape_mount_field(fields[separator + 2])?;
    let source = (!raw_source.is_empty() && raw_source != "none").then_some(raw_source);

    let mut options: Vec<String> = fields[5]
        .split(',')
        .chain(fields[separator + 3].split(','))
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect();
    options.sort();
    options.dedup();

    Ok(MountInfo {
        mount_id,
        mount_point,
        source,
        fs_type,
        options,
    })
}

fn unescape_mount_field(field: &str) -> Result<String, String> {
    let bytes = field.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if index + 3 >= bytes.len()
                || !bytes[index + 1..=index + 3].iter().all(u8::is_ascii_digit)
            {
                return Err(format!("invalid escape in {field:?}"));
            }
            let octal = &field[index + 1..index + 4];
            let value = u8::from_str_radix(octal, 8)
                .map_err(|_| format!("invalid octal escape in {field:?}"))?;
            output.push(value);
            index += 4;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| "mount path is not UTF-8".to_owned())
}

pub fn resolve_mount(path: &Path) -> Result<MountInfo, String> {
    let contents = fs::read_to_string("/proc/self/mountinfo")
        .map_err(|error| format!("cannot read /proc/self/mountinfo: {error}"))?;
    let mounts = parse_mountinfo(&contents)?;

    if let Some(mount_id) = statx_mount_id(path)? {
        if let Some(mount) = mounts.iter().find(|mount| mount.mount_id == mount_id) {
            return Ok(mount.clone());
        }
    }

    mounts
        .into_iter()
        .filter(|mount| path.starts_with(&mount.mount_point))
        .max_by_key(|mount| mount.mount_point.components().count())
        .ok_or_else(|| format!("no mount found for {}", path.display()))
}

fn statx_mount_id(path: &Path) -> Result<Option<u64>, String> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "target path contains a NUL byte".to_owned())?;
    let mut statx = MaybeUninit::<libc::statx>::zeroed();
    // SAFETY: path is a valid NUL-terminated string and statx points to writable memory.
    let result = unsafe {
        libc::statx(
            libc::AT_FDCWD,
            path.as_ptr(),
            libc::AT_STATX_SYNC_AS_STAT,
            libc::STATX_MNT_ID,
            statx.as_mut_ptr(),
        )
    };
    if result == 0 {
        // SAFETY: a successful statx initialized the structure.
        let statx = unsafe { statx.assume_init() };
        if statx.stx_mask & libc::STATX_MNT_ID != 0 {
            return Ok(Some(statx.stx_mnt_id));
        }
        return Ok(None);
    }

    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENOSYS) | Some(libc::EINVAL) => Ok(None),
        _ => Err(format!(
            "statx failed for {}: {error}",
            path.to_string_lossy()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_escaped_paths_and_combines_options() {
        let mounts = parse_mountinfo(
            "42 31 8:2 /root /mnt/a\\040b rw,nosuid shared:1 - ext4 /dev/sda2 rw,errors=remount-ro\n",
        )
        .unwrap();
        assert_eq!(mounts[0].mount_point, PathBuf::from("/mnt/a b"));
        assert_eq!(mounts[0].source.as_deref(), Some("/dev/sda2"));
        assert!(mounts[0].options.contains(&"errors=remount-ro".to_owned()));
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse_mountinfo("1 2 3").is_err());
        assert!(parse_mountinfo("1 2 0:1 / /bad\\xx rw - ext4 none rw").is_err());
    }
}
