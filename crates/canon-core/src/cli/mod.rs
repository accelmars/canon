pub mod align;
pub mod align_apply;
pub mod audit;
pub mod template;

use audit::OutputFormat;

/// Parse CLI arguments and dispatch to the appropriate subcommand.
///
/// Returns exit code: 0 = success/conformant, 1 = drift found, 2 = error.
pub fn run(args: &[String]) -> i32 {
    run_with_io(args, &mut std::io::stdout(), &mut std::io::stderr())
}

/// Testable dispatch — accepts explicit output writers.
pub fn run_with_io(
    args: &[String],
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> i32 {
    let Some(subcommand) = args.first() else {
        let _ = writeln!(err, "canon: no subcommand given. Try 'canon audit --help'.");
        return 2;
    };
    match subcommand.as_str() {
        "audit" => parse_audit(&args[1..], out, err),
        "align" => parse_align(&args[1..], out, err),
        "template" => template::run(&args[1..], out, err),
        other => {
            let _ = writeln!(
                err,
                "canon: unknown subcommand '{}'. Try 'canon audit --help', 'canon align --help', or 'canon template --help'.",
                other
            );
            2
        }
    }
}

fn parse_audit(args: &[String], out: &mut dyn std::io::Write, err: &mut dyn std::io::Write) -> i32 {
    let mut corpus_path: Option<String> = None;
    let mut template: Option<String> = None;
    let mut format = OutputFormat::Table;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--template" | "-t" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --template requires a value");
                    return 2;
                }
                template = Some(args[i].clone());
            }
            "--format" | "-f" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --format requires a value");
                    return 2;
                }
                match args[i].parse::<OutputFormat>() {
                    Ok(f) => format = f,
                    Err(e) => {
                        let _ = writeln!(err, "error: {}", e);
                        return 2;
                    }
                }
            }
            "--help" | "-h" => {
                print_audit_help(out);
                return 0;
            }
            arg if !arg.starts_with('-') => {
                if corpus_path.is_some() {
                    let _ = writeln!(
                        err,
                        "error: unexpected positional argument '{}' (corpus-path already set)",
                        arg
                    );
                    return 2;
                }
                corpus_path = Some(arg.to_string());
            }
            other => {
                let _ = writeln!(err, "error: unknown flag '{}'", other);
                return 2;
            }
        }
        i += 1;
    }

    let Some(corpus) = corpus_path else {
        let _ = writeln!(err, "error: <corpus-path> is required");
        print_audit_help(err);
        return 2;
    };

    let Some(tmpl) = template else {
        let _ = writeln!(err, "error: --template is required");
        print_audit_help(err);
        return 2;
    };

    audit::run(&corpus, &tmpl, &format, out, err)
}

fn parse_align(args: &[String], out: &mut dyn std::io::Write, err: &mut dyn std::io::Write) -> i32 {
    let mut corpus_path: Option<String> = None;
    let mut template: Option<String> = None;
    let mut output: Option<String> = None;
    let mut fm_output: Option<String> = None;
    let mut apply = false;
    let mut gap_report_dir: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--template" | "-t" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --template requires a value");
                    return 2;
                }
                template = Some(args[i].clone());
            }
            "--output" | "-o" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --output requires a value");
                    return 2;
                }
                output = Some(args[i].clone());
            }
            "--frontmatter-output" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --frontmatter-output requires a value");
                    return 2;
                }
                fm_output = Some(args[i].clone());
            }
            "--apply" => {
                apply = true;
            }
            "--gap-report-dir" => {
                i += 1;
                if i >= args.len() {
                    let _ = writeln!(err, "error: --gap-report-dir requires a value");
                    return 2;
                }
                gap_report_dir = Some(args[i].clone());
            }
            "--help" | "-h" => {
                if apply || output.is_none() {
                    align_apply::print_help(out);
                } else {
                    align::print_help(out);
                }
                return 0;
            }
            arg if !arg.starts_with('-') => {
                if corpus_path.is_some() {
                    let _ = writeln!(
                        err,
                        "error: unexpected positional argument '{}' (corpus-path already set)",
                        arg
                    );
                    return 2;
                }
                corpus_path = Some(arg.to_string());
            }
            other => {
                let _ = writeln!(err, "error: unknown flag '{}'", other);
                return 2;
            }
        }
        i += 1;
    }

    let Some(corpus) = corpus_path else {
        let _ = writeln!(err, "error: <corpus-path> is required");
        align_apply::print_help(err);
        return 2;
    };
    let Some(tmpl) = template else {
        let _ = writeln!(err, "error: --template is required");
        align_apply::print_help(err);
        return 2;
    };

    // Route: --apply (or no --output) → orchestrator path; --output → plan-emission path.
    if apply || output.is_none() {
        align_apply::run(&corpus, &tmpl, apply, gap_report_dir.as_deref(), out, err)
    } else {
        align::run(
            &corpus,
            &tmpl,
            output.as_deref().unwrap(),
            fm_output.as_deref(),
            out,
            err,
        )
    }
}

fn print_audit_help(out: &mut dyn std::io::Write) {
    let _ = writeln!(
        out,
        "USAGE:\n  canon audit <corpus-path> --template <name|path> [--format table|json|markdown]\n\n\
         ARGS:\n  <corpus-path>   Directory to audit\n\n\
         OPTIONS:\n  --template, -t  Template name or explicit path (required)\n  \
         --format, -f    Output format: table (default), json, markdown\n\n\
         EXIT CODES:\n  0  No drift (conformant)\n  1  Drift found\n  2  Error"
    );
}
