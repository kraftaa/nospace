use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MountInfo {
    pub mount_id: u64,
    pub mount_point: PathBuf,
    pub source: Option<String>,
    pub fs_type: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsStats {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub total_inodes: u64,
    pub free_inodes: u64,
    pub inode_accounting_supported: bool,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbePhase {
    OpenDirectory,
    Create,
    Write,
    Fsync,
    Close,
    Unlink,
    Init,
    AddWatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrnoInfo {
    pub code: i32,
    pub name: String,
}

impl ErrnoInfo {
    pub fn new(code: i32) -> Self {
        Self {
            code,
            name: errno_name(code).to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ProbeResult {
    Success,
    Error { phase: ProbePhase, errno: ErrnoInfo },
    NotRun,
}

impl ProbeResult {
    pub fn errno(&self) -> Option<i32> {
        match self {
            Self::Error { errno, .. } => Some(errno.code),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateProbe {
    pub outcome: ProbeResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InotifyProbe {
    pub init: ProbeResult,
    pub add_watch: ProbeResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessFd {
    pub pid: u32,
    pub process: String,
    pub fd: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeletedOpenFile {
    pub device: u64,
    pub inode: u64,
    pub path: PathBuf,
    pub allocated_bytes: u64,
    pub holders: Vec<ProcessFd>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InotifyConsumer {
    pub pid: u32,
    pub process: String,
    pub instances: u64,
    pub watches: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcScan {
    pub deleted_open: Vec<DeletedOpenFile>,
    pub inotify_consumers: Vec<InotifyConsumer>,
    pub processes_scanned: u64,
    pub processes_inaccessible: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InotifyLimits {
    pub max_user_watches: Option<u64>,
    pub max_user_instances: Option<u64>,
    pub max_queued_events: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub target: PathBuf,
    pub target_exists: bool,
    pub probe_directory: PathBuf,
    pub mount: MountInfo,
    pub filesystem: FsStats,
    pub create_probe: CreateProbe,
    pub inotify_probe: InotifyProbe,
    pub inotify_limits: InotifyLimits,
    pub proc_scan: ProcScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Diagnosis {
    InodeExhaustion,
    BlockExhaustion,
    InotifyExhaustion,
    InotifyInstanceExhaustion,
    ReadOnlyFilesystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContributingEvidence {
    pub kind: String,
    pub bytes: u64,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    Confirmed,
    NoSupportedFailure,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosisResult {
    pub status: ResultStatus,
    pub causes: Vec<Diagnosis>,
    pub contributing: Vec<ContributingEvidence>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Report<'a> {
    #[serde(flatten)]
    pub evidence: &'a Evidence,
    pub diagnosis: &'a DiagnosisResult,
}

pub fn errno_name(errno: i32) -> &'static str {
    match errno {
        libc::EACCES => "EACCES",
        libc::EDQUOT => "EDQUOT",
        libc::EEXIST => "EEXIST",
        libc::EINVAL => "EINVAL",
        libc::EIO => "EIO",
        libc::EMFILE => "EMFILE",
        libc::ENFILE => "ENFILE",
        libc::ENOENT => "ENOENT",
        libc::ENOMEM => "ENOMEM",
        libc::ENOSPC => "ENOSPC",
        libc::ENOSYS => "ENOSYS",
        libc::ENOTDIR => "ENOTDIR",
        libc::ENOTSUP => "ENOTSUP",
        libc::EPERM => "EPERM",
        libc::EROFS => "EROFS",
        _ => "UNKNOWN_ERRNO",
    }
}
