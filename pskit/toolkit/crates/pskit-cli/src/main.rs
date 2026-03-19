use clap::{Parser, Subcommand, ValueEnum};
use pskit_core::{annotate, contact, split};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum FormatArg {
    Auto,
    Pdb,
    Cif,
}

impl FormatArg {
    fn as_core_format(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Pdb => "pdb",
            Self::Cif => "cif",
        }
    }

    fn output_extension(self) -> &'static str {
        match self {
            Self::Pdb => "pdb",
            Self::Cif | Self::Auto => "cif",
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum ContactMode {
    D,
    Knn,
}

#[derive(Parser, Debug)]
#[command(name = "pskit-cli")]
#[command(about = "Command line toolkit powered by pskit-core")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    SplitByChain {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short = 'F', long, value_enum, default_value_t = FormatArg::Auto)]
        format: FormatArg,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(long, default_value = "")]
        prefix: String,
    },
    SplitComplex {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short = 'F', long, value_enum, default_value_t = FormatArg::Auto)]
        format: FormatArg,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(long, default_value = "")]
        prefix: String,
    },
    ExtractFragment {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short = 'F', long, value_enum, default_value_t = FormatArg::Auto)]
        format: FormatArg,
        #[arg(short, long)]
        chain: String,
        #[arg(long, default_value = None)]
        start: Option<isize>,
        #[arg(long, default_value = None)]
        end: Option<isize>,
        #[arg(short, long)]
        output: PathBuf,
    },
    ContactMap {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short = 'F', long, value_enum, default_value_t = FormatArg::Auto)]
        format: FormatArg,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long)]
        chain: Option<String>,
        #[arg(short, long, value_enum, default_value_t = ContactMode::D)]
        mode: ContactMode,
        #[arg(long)]
        k: Option<usize>,
    },
    AnnotateBindingPairs {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short = 'F', long, value_enum, default_value_t = FormatArg::Auto)]
        format: FormatArg,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 3.5)]
        cutoff: f64,
    },
}

#[derive(Serialize)]
struct ContactMapOutput {
    axis: Vec<String>,
    values: Vec<Vec<f64>>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    match cli.command {
        Commands::SplitByChain {
            input,
            format,
            output_dir,
            prefix,
        } => {
            let reader = open_reader(&input)?;
            let chunks = split::split_by_chain(reader, format.as_core_format())?;
            write_named_chunks(chunks, &output_dir, &prefix, format.output_extension())
        }
        Commands::SplitComplex {
            input,
            format,
            output_dir,
            prefix,
        } => {
            let reader = open_reader(&input)?;
            let chunks = split::split_complex(reader, format.as_core_format())?;
            write_named_chunks(chunks, &output_dir, &prefix, format.output_extension())
        }
        Commands::ExtractFragment {
            input,
            format,
            chain,
            start,
            end,
            output,
        } => {
            let reader = open_reader(&input)?;
            let (bytes, actual_start, actual_end) =
                split::extract_fragment(reader, chain, start, end, format.as_core_format())?;
            write_bytes(&output, &bytes)?;
            println!("fragment range: {actual_start}-{actual_end}");
            println!("written: {}", output.display());
            Ok(())
        }
        Commands::ContactMap {
            input,
            format,
            output,
            chain,
            mode,
            k,
        } => {
            let reader = open_reader(&input)?;
            let (axis, values) = match mode {
                ContactMode::D => contact::d_map(reader, chain, format.as_core_format())?,
                ContactMode::Knn => {
                    let actual_k = k.ok_or_else(|| "--k is required for mode=knn".to_string())?;
                    contact::knn_map(reader, chain, actual_k, format.as_core_format())?
                }
            };

            let payload = ContactMapOutput { axis, values };
            let content = serde_json::to_vec_pretty(&payload)
                .map_err(|e| format!("serialize contact map failed: {e}"))?;
            write_bytes(&output, &content)?;
            println!("written: {}", output.display());
            Ok(())
        }
        Commands::AnnotateBindingPairs {
            input,
            format,
            output,
            cutoff,
        } => {
            let reader = open_reader(&input)?;
            let pairs = annotate::compute_binding_pairs(reader, cutoff, format.as_core_format())?;
            write_pairs_tsv(&output, &pairs)?;
            println!("written: {}", output.display());
            Ok(())
        }
    }
}

fn open_reader(path: &Path) -> Result<BufReader<File>, String> {
    let file = File::open(path).map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    Ok(BufReader::new(file))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    std::fs::write(path, bytes).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

fn write_named_chunks(
    chunks: HashMap<String, Vec<u8>>,
    output_dir: &Path,
    prefix: &str,
    ext: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("failed to create {}: {e}", output_dir.display()))?;

    let mut names: Vec<_> = chunks.keys().cloned().collect();
    names.sort();

    for name in names {
        if let Some(bytes) = chunks.get(&name) {
            let output_path = output_dir.join(format!("{prefix}{name}.{ext}"));
            write_bytes(&output_path, bytes)?;
            println!("written: {}", output_path.display());
        }
    }

    Ok(())
}

fn write_pairs_tsv(path: &Path, pairs: &[(String, f64)]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }

    let mut file =
        File::create(path).map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    file.write_all(b"pair\tdistance\n")
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    for (pair, distance) in pairs {
        writeln!(file, "{pair}\t{distance:.3}")
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    }

    Ok(())
}
