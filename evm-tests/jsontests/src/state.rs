use crate::utils::*;
use ethjson::spec::ForkSpec;
use evm::backend::{ApplyBackend, MemoryAccount, MemoryBackend, MemoryVicinity};
use evm::executor::stack::{
	MemoryStackState, PrecompileFailure, PrecompileFn, PrecompileOutput, StackExecutor,
	StackSubstateMetadata,
};
use evm::{Config, Context, ExitError, ExitSucceed};
use lazy_static::lazy_static;
use libsecp256k1::SecretKey;
use primitive_types::{H160, H256, U256};
use serde::Deserialize;
use sha3::{Digest, Keccak256};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct FailedTestDetails {
	pub name: String,
	pub spec: ForkSpec,
	pub index: usize,
	pub expected_hash: H256,
	pub actual_hash: H256,
}

#[derive(Clone, Debug)]
pub struct TestExecutionResult {
	pub total: u64,
	pub failed: u64,
	pub failed_tests: Vec<FailedTestDetails>,
}

impl TestExecutionResult {
	#[allow(clippy::new_without_default)]
	pub const fn new() -> Self {
		Self {
			total: 0,
			failed: 0,
			failed_tests: Vec::new(),
		}
	}

	pub fn merge(&mut self, src: Self) {
		self.failed_tests.extend(src.failed_tests);
		self.total += src.total;
		self.failed += src.failed;
	}
}

#[derive(Deserialize, Debug)]
pub struct Test(ethjson::test_helpers::state::State);

impl Test {
	pub fn unwrap_to_pre_state(&self) -> BTreeMap<H160, MemoryAccount> {
		unwrap_to_state(&self.0.pre_state)
	}

	pub fn unwrap_caller(&self) -> H160 {
		let hash: H256 = self.0.transaction.secret.unwrap().into();
		let mut secret_key = [0; 32];
		secret_key.copy_from_slice(hash.as_bytes());
		let secret = SecretKey::parse(&secret_key);
		let public = libsecp256k1::PublicKey::from_secret_key(&secret.unwrap());
		let mut res = [0u8; 64];
		res.copy_from_slice(&public.serialize()[1..65]);

		H160::from(H256::from_slice(Keccak256::digest(res).as_slice()))
	}

	pub fn unwrap_to_vicinity(&self, spec: &ForkSpec) -> Option<MemoryVicinity> {
		let block_base_fee_per_gas = self.0.env.block_base_fee_per_gas.0;
		let gas_price = if self.0.transaction.gas_price.0.is_zero() {
			let max_fee_per_gas = self.0.transaction.max_fee_per_gas.0;

			// max_fee_per_gas is only defined for London and later
			if !max_fee_per_gas.is_zero() && spec < &ForkSpec::London {
				return None;
			}

			// Cannot specify a lower fee than the base fee
			if max_fee_per_gas < block_base_fee_per_gas {
				return None;
			}

			let max_priority_fee_per_gas = self.0.transaction.max_priority_fee_per_gas.0;

			// priority fee must be lower than regaular fee
			if max_fee_per_gas < max_priority_fee_per_gas {
				return None;
			}

			let priority_fee_per_gas = std::cmp::min(
				max_priority_fee_per_gas,
				max_fee_per_gas - block_base_fee_per_gas,
			);
			priority_fee_per_gas + block_base_fee_per_gas
		} else {
			self.0.transaction.gas_price.0
		};

		// gas price cannot be lower than base fee
		if gas_price < block_base_fee_per_gas {
			return None;
		}

		let block_randomness = if spec.is_eth2() {
			self.0.env.random.map(|r| {
				// Convert between U256 and H256. U256 is in little-endian but since H256 is just
				// a string-like byte array, it's big endian (MSB is the first element of the array).
				//
				// Byte order here is important because this opcode has the same value as DIFFICULTY
				// (0x44), and so for older forks of Ethereum, the threshold value of 2^64 is used to
				// distinguish between the two: if it's below, the value corresponds to the DIFFICULTY
				// opcode, otherwise to the PREVRANDAO opcode.
				u256_to_h256(r.0)
			})
		} else {
			None
		};

		Some(MemoryVicinity {
			gas_price,
			origin: self.unwrap_caller(),
			block_hashes: Vec::new(),
			block_number: self.0.env.number.into(),
			block_coinbase: self.0.env.author.into(),
			block_timestamp: self.0.env.timestamp.into(),
			block_difficulty: self.0.env.difficulty.into(),
			block_gas_limit: self.0.env.gas_limit.into(),
			chain_id: U256::one(),
			block_base_fee_per_gas,
			block_randomness,
			blob_base_fee: self.0.env.blob_base_fee,
		})
	}
}

lazy_static! {
	static ref ISTANBUL_BUILTINS: BTreeMap<H160, ethcore_builtin::Builtin> =
		JsonPrecompile::builtins("./res/istanbul_builtins.json");
}

lazy_static! {
	static ref BERLIN_BUILTINS: BTreeMap<H160, ethcore_builtin::Builtin> =
		JsonPrecompile::builtins("./res/berlin_builtins.json");
}

macro_rules! precompile_entry {
	($map:expr, $builtins:expr, $index:expr) => {
		let x: PrecompileFn =
			|input: &[u8], gas_limit: Option<u64>, _context: &Context, _is_static: bool| {
				let builtin = $builtins.get(&H160::from_low_u64_be($index)).unwrap();
				Self::exec_as_precompile(builtin, input, gas_limit)
			};
		$map.insert(H160::from_low_u64_be($index), x);
	};
}

pub struct JsonPrecompile;

impl JsonPrecompile {
	pub fn precompile(spec: &ForkSpec) -> Option<BTreeMap<H160, PrecompileFn>> {
		match spec {
			ForkSpec::Istanbul => {
				let mut map = BTreeMap::new();
				precompile_entry!(map, ISTANBUL_BUILTINS, 1);
				precompile_entry!(map, ISTANBUL_BUILTINS, 2);
				precompile_entry!(map, ISTANBUL_BUILTINS, 3);
				precompile_entry!(map, ISTANBUL_BUILTINS, 4);
				precompile_entry!(map, ISTANBUL_BUILTINS, 5);
				precompile_entry!(map, ISTANBUL_BUILTINS, 6);
				precompile_entry!(map, ISTANBUL_BUILTINS, 7);
				precompile_entry!(map, ISTANBUL_BUILTINS, 8);
				precompile_entry!(map, ISTANBUL_BUILTINS, 9);
				Some(map)
			}
			ForkSpec::Berlin => {
				let mut map = BTreeMap::new();
				precompile_entry!(map, BERLIN_BUILTINS, 1);
				precompile_entry!(map, BERLIN_BUILTINS, 2);
				precompile_entry!(map, BERLIN_BUILTINS, 3);
				precompile_entry!(map, BERLIN_BUILTINS, 4);
				precompile_entry!(map, BERLIN_BUILTINS, 5);
				precompile_entry!(map, BERLIN_BUILTINS, 6);
				precompile_entry!(map, BERLIN_BUILTINS, 7);
				precompile_entry!(map, BERLIN_BUILTINS, 8);
				precompile_entry!(map, BERLIN_BUILTINS, 9);
				Some(map)
			}
			// precompiles for London and Berlin are the same
			ForkSpec::London => Self::precompile(&ForkSpec::Berlin),
			// precompiles for Merge and Berlin are the same
			ForkSpec::Merge => Self::precompile(&ForkSpec::Berlin),
			// precompiles for Paris and Berlin are the same
			ForkSpec::Paris => Self::precompile(&ForkSpec::Berlin),
			// precompiles for Shanghai and Berlin are the same
			ForkSpec::Shanghai => Self::precompile(&ForkSpec::Berlin),
			ForkSpec::Cancun => Self::precompile(&ForkSpec::Berlin),
			_ => None,
		}
	}

	fn builtins(spec_path: &str) -> BTreeMap<H160, ethcore_builtin::Builtin> {
		let reader = std::fs::File::open(spec_path).expect(spec_path);
		let builtins: BTreeMap<ethjson::hash::Address, ethjson::spec::builtin::BuiltinCompat> =
			serde_json::from_reader(reader).unwrap();
		builtins
			.into_iter()
			.map(|(address, builtin)| {
				(
					address.into(),
					ethjson::spec::Builtin::from(builtin).try_into().unwrap(),
				)
			})
			.collect()
	}

	fn exec_as_precompile(
		builtin: &ethcore_builtin::Builtin,
		input: &[u8],
		gas_limit: Option<u64>,
	) -> Result<(PrecompileOutput, u64), PrecompileFailure> {
		let cost = builtin.cost(input, 0);

		if let Some(target_gas) = gas_limit {
			if cost > U256::from(u64::MAX) || target_gas < cost.as_u64() {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::OutOfGas,
				});
			}
		}

		let mut output = Vec::new();
		match builtin.execute(input, &mut parity_bytes::BytesRef::Flexible(&mut output)) {
			Ok(()) => Ok((
				PrecompileOutput {
					exit_status: ExitSucceed::Stopped,
					output,
				},
				cost.as_u64(),
			)),
			Err(e) => Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(e.into()),
			}),
		}
	}
}

pub fn test(
	name: &str,
	test: Test,
	print_output: bool,
	specific_spec: Option<ForkSpec>,
) -> TestExecutionResult {
	use std::thread;

	const STACK_SIZE: usize = 16 * 1024 * 1024;

	let name = name.to_string();
	// Spawn thread with explicit stack size
	let child = thread::Builder::new()
		.stack_size(STACK_SIZE)
		.spawn(move || test_run(&name, test, print_output, specific_spec))
		.unwrap();

	// Wait for thread to join
	child.join().unwrap()
}

fn test_run(
	name: &str,
	test: Test,
	print_output: bool,
	specific_spec: Option<ForkSpec>,
) -> TestExecutionResult {
	let mut tests_result = TestExecutionResult::new();
	for (spec, states) in &test.0.post_states {
		// Run tests for specific SPEC (Hard fork)
		if let Some(s) = specific_spec.as_ref() {
			if s != spec {
				continue;
			}
		}
		let (gasometer_config, delete_empty) = match spec {
			ForkSpec::Istanbul => (Config::istanbul(), true),
			ForkSpec::Berlin => (Config::berlin(), true),
			ForkSpec::London => (Config::london(), true),
			ForkSpec::Merge => (Config::merge(), true),
			ForkSpec::Paris => (Config::merge(), true),
			ForkSpec::Shanghai => (Config::shanghai(), true),
			ForkSpec::Cancun => (Config::cancun(), false),
			spec => {
				println!("Skip spec {spec:?}");
				continue;
			}
		};

		let original_state = test.unwrap_to_pre_state();
		let vicinity = test.unwrap_to_vicinity(spec);
		if vicinity.is_none() {
			let h = states.first().unwrap().hash.0;
			// if vicinity could not be computed then the transaction was invalid so we simply
			// check the original state and move on
			let (is_valid_hash, actual_hash) = assert_valid_hash(&h, &original_state);
			if !is_valid_hash {
				tests_result.failed_tests.push(FailedTestDetails {
					expected_hash: h,
					actual_hash,
					index: 0,
					name: String::from_str(name).unwrap(),
					spec: spec.clone(),
				});
				tests_result.failed += 1;
			}
			continue;
		}
		let vicinity = vicinity.unwrap();
		let caller = test.unwrap_caller();
		let caller_balance = original_state
			.get(&caller)
			.map_or_else(U256::zero, |acc| acc.balance);

		for (i, state) in states.iter().enumerate() {
			if print_output {
				print!("Running {}:{:?}:{} ... ", name, spec, i);
				flush();
			}

			let transaction = test.0.transaction.select(&state.indexes);
			let mut backend = MemoryBackend::new(&vicinity, original_state.clone());

			// Test case may be expected to fail with an unsupported tx type if the current fork is
			// older than Berlin (see EIP-2718). However, this is not implemented in sputnik itself and rather
			// in the code hosting sputnik. https://github.com/rust-blockchain/evm/pull/40
			let expect_tx_type_not_supported =
				matches!(
					spec,
					ForkSpec::EIP150
						| ForkSpec::EIP158 | ForkSpec::Frontier
						| ForkSpec::Homestead | ForkSpec::Byzantium
						| ForkSpec::Constantinople
						| ForkSpec::ConstantinopleFix
						| ForkSpec::Istanbul
				) && TxType::from_txbytes(&state.txbytes) != TxType::Legacy
					&& state.expect_exception.as_deref() == Some("TR_TypeNotSupported");
			if expect_tx_type_not_supported {
				continue;
			}

			tests_result.total += 1;

			// Only execute valid transactions
			if let Ok(transaction) = crate::utils::transaction::validate(
				transaction,
				test.0.env.gas_limit.0,
				caller_balance,
				&gasometer_config,
			) {
				let gas_limit: u64 = transaction.gas_limit.into();
				let data: Vec<u8> = transaction.data.into();
				let metadata =
					StackSubstateMetadata::new(transaction.gas_limit.into(), &gasometer_config);
				let executor_state = MemoryStackState::new(metadata, &backend);
				let precompile = JsonPrecompile::precompile(spec).unwrap();
				let mut executor = StackExecutor::new_with_precompiles(
					executor_state,
					&gasometer_config,
					&precompile,
				);
				let total_fee = vicinity.gas_price * gas_limit;

				executor.state_mut().withdraw(caller, total_fee).unwrap();

				let access_list = transaction
					.access_list
					.into_iter()
					.map(|(address, keys)| (address.0, keys.into_iter().map(|k| k.0).collect()))
					.collect();

				match transaction.to {
					ethjson::maybe::MaybeEmpty::Some(to) => {
						let value = transaction.value.into();

						let _reason = executor.transact_call(
							caller,
							to.into(),
							value,
							data,
							gas_limit,
							access_list,
						);
					}
					ethjson::maybe::MaybeEmpty::None => {
						let code = data;
						let value = transaction.value.into();

						let _reason =
							executor.transact_create(caller, value, code, gas_limit, access_list);
					}
				}

				let actual_fee = executor.fee(vicinity.gas_price);
				// Forks after London burn miner rewards and thus have different gas fee
				// calculation (see EIP-1559)
				let miner_reward = if spec.is_eth2() {
					let max_priority_fee_per_gas = test.0.transaction.max_priority_fee_per_gas();
					let max_fee_per_gas = test.0.transaction.max_fee_per_gas();
					let base_fee_per_gas = vicinity.block_base_fee_per_gas;
					let priority_fee_per_gas =
						std::cmp::min(max_priority_fee_per_gas, max_fee_per_gas - base_fee_per_gas);
					executor.fee(priority_fee_per_gas)
				} else {
					actual_fee
				};

				executor
					.state_mut()
					.deposit(vicinity.block_coinbase, miner_reward);
				executor.state_mut().deposit(caller, total_fee - actual_fee);

				let (values, logs) = executor.into_state().deconstruct();

				backend.apply(values, logs, delete_empty);
			}
			let (is_valid_hash, actual_hash) = assert_valid_hash(&state.hash.0, backend.state());
			if !is_valid_hash {
				tests_result.failed_tests.push(FailedTestDetails {
					expected_hash: state.hash.0,
					actual_hash,
					index: i,
					name: String::from_str(name).unwrap(),
					spec: spec.clone(),
				});
				tests_result.failed += 1;
			} else if print_output {
				println!("passed");
			}
		}
	}
	tests_result
}

/// Denotes the type of transaction.
#[derive(Debug, PartialEq)]
enum TxType {
	/// All transactions before EIP-2718 are legacy.
	Legacy,
	/// https://eips.ethereum.org/EIPS/eip-2718
	AccessList,
	/// https://eips.ethereum.org/EIPS/eip-1559
	DynamicFee,
	/// https://eips.ethereum.org/EIPS/eip-4844
	ShardBlob,
}

impl TxType {
	/// Whether this is a legacy, access list, dynamic fee, etc transaction
	// Taken from geth's core/types/transaction.go/UnmarshalBinary, but we only detect the transaction
	// type rather than unmarshal the entire payload.
	const fn from_txbytes(txbytes: &[u8]) -> Self {
		match txbytes[0] {
			b if b > 0x7f => Self::Legacy,
			1 => Self::AccessList,
			2 => Self::DynamicFee,
			3 => Self::ShardBlob,
			_ => panic!(
				"Unknown tx type. \
You may need to update the TxType enum if Ethereum introduced new enveloped transaction types."
			),
		}
	}
}
