use crate::model::{Diagnosis, DiagnosisResult, Evidence, ProbePhase, ProbeResult, ResultStatus};
use std::fmt::Write;

pub fn text(evidence: &Evidence, diagnosis: &DiagnosisResult, verbose: bool) -> String {
    let mut output = String::new();
    writeln!(output, "Target: {}", evidence.target.display()).unwrap();
    if !evidence.target_exists {
        writeln!(
            output,
            "Target exists: no (probing nearest existing parent)"
        )
        .unwrap();
    }
    if evidence.probe_directory != evidence.target {
        writeln!(
            output,
            "Probe directory: {}",
            evidence.probe_directory.display()
        )
        .unwrap();
    }
    writeln!(
        output,
        "Filesystem: {}",
        evidence.mount.source.as_deref().unwrap_or("unknown")
    )
    .unwrap();
    writeln!(output, "Mount: {}", evidence.mount.mount_point.display()).unwrap();
    writeln!(output, "Type: {}", evidence.mount.fs_type).unwrap();

    writeln!(output, "\nChecks:").unwrap();
    writeln!(
        output,
        "  file probe       {}",
        probe_label(&evidence.create_probe.outcome)
    )
    .unwrap();
    writeln!(
        output,
        "  available space  {}",
        human_bytes(evidence.filesystem.available_bytes)
    )
    .unwrap();
    writeln!(
        output,
        "  free inodes      {}{}",
        evidence.filesystem.free_inodes,
        if evidence.filesystem.inode_accounting_supported {
            ""
        } else {
            " (not authoritative for this filesystem)"
        }
    )
    .unwrap();
    writeln!(
        output,
        "  inotify init     {}",
        probe_label(&evidence.inotify_probe.init)
    )
    .unwrap();
    writeln!(
        output,
        "  inotify watch    {}",
        probe_label(&evidence.inotify_probe.add_watch)
    )
    .unwrap();
    writeln!(
        output,
        "  read-only        {}",
        if evidence.filesystem.read_only {
            "yes"
        } else {
            "no"
        }
    )
    .unwrap();
    writeln!(
        output,
        "  mount option ro  {}",
        if evidence.mount.options.iter().any(|option| option == "ro") {
            "yes"
        } else {
            "no"
        }
    )
    .unwrap();

    match diagnosis.status {
        ResultStatus::Confirmed => {
            for cause in &diagnosis.causes {
                writeln!(output, "\nCONFIRMED: {}", diagnosis_label(*cause)).unwrap();
            }
        }
        ResultStatus::NoSupportedFailure => {
            writeln!(output, "\nOK: no supported failure detected").unwrap();
        }
        ResultStatus::Unknown => {
            writeln!(output, "\nUNKNOWN").unwrap();
        }
    }

    if !diagnosis.contributing.is_empty() {
        for finding in &diagnosis.contributing {
            writeln!(
                output,
                "\nCONTRIBUTING: {} is held by deleted-open files",
                human_bytes(finding.bytes)
            )
            .unwrap();
        }
    }

    if !evidence.proc_scan.deleted_open.is_empty()
        && (verbose || !diagnosis.contributing.is_empty())
    {
        writeln!(output, "\nDeleted-open files:").unwrap();
        let limit = if verbose { usize::MAX } else { 10 };
        for file in evidence.proc_scan.deleted_open.iter().take(limit) {
            let holder = file.holders.first();
            if let Some(holder) = holder {
                writeln!(
                    output,
                    "  {} PID {} FD {}",
                    holder.process, holder.pid, holder.fd
                )
                .unwrap();
            }
            writeln!(output, "    {}", file.path.display()).unwrap();
            writeln!(
                output,
                "    allocated: {}",
                human_bytes(file.allocated_bytes)
            )
            .unwrap();
            if file.holders.len() > 1 {
                writeln!(output, "    holders: {}", file.holders.len()).unwrap();
            }
        }
        if !verbose && evidence.proc_scan.deleted_open.len() > limit {
            writeln!(
                output,
                "  … {} more; use --verbose",
                evidence.proc_scan.deleted_open.len() - limit
            )
            .unwrap();
        }
    }

    let show_inotify_details = verbose || diagnosis.causes.contains(&Diagnosis::InotifyExhaustion);
    if show_inotify_details {
        let observed_instances = evidence
            .proc_scan
            .inotify_consumers
            .iter()
            .fold(0u64, |total, consumer| {
                total.saturating_add(consumer.instances)
            });
        let observed_watches = evidence
            .proc_scan
            .inotify_consumers
            .iter()
            .fold(0u64, |total, consumer| {
                total.saturating_add(consumer.watches)
            });
        writeln!(output, "\nInotify usage (current UID):").unwrap();
        writeln!(
            output,
            "  watches                {}",
            usage_label(
                observed_watches,
                evidence.inotify_limits.max_user_watches,
                evidence.proc_scan.complete
            )
        )
        .unwrap();
        writeln!(
            output,
            "  instances              {}",
            usage_label(
                observed_instances,
                evidence.inotify_limits.max_user_instances,
                evidence.proc_scan.complete
            )
        )
        .unwrap();
        writeln!(
            output,
            "  assessment             {}",
            inotify_assessment(evidence)
        )
        .unwrap();
    }

    if show_inotify_details && !evidence.proc_scan.inotify_consumers.is_empty() {
        writeln!(output, "\nTop inotify consumers:").unwrap();
        writeln!(
            output,
            "  PID       PROCESS                  INSTANCES  WATCHES"
        )
        .unwrap();
        let limit = if verbose { usize::MAX } else { 10 };
        for consumer in evidence.proc_scan.inotify_consumers.iter().take(limit) {
            writeln!(
                output,
                "  {:<9} {:<24} {:<10} {}",
                consumer.pid, consumer.process, consumer.instances, consumer.watches
            )
            .unwrap();
        }
    }

    if !diagnosis.notes.is_empty() {
        writeln!(output, "\nNotes:").unwrap();
        for note in &diagnosis.notes {
            writeln!(output, "  - {note}").unwrap();
        }
    }

    if !evidence.proc_scan.complete {
        writeln!(
            output,
            "\nSome process information was unavailable due to permissions or process exit."
        )
        .unwrap();
    }
    output.trim_end().to_owned()
}

fn probe_label(probe: &ProbeResult) -> String {
    match probe {
        ProbeResult::Success => "OK".to_owned(),
        ProbeResult::NotRun => "NOT RUN".to_owned(),
        ProbeResult::Error { phase, errno } => {
            format!("{} during {}", errno.name, phase_label(*phase))
        }
    }
}

fn phase_label(phase: ProbePhase) -> &'static str {
    match phase {
        ProbePhase::OpenDirectory => "open_directory",
        ProbePhase::Create => "create",
        ProbePhase::Write => "write",
        ProbePhase::Fsync => "fsync",
        ProbePhase::Close => "close",
        ProbePhase::Unlink => "unlink",
        ProbePhase::Init => "inotify_init",
        ProbePhase::AddWatch => "inotify_add_watch",
    }
}

fn diagnosis_label(diagnosis: Diagnosis) -> &'static str {
    match diagnosis {
        Diagnosis::InodeExhaustion => "inode exhaustion",
        Diagnosis::BlockExhaustion => "filesystem space exhausted",
        Diagnosis::InotifyExhaustion => "inotify resource exhaustion",
        Diagnosis::ReadOnlyFilesystem => "filesystem is read-only",
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn human_count(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.bytes().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push(',');
        }
        output.push(char::from(digit));
    }
    output
}

fn usage_label(observed: u64, limit: Option<u64>, complete: bool) -> String {
    let observed_label = human_count(observed);
    let Some(limit) = limit else {
        return format!("{observed_label} observed; limit unavailable");
    };
    if !complete {
        return format!(
            ">= {observed_label} / {} (lower bound; process scan incomplete)",
            human_count(limit)
        );
    }
    if limit == 0 {
        return format!("{observed_label} / 0");
    }
    format!(
        "{observed_label} / {} ({}; {} available)",
        human_count(limit),
        utilization(observed, limit),
        human_count(limit.saturating_sub(observed))
    )
}

fn utilization(observed: u64, limit: u64) -> String {
    if observed == 0 {
        return "0%".to_owned();
    }
    let percent = observed as f64 * 100.0 / limit as f64;
    if percent < 0.1 {
        "<0.1%".to_owned()
    } else {
        format!("{percent:.1}%")
    }
}

fn inotify_assessment(evidence: &Evidence) -> &'static str {
    if evidence.inotify_probe.add_watch.errno() == Some(libc::ENOSPC) {
        "watch creation failed: inotify resources exhausted"
    } else if matches!(evidence.inotify_probe.init, ProbeResult::Success)
        && matches!(evidence.inotify_probe.add_watch, ProbeResult::Success)
    {
        "new instance and watch created successfully"
    } else {
        "probe did not establish available inotify capacity"
    }
}
