use clap::{arg, command, value_parser, Arg, ArgAction, Command};
use ethjson::spec::ForkSpec;
use evm_jsontests::state as statetests;
use evm_jsontests::state::{TestExecutionResult, VerboseOutput};
use evm_jsontests::vm as vmtests;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[allow(clippy::cognitive_complexity)]
fn main() {
	let matches = command!()
		.version(env!("CARGO_PKG_VERSION"))
		.subcommand_required(true)
		.subcommand(
			Command::new("vm").about("vm tests runner").arg(
				Arg::new("PATH")
					.help("json file or directory for tests run")
					.required(true),
			),
		)
		.subcommand(
			Command::new("state")
				.about("state tests runner")
				.arg(
					arg!([PATH] "json file or directory for tests run")
						.required(true)
						.value_parser(value_parser!(PathBuf)),
				)
				.arg(arg!(-s --spec <SPEC> "Ethereum hard fork"))
				.arg(
					arg!(-v --verbose "Verbose output")
						.default_value("false")
						.action(ArgAction::SetTrue),
				)
				.arg(
					arg!(-f --verbose_failed "Verbose failed only output")
						.default_value("false")
						.action(ArgAction::SetTrue),
				)
				.arg(
					arg!(-w --very_verbose "Very verbose output")
						.default_value("false")
						.action(ArgAction::SetTrue),
				)
				.arg(
					arg!(-p --print_state "When test failed print state")
						.default_value("false")
						.action(ArgAction::SetTrue),
				),
		)
		.get_matches();

	if let Some(matches) = matches.subcommand_matches("vm") {
		for file_name in matches.get_many::<PathBuf>("PATH").unwrap() {
			let file = File::open(file_name).expect("Open failed");

			let reader = BufReader::new(file);
			let test_suite = serde_json::from_reader::<_, HashMap<String, vmtests::Test>>(reader)
				.expect("Parse test cases failed");

			for (name, test) in test_suite {
				vmtests::test(&name, test);
			}
		}
	}

	if let Some(matches) = matches.subcommand_matches("state") {
		let spec: Option<ForkSpec> = matches
			.get_one::<String>("spec")
			.and_then(|spec| spec.clone().try_into().ok());

		let verbose_output = VerboseOutput {
			verbose: matches.get_flag("verbose"),
			verbose_failed: matches.get_flag("verbose_failed"),
			very_verbose: matches.get_flag("very_verbose"),
			print_state: matches.get_flag("print_state"),
		};
		let mut tests_result = TestExecutionResult::new();
		for src_name in matches.get_many::<PathBuf>("PATH").unwrap() {
			let path = Path::new(src_name);
			assert!(path.exists(), "data source is not exist");
			if path.is_file() {
				run_test_for_file(&spec, &verbose_output, path, &mut tests_result);
			} else if path.is_dir() {
				run_test_for_dir(&spec, &verbose_output, path, &mut tests_result);
			}
		}
		println!("\nTOTAL: {}", tests_result.total);
		println!("FAILED: {}\n", tests_result.failed);
	}
}

fn run_test_for_dir(
	spec: &Option<ForkSpec>,
	verbose_output: &VerboseOutput,
	dir_name: &Path,
	tests_result: &mut TestExecutionResult,
) {
	if should_skip(dir_name) {
		println!("Skipping test case {:?}", dir_name);
		return;
	}
	for entry in fs::read_dir(dir_name).unwrap() {
		let entry = entry.unwrap();
		if let Some(s) = entry.file_name().to_str() {
			if s.starts_with('.') {
				continue;
			}
		}
		let path = entry.path();
		if path.is_dir() {
			run_test_for_dir(spec, verbose_output, path.as_path(), tests_result);
		} else {
			run_test_for_file(spec, verbose_output, path.as_path(), tests_result);
		}
	}
}

fn run_test_for_file(
	spec: &Option<ForkSpec>,
	verbose_output: &VerboseOutput,
	file_name: &Path,
	tests_result: &mut TestExecutionResult,
) {
	if should_skip(file_name) {
		if verbose_output.verbose {
			println!("Skipping test case {:?}", file_name);
		}
		return;
	}
	if verbose_output.verbose {
		println!(
			"RUN for: {}",
			short_test_file_name(file_name.to_str().unwrap())
		);
	}
	let file = File::open(file_name).expect("Open file failed");

	let reader = BufReader::new(file);
	let test_suite = serde_json::from_reader::<_, HashMap<String, statetests::Test>>(reader)
		.expect("Parse test cases failed");

	for (name, test) in test_suite {
		let test_res = statetests::test(verbose_output.clone(), &name, test, spec.clone());

		if test_res.failed > 0 {
			if verbose_output.verbose {
				println!("Tests count:\t{}", test_res.total);
				println!(
					"Failed:\t\t{} - {}\n",
					test_res.failed,
					short_test_file_name(file_name.to_str().unwrap())
				);
			} else if verbose_output.verbose_failed {
				println!(
					"RUN for: {}",
					short_test_file_name(file_name.to_str().unwrap())
				);
				println!("Tests count:\t{}", test_res.total);
				println!(
					"Failed:\t\t{} - {}\n",
					test_res.failed,
					short_test_file_name(file_name.to_str().unwrap())
				);
			}
		} else if verbose_output.verbose {
			println!("Tests count: {}\n", test_res.total);
		}

		tests_result.merge(test_res);
	}
}

fn short_test_file_name(name: &str) -> String {
	let res: Vec<_> = name.split("GeneralStateTests/").collect();
	if res.len() > 1 {
		res[1].to_string()
	} else {
		res[0].to_string()
	}
}

const SKIPPED_CASES: &[&str] = &[
	// funky test with `bigint 0x00` value in json :) not possible to happen on mainnet and require
	// custom json parser. https://github.com/ethereum/tests/issues/971
	"stTransactionTest/ValueOverflow",
	"stTransactionTest/ValueOverflowParis",
	// Long execution
	"stTimeConsuming/static_Call50000_sha256",
	"vmPerformance/loopMul",
	"stTimeConsuming/CALLBlake2f_MaxRounds",
	// Skip python-specific tests
	"eip4844_blobs",
	"eip3860_initcode",
	// Cancun blob txs
	"stEIP4844-blobtransactions",
	// KZG-precompile not supported
	"stPreCompiledContracts/precompsEIP2929Cancun",
	"stPreCompiledContracts/idPrecomps",
	"stSpecialTest/failed_tx_xcf416c53_Paris",
];

fn should_skip(path: &Path) -> bool {
	let matches = |case: &str| {
		let file_stem = path.file_stem().unwrap();
		let dir_path = path.parent().unwrap();
		let dir_name = dir_path.file_name().unwrap();
		Path::new(dir_name).join(file_stem) == Path::new(case)
			|| Path::new(dir_name) == Path::new(case)
	};

	for case in SKIPPED_CASES {
		if matches(case) {
			return true;
		}
	}
	false
}
