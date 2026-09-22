use crate::model::{
    ContributingEvidence, Diagnosis, DiagnosisResult, Evidence, ProbeResult, ResultStatus,
};
use std::collections::BTreeSet;

pub fn diagnose(evidence: &Evidence) -> DiagnosisResult {
    let mut causes = Vec::new();
    let mut notes = Vec::new();
    let probe_errno = evidence.create_probe.outcome.errno();

    if probe_errno == Some(libc::EROFS) {
        causes.push(Diagnosis::ReadOnlyFilesystem);
    }

    if allocation_failed_with_enospc(&evidence.create_probe.outcome) {
        if create_failed_with_enospc(&evidence.create_probe.outcome)
            && evidence.filesystem.inode_accounting_supported
            && evidence.filesystem.free_inodes == 0
        {
            causes.push(Diagnosis::InodeExhaustion);
        } else if create_failed_with_enospc(&evidence.create_probe.outcome)
            && evidence.filesystem.free_inodes == 0
        {
            notes.push(format!(
                "{} reports zero free inodes, but fixed inode accounting is not supported for confirmation",
                evidence.mount.fs_type
            ));
        }
        if evidence.filesystem.available_bytes == 0 {
            causes.push(Diagnosis::BlockExhaustion);
        }
    }

    if matches!(evidence.create_probe.outcome, ProbeResult::Success)
        && evidence.inotify_probe.add_watch.errno() == Some(libc::ENOSPC)
    {
        causes.push(Diagnosis::InotifyExhaustion);
    }
    if matches!(evidence.create_probe.outcome, ProbeResult::Success)
        && inotify_instance_limit_reached(evidence)
    {
        causes.push(Diagnosis::InotifyInstanceExhaustion);
    }

    if evidence.filesystem.read_only && !causes.contains(&Diagnosis::ReadOnlyFilesystem) {
        notes.push(
            "statvfs reports a read-only mount, but the create probe did not return EROFS"
                .to_owned(),
        );
    }
    if !evidence.proc_scan.complete {
        notes.push(format!(
            "process evidence is incomplete: {} process directories were inaccessible",
            evidence.proc_scan.processes_inaccessible
        ));
    }

    let deleted_bytes = unique_deleted_bytes(evidence);
    let mut contributing = Vec::new();
    if deleted_bytes > 0 && evidence.filesystem.available_bytes == 0 {
        contributing.push(ContributingEvidence {
            kind: "deleted_open_files".to_owned(),
            bytes: deleted_bytes,
            description: format!(
                "{} allocated bytes are reclaimable from deleted-open files on this filesystem",
                deleted_bytes
            ),
        });
    }

    let status = if !causes.is_empty() {
        ResultStatus::Confirmed
    } else if matches!(evidence.create_probe.outcome, ProbeResult::Success)
        && matches!(evidence.inotify_probe.add_watch, ProbeResult::Success)
        && !evidence.filesystem.read_only
    {
        ResultStatus::NoSupportedFailure
    } else {
        ResultStatus::Unknown
    };

    if status == ResultStatus::Unknown {
        notes.push(unknown_reason(evidence));
    }

    DiagnosisResult {
        status,
        causes,
        contributing,
        notes,
    }
}

fn create_failed_with_enospc(probe: &ProbeResult) -> bool {
    matches!(
        probe,
        ProbeResult::Error { phase: crate::model::ProbePhase::Create, errno }
            if errno.code == libc::ENOSPC
    )
}

fn allocation_failed_with_enospc(probe: &ProbeResult) -> bool {
    matches!(
        probe,
        ProbeResult::Error {
            phase: crate::model::ProbePhase::Create
                | crate::model::ProbePhase::Write
                | crate::model::ProbePhase::Fsync
                | crate::model::ProbePhase::Close,
            errno,
        } if errno.code == libc::ENOSPC
    )
}

fn unique_deleted_bytes(evidence: &Evidence) -> u64 {
    let mut seen = BTreeSet::new();
    evidence
        .proc_scan
        .deleted_open
        .iter()
        .filter(|file| seen.insert((file.device, file.inode)))
        .fold(0u64, |total, file| {
            total.saturating_add(file.allocated_bytes)
        })
}

fn inotify_instance_limit_reached(evidence: &Evidence) -> bool {
    if evidence.inotify_probe.init.errno() != Some(libc::EMFILE) {
        return false;
    }
    let Some(limit) = evidence.inotify_limits.max_user_instances else {
        return false;
    };
    evidence
        .proc_scan
        .inotify_consumers
        .iter()
        .fold(0u64, |total, consumer| {
            total.saturating_add(consumer.instances)
        })
        >= limit
}

fn unknown_reason(evidence: &Evidence) -> String {
    if matches!(evidence.create_probe.outcome, ProbeResult::NotRun) {
        return "active probes were disabled; deterministic classification may be impossible"
            .to_owned();
    }
    if let ProbeResult::Error { errno, .. } = &evidence.create_probe.outcome {
        if errno.code == libc::ENOSPC {
            return "the observed ENOSPC is not explained by the checks supported by nospace"
                .to_owned();
        }
        return format!(
            "file creation failed with {}, which is not a supported confirmed diagnosis",
            errno.name
        );
    }
    if let ProbeResult::Error { errno, .. } = &evidence.inotify_probe.init {
        if errno.code == libc::EMFILE {
            return "inotify_init failed with EMFILE, but the evidence does not distinguish the per-user inotify instance limit from the process file-descriptor limit".to_owned();
        }
        return format!(
            "inotify_init failed with {}, which is not a supported confirmed diagnosis",
            errno.name
        );
    }
    if let ProbeResult::Error { errno, .. } = &evidence.inotify_probe.add_watch {
        return format!(
            "inotify_add_watch failed with {}, which is not a supported confirmed diagnosis",
            errno.name
        );
    }
    "the collected evidence does not satisfy a confirmed diagnosis".to_owned()
}
