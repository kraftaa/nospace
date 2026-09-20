use nospace::model::Report;
use nospace::{CollectOptions, collect, diagnose};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "nospace — evidence-based Linux ENOSPC diagnosis

USAGE:
    nospace PATH [--json] [--verbose] [--no-probe]

OPTIONS:
    --json       Emit machine-readable JSON
    --verbose    Show all process evidence instead of the top entries
    --no-probe   Do not create a temporary file or add an inotify watch
    -h, --help   Show this help
    -V, --version
                 Show the version";

#[derive(Default)]
struct Args {
    path: Option<PathBuf>,
    json: bool,
    verbose: bool,
    no_probe: bool,
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let evidence = match collect(
        args.path.as_deref().expect("validated path"),
        CollectOptions {
            no_probe: args.no_probe,
        },
    ) {
        Ok(evidence) => evidence,
        Err(error) => {
            eprintln!("nospace: {error}");
            return ExitCode::from(2);
        }
    };
    let diagnosis = diagnose(&evidence);
    if args.json {
        let report = Report {
            evidence: &evidence,
            diagnosis: &diagnosis,
        };
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("nospace: cannot serialize report: {error}");
                return ExitCode::from(2);
            }
        }
    } else {
        println!(
            "{}",
            nospace::render::text(&evidence, &diagnosis, args.verbose)
        );
    }
    ExitCode::SUCCESS
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut parsed = Args::default();
    for argument in env::args_os().skip(1) {
        match argument.to_str() {
            Some("-h" | "--help") => {
                println!("{HELP}");
                return Ok(None);
            }
            Some("-V" | "--version") => {
                println!("nospace {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Some("--json") => parsed.json = true,
            Some("--verbose") => parsed.verbose = true,
            Some("--no-probe") => parsed.no_probe = true,
            Some(value) if value.starts_with('-') => {
                return Err(format!("unknown option: {value}"));
            }
            _ if parsed.path.is_some() => return Err("expected exactly one PATH".to_owned()),
            _ => parsed.path = Some(PathBuf::from(argument)),
        }
    }
    if parsed.path.is_none() {
        return Err("PATH is required".to_owned());
    }
    Ok(Some(parsed))
}
