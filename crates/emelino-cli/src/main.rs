mod frontend;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const HELP: &str = "emelino — minimal Emela Flow → C experiment

Usage:
  emelino flow <source.emel> --out-dir <directory>
  emelino build <source.emel> --out-dir <directory>
  emelino build --flow <program.flow.json> --out-dir <directory>

flow writes program.flow.json and program.flow.txt (an inspection-only dump).
build writes program.c and program.h; source builds also save the Flow files.
Only exported, zero-argument functions returning an Int literal are supported.
";

enum Input {
    Source(PathBuf),
    Flow(PathBuf),
}

struct Options {
    input: Input,
    out_dir: PathBuf,
    emit_c: bool,
}

fn parse(args: impl Iterator<Item = OsString>) -> Result<Options, String> {
    let mut args = args;
    let emit_c = match args.next().as_deref() {
        Some(command) if command == "build" => true,
        Some(command) if command == "flow" => false,
        _ => return Err(format!("expected `flow` or `build`\n\n{HELP}")),
    };
    let mut input = None;
    let mut out_dir = None;
    while let Some(arg) = args.next() {
        if arg == "--out-dir" {
            if out_dir.is_some() {
                return Err("--out-dir was specified more than once".into());
            }
            out_dir = Some(next_path(&mut args, "--out-dir")?);
        } else if arg == "--flow" {
            if !emit_c || input.is_some() {
                return Err("--flow requires `build` and exactly one input".into());
            }
            input = Some(Input::Flow(next_path(&mut args, "--flow")?));
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}", arg.to_string_lossy()));
        } else if input.is_none() {
            input = Some(Input::Source(arg.into()));
        } else {
            return Err("expected exactly one input".into());
        }
    }
    Ok(Options {
        input: input.ok_or("missing input")?,
        out_dir: out_dir.ok_or("missing --out-dir")?,
        emit_c,
    })
}

fn next_path(args: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<PathBuf, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("missing path for {flag}"))?;
    if value.is_empty() || value.to_string_lossy().starts_with('-') {
        return Err(format!("missing path for {flag}"));
    }
    Ok(value.into())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn write(directory: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = directory.join(name);
    std::fs::write(&path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn run(options: Options) -> Result<(), String> {
    let (artifact, flow_files) = match options.input {
        Input::Source(path) => {
            let generated = frontend::generate(&read(&path)?, &path)?;
            let json = emelino_compiler::to_json(&generated.artifact)?;
            (generated.artifact, Some((json, generated.dump)))
        }
        Input::Flow(path) => (emelino_compiler::from_json(&read(&path)?)?, None),
    };
    // Validate everything before creating or replacing any output.
    let c = emelino_compiler::compile(&artifact)?;
    std::fs::create_dir_all(&options.out_dir)
        .map_err(|e| format!("cannot create {}: {e}", options.out_dir.display()))?;
    if let Some((json, dump)) = flow_files {
        write(&options.out_dir, "program.flow.json", &json)?;
        write(&options.out_dir, "program.flow.txt", &dump)?;
    }
    if options.emit_c {
        write(&options.out_dir, "program.c", &c.source)?;
        write(&options.out_dir, "program.h", &c.header)?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        print!("{HELP}");
        return std::process::ExitCode::SUCCESS;
    }
    if args.len() == 1 && args[0] == "--version" {
        println!(
            "emelino {} (emela {})",
            env!("CARGO_PKG_VERSION"),
            emelino_compiler::UPSTREAM_REVISION
        );
        return std::process::ExitCode::SUCCESS;
    }
    match parse(args.into_iter()).and_then(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("emelino: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
