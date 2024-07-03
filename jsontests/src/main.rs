use anyhow::ensure;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Source path for tests errors
#[derive(Debug, Error)]
pub enum SourcePathError {
	#[error("source tests not found for: {0}")]
	SourceTestsNotFou(PathBuf),
}

///  Ethereum `jsontests` CLI tools
#[derive(Parser, Debug)]
#[command(author, version, long_about = None)]
pub struct Cli {
	/// Enable verbose mode
	#[arg(short)]
	verbose: bool,

	/// Enable very verbose mode
	#[arg(short = 'w')]
	very_verbose: bool,

	/// Verbose output failed tests only
	#[arg(short = 'f')]
	verbose_failed: bool,

	/// Ethereum Hard fork
	#[arg(short, long)]
	spec: Option<String>,

	#[command(subcommand)]
	commands: Commands,
}

/// Subcommand for CLI
#[derive(Debug, Subcommand)]
enum Commands {
	/// Run VM tests
	#[command(arg_required_else_help = true)]
	Vm {
		/// Set of json file or directory paths for VM test
		#[arg(required = true)]
		path: Vec<PathBuf>,
	},

	/// Run State tests
	#[command(arg_required_else_help = true)]
	State {
		/// Set of json file or directory paths for State test
		#[arg(required = true)]
		path: Vec<PathBuf>,
	},
}

/// Check source path - is it exist
fn check_source_path(paths: &[PathBuf]) -> anyhow::Result<()> {
	for src in paths {
		let path = Path::new(&src);
		ensure!(
			path.exists(),
			SourcePathError::SourceTestsNotFou(src.clone())
		);
	}
	Ok(())
}

/// Get source path for tests run
fn get_sources_path(commands: &Commands) -> anyhow::Result<Vec<PathBuf>> {
	let path = match commands {
		Commands::State { path } => {
			check_source_path(path)?;
			path
		}
		Commands::Vm { path } => {
			check_source_path(path)?;
			path
		}
	};
	Ok(path.clone())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args = Cli::parse();
	let path = get_sources_path(&args.commands)?;

	for src_name in path {
		let _path = Path::new(&src_name);
		// if path.is_file() {
		// 	run_vm_test_for_file(&verbose_output, path, &mut tests_result);
		// } else if path.is_dir() {
		// 	run_vm_test_for_dir(&verbose_output, path, &mut tests_result);
		// }
	}
	Ok(())
}
