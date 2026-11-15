use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use yuka_rs::graphics::Ykg;
use yuka_rs::script::{Script, TextEncoding};
use yuka_rs::vm::{Event, Vm};
use yuka_rs::{Result, archive::Archive};

#[derive(Clone, Copy, ValueEnum)]
enum Encoding {
    ShiftJis,
    Gbk,
    Utf8,
}

impl From<Encoding> for TextEncoding {
    fn from(value: Encoding) -> Self {
        match value {
            Encoding::ShiftJis => Self::ShiftJis,
            Encoding::Gbk => Self::Gbk,
            Encoding::Utf8 => Self::Utf8,
        }
    }
}

#[derive(clap::Args)]
struct Input {
    /// A standalone asset, or a YKC archive when --member is supplied.
    path: PathBuf,
    #[arg(long)]
    member: Option<String>,
    /// Maximum allocation for one asset, in bytes.
    #[arg(long, default_value_t = 268435456)]
    max_bytes: u64,
}

impl Input {
    fn read(&self) -> Result<Vec<u8>> {
        if let Some(name) = &self.member {
            let mut archive = Archive::open(&self.path)?;
            let index = archive.find(name)?;
            archive.read_entry(index, self.max_bytes)
        } else {
            use std::io::Read;
            let file = fs::File::open(&self.path)?;
            if file.metadata()?.len() > self.max_bytes {
                return Err("input exceeds --max-bytes".into());
            }
            let mut bytes = Vec::new();
            file.take(self.max_bytes.saturating_add(1))
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > self.max_bytes {
                return Err("input grew beyond --max-bytes".into());
            }
            Ok(bytes)
        }
    }
}

#[derive(Parser)]
#[command(
    version,
    about = "Yuka binary format inspection and engine reconstruction"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run an interactive opening-story demo using the original Yuka script and resources.
    Demo {
        game_dir: PathBuf,
        #[arg(long, default_value = yuka_rs::runtime::DEFAULT_SCRIPT)]
        script: String,
        #[arg(long, value_enum, default_value_t = Encoding::Gbk)]
        encoding: Encoding,
        #[arg(long, default_value_t = 100)]
        pages: usize,
        #[arg(long)]
        font: Option<PathBuf>,
        #[arg(long)]
        mute: bool,
        #[arg(long)]
        fast: bool,
        /// Execute the same runtime without a window, advancing the selected number of pages.
        #[arg(long)]
        headless: bool,
        /// Save the final rendered frame to a new PNG file.
        #[arg(long)]
        screenshot: Option<PathBuf>,
        /// Save runtime counters and approximations to a new JSON file.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Summarize all data*.ykc archives in a game directory without extracting assets.
    Inspect { game_dir: PathBuf },
    /// List members in a YKC archive as JSON, optionally filtering by a name substring.
    List {
        archive: PathBuf,
        #[arg(long)]
        filter: Option<String>,
    },
    /// Extract one raw member to an explicit new output file. Existing files are refused.
    Extract {
        archive: PathBuf,
        member: String,
        output: PathBuf,
    },
    /// Decode a YKS001 script and print symbolic instructions or full structured data.
    Script {
        #[command(flatten)]
        input: Input,
        #[arg(long, value_enum, default_value_t = Encoding::ShiftJis)]
        encoding: Encoding,
        #[arg(long)]
        json: bool,
    },
    /// Inspect a YKG container, optionally exporting image planes into a new directory.
    Graphics {
        #[command(flatten)]
        input: Input,
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
    /// Validate all YKS/YKG members and Ogg capture patterns, collecting failures and command names.
    Verify {
        game_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = Encoding::ShiftJis)]
        encoding: Encoding,
        #[arg(long, default_value_t = 268435456)]
        max_bytes: u64,
    },
    /// Step supported YKS operations until a host call, unsupported behavior, or the budget.
    Trace {
        #[command(flatten)]
        input: Input,
        #[arg(long, value_enum, default_value_t = Encoding::ShiftJis)]
        encoding: Encoding,
        #[arg(long, default_value_t = 1000)]
        max_steps: usize,
    },
}

#[derive(Serialize)]
struct ArchiveSummary {
    path: PathBuf,
    version: u8,
    file_size: u64,
    member_count: usize,
    undecodable_names: usize,
    types: BTreeMap<String, usize>,
}

fn archive_paths(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for item in fs::read_dir(directory)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().to_ascii_lowercase();
        if item.file_type()?.is_file() && name.starts_with("data") && name.ends_with(".ykc") {
            paths.push(item.path());
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("no data*.ykc archives found in the supplied directory".into());
    }
    Ok(paths)
}

fn print_json(value: &impl Serialize) -> Result<()> {
    let mut out = BufWriter::new(std::io::stdout().lock());
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Demo {
            game_dir,
            script,
            encoding,
            pages,
            font,
            mute,
            fast,
            headless,
            screenshot,
            report,
        } => {
            use yuka_rs::{desktop::Renderer, runtime::Runtime};
            let font_path = font
                .or_else(|| {
                    [
                        "C:/Windows/Fonts/msyh.ttc",
                        "C:/Windows/Fonts/simhei.ttf",
                        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                    ]
                    .into_iter()
                    .map(PathBuf::from)
                    .find(|path| path.is_file())
                })
                .ok_or("Chinese font not found; supply --font with a TTF/TTC font file")?;
            for path in [&screenshot, &report].into_iter().flatten() {
                if path.exists() {
                    return Err(format!("output already exists: {}", path.display()).into());
                }
            }
            if headless && pages == 0 {
                return Err("headless mode requires a nonzero --pages budget".into());
            }
            let mut runtime = Runtime::new(
                &game_dir,
                &script,
                encoding.into(),
                fast || headless,
                mute || headless,
                pages,
            )?;
            let result = if headless {
                (|| {
                    while !runtime.scene.finished {
                        runtime.advance()?;
                    }
                    Ok(())
                })()
            } else {
                yuka_rs::desktop::run(&mut runtime, &font_path)
            };
            if let Some(path) = report {
                let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
                serde_json::to_writer_pretty(&mut file, &runtime.report())?;
                file.flush()?;
            }
            result?;
            if let Some(path) = screenshot {
                let mut renderer = Renderer::new(fs::read(font_path)?)?;
                renderer.render(&runtime.scene, false);
                renderer.save_png(&path)?;
            }
            print_json(&runtime.report())
        }
        Command::Inspect { game_dir } => {
            let mut summaries = Vec::new();
            for path in archive_paths(&game_dir)? {
                let archive = Archive::open(&path)?;
                let mut types = BTreeMap::new();
                for entry in &archive.entries {
                    let extension = entry
                        .name
                        .rsplit_once('.')
                        .map(|(_, ext)| ext.to_ascii_lowercase())
                        .unwrap_or_default();
                    *types.entry(extension).or_insert(0) += 1;
                }
                summaries.push(ArchiveSummary {
                    path,
                    version: archive.version,
                    file_size: archive.file_size,
                    member_count: archive.entries.len(),
                    undecodable_names: archive.entries.iter().filter(|e| e.name_had_errors).count(),
                    types,
                });
            }
            print_json(&summaries)
        }
        Command::List { archive, filter } => {
            let archive = Archive::open(archive)?;
            let entries: Vec<_> = archive
                .entries
                .iter()
                .filter(|entry| filter.as_ref().is_none_or(|f| entry.name.contains(f)))
                .collect();
            print_json(&entries)
        }
        Command::Extract {
            archive,
            member,
            output,
        } => {
            let mut archive = Archive::open(archive)?;
            let index = archive.find(&member)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)?;
            let copied = archive.copy_entry(index, &mut file).and_then(|count| {
                file.flush()?;
                Ok(count)
            });
            drop(file);
            match copied {
                Ok(bytes) => {
                    print_json(&serde_json::json!({"output": output, "bytes": bytes, "raw": true}))
                }
                Err(error) => {
                    if let Err(cleanup) = fs::remove_file(&output) {
                        return Err(format!(
                            "{error}; failed to remove incomplete output {}: {cleanup}",
                            output.display()
                        )
                        .into());
                    }
                    Err(error)
                }
            }
        }
        Command::Script {
            input,
            encoding,
            json,
        } => {
            let bytes = input.read()?;
            let script = Script::parse_with_encoding(&bytes, encoding.into())?;
            let instructions = script.disassemble()?;
            if json {
                print_json(&serde_json::json!({"script": script, "instructions": instructions}))
            } else {
                let mut out = BufWriter::new(std::io::stdout().lock());
                for instruction in &instructions {
                    writeln!(out, "{}", script.format_instruction(instruction))?;
                }
                out.flush()?;
                Ok(())
            }
        }
        Command::Graphics { input, output_dir } => {
            let bytes = input.read()?;
            let image = Ykg::parse(&bytes)?;
            if let Some(directory) = output_dir {
                export_graphics(&image, &directory)?;
            }
            print_json(&image)
        }
        Command::Verify {
            game_dir,
            encoding,
            max_bytes,
        } => verify(&game_dir, encoding.into(), max_bytes),
        Command::Trace {
            input,
            encoding,
            max_steps,
        } => {
            let script = Script::parse_with_encoding(&input.read()?, encoding.into())?;
            let mut vm = Vm::new(&script);
            let mut events = Vec::new();
            let mut stop_reason = "step_limit";
            let mut error = None;
            for _ in 0..max_steps {
                match vm.step() {
                    Ok(event) => {
                        let stopped = matches!(event, Event::HostCall { .. } | Event::Halted);
                        if matches!(event, Event::HostCall { .. }) {
                            stop_reason = "host_call";
                        }
                        if matches!(event, Event::Halted) {
                            stop_reason = "halted";
                        }
                        events.push(event);
                        if stopped {
                            break;
                        }
                    }
                    Err(cause) => {
                        error = Some(cause.to_string());
                        stop_reason = "unsupported_or_invalid";
                        break;
                    }
                }
            }
            let memory: Vec<_> = vm
                .memory()
                .iter()
                .map(|(slot, value)| serde_json::json!({"slot":slot,"value":value}))
                .collect();
            print_json(
                &serde_json::json!({"pc":vm.pc,"stop_reason":stop_reason,"events":events,"memory":memory,"error":error}),
            )?;
            if let Some(error) = error {
                return Err(error.into());
            }
            Ok(())
        }
    }
}

fn export_graphics(image: &Ykg<'_>, directory: &Path) -> Result<()> {
    fs::create_dir(directory)?;
    let mut created = Vec::new();
    let result: Result<()> = (|| {
        let mut write = |name: &str, bytes: &[u8]| -> Result<()> {
            let path = directory.join(name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            created.push(path);
            file.write_all(bytes)?;
            file.flush()?;
            Ok(())
        };
        for (name, plane) in [("color", &image.color), ("alpha", &image.alpha)] {
            if let Some(plane) = plane {
                write(
                    &format!("{name}.{}", plane.extension()),
                    &plane.normalized_bytes(),
                )?;
            }
        }
        if !image.frame_data.is_empty() {
            write("frames.bin", image.frame_data)?;
        }
        write("metadata.json", &serde_json::to_vec_pretty(image)?)
    })();
    if let Err(error) = result {
        let mut cleanup_errors = Vec::new();
        for path in created {
            if let Err(cause) = fs::remove_file(&path) {
                cleanup_errors.push(format!("{}: {cause}", path.display()));
            }
        }
        if let Err(cause) = fs::remove_dir(directory) {
            cleanup_errors.push(cause.to_string());
        }
        if !cleanup_errors.is_empty() {
            return Err(format!(
                "{error}; incomplete export at {}: {}",
                directory.display(),
                cleanup_errors.join(", ")
            )
            .into());
        }
        return Err(error);
    }
    Ok(())
}

fn verify(game_dir: &Path, encoding: TextEncoding, max_bytes: u64) -> Result<()> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut functions: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut instructions = 0;
    let mut animation_frames = 0;
    let mut archive_count = 0;
    for path in archive_paths(game_dir)? {
        let mut archive = match Archive::open(&path) {
            Ok(archive) => archive,
            Err(error) => {
                failures.push(serde_json::json!({"archive":path, "error":error.to_string()}));
                continue;
            }
        };
        archive_count += 1;
        for index in 0..archive.entries.len() {
            let name = archive.entries[index].name.clone();
            let extension = name
                .rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase())
                .unwrap_or_default();
            let result: Result<()> = (|| {
                match extension.as_str() {
                    "yks" => {
                        let bytes = archive.read_entry(index, max_bytes)?;
                        let script = Script::parse_with_encoding(&bytes, encoding)?;
                        instructions += script.disassemble()?.len();
                        for name in script.functions() {
                            *functions.entry(name).or_default() += 1;
                        }
                    }
                    "ykg" => {
                        let bytes = archive.read_entry(index, max_bytes)?;
                        let image = Ykg::parse(&bytes)?;
                        animation_frames += image.frames.len();
                    }
                    "ogg" => {
                        if archive.read_prefix(index, 4)? != b"OggS" {
                            return Err("missing OggS capture pattern".into());
                        }
                    }
                    _ => {
                        *counts.entry("unvalidated_other".to_string()).or_default() += 1;
                        return Ok(());
                    }
                }
                *counts.entry(extension.clone()).or_default() += 1;
                Ok(())
            })();
            if let Err(error) = result {
                failures.push(
                    serde_json::json!({"archive":path,"member":name,"error":error.to_string()}),
                );
            }
        }
    }
    print_json(&serde_json::json!({
        "game_dir":game_dir,"encoding":encoding,"archive_count":archive_count,
        "validated":counts,"script_instruction_count":instructions,
        "animation_frame_count":animation_frames,"function_script_counts":functions,
        "failures":failures,
        "coverage":"YKC directory ranges, YKS001 sections and symbolic instruction boundaries, YKG structure, Ogg capture pattern. No gameplay, pixel decoding, audio decoding or native parity validation."
    }))?;
    if !failures.is_empty() {
        return Err(format!("{} validation failures", failures.len()).into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
