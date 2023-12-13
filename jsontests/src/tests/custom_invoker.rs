use crate::error::{Error, TestError};
use crate::in_memory::{InMemoryAccount, InMemoryBackend, InMemoryEnvironment, InMemoryLayer};
use crate::types::{Fork, TestCompletionStatus, TestData, TestExpectException, TestMulti};
use evm::interpreter::Interpreter;
use evm::standard::{
	routines, Config, Etable, EtableResolver, InvokerState, Resolver, SubstackInvoke, TransactArgs,
	TransactInvoke,
};
use evm::trap::{CallCreateTrap, CallCreateTrapData, CallTrapData, CreateScheme};
use evm::utils::u256_to_h256;
use evm::GasState;
use evm::Invoker as InvokerT;
use evm::{
	Capture, Context, ExitError, ExitException, ExitResult, ExitSucceed, InvokerControl,
	MergeStrategy, RuntimeBackend, RuntimeEnvironment, RuntimeState, TransactionContext,
	TransactionalBackend, Transfer, TrapConsume,
};
use evm_precompile::StandardPrecompileSet;
use primitive_types::{H160, U256};
use std::cmp::min;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::rc::Rc;

pub struct Invoker<'config, 'resolver, R> {
	config: &'config Config,
	resolver: &'resolver R,
}

impl<'config, 'resolver, R> Invoker<'config, 'resolver, R> {
	pub fn new(config: &'config Config, resolver: &'resolver R) -> Self {
		Self { config, resolver }
	}
}

impl<'config, 'resolver, H, R, Tr> InvokerT<H, Tr> for Invoker<'config, 'resolver, R>
where
	R::State: InvokerState<'config> + AsRef<RuntimeState> + AsMut<RuntimeState>,
	H: RuntimeEnvironment + RuntimeBackend + TransactionalBackend,
	R: Resolver<H>,
	Tr: TrapConsume<CallCreateTrap>,
{
	type State = R::State;
	type Interpreter = R::Interpreter;
	type Interrupt = Tr::Rest;
	type TransactArgs = TransactArgs;
	type TransactInvoke = TransactInvoke;
	type TransactValue = (ExitSucceed, Option<H160>);
	type SubstackInvoke = SubstackInvoke;

	fn new_transact(
		&self,
		args: Self::TransactArgs,
		handler: &mut H,
	) -> Result<
		(
			Self::TransactInvoke,
			InvokerControl<Self::Interpreter, (ExitResult, (R::State, Vec<u8>))>,
		),
		ExitError,
	> {
		let caller = args.caller();
		let gas_price = args.gas_price();

		let gas_fee = args.gas_limit().saturating_mul(gas_price);
		handler.withdrawal(caller, gas_fee)?;

		handler.inc_nonce(caller)?;

		let address = match &args {
			TransactArgs::Call { address, .. } => *address,
			TransactArgs::Create { caller, salt, .. } => match salt {
				Some(_) => {
					// `create_fixed` to mirroring ERC20 addresses
					caller.clone()
				}
				None => {
					let scheme = CreateScheme::Legacy { caller: *caller };
					scheme.address(handler)
				}
			},
		};
		let value = args.value();

		let invoke = TransactInvoke {
			gas_fee,
			gas_price: args.gas_price(),
			caller: args.caller(),
			create_address: match &args {
				TransactArgs::Call { .. } => None,
				TransactArgs::Create { .. } => Some(address),
			},
		};

		handler.push_substate();

		let context = Context {
			caller,
			address,
			apparent_value: value,
		};
		let transaction_context = TransactionContext {
			origin: caller,
			gas_price,
		};
		let transfer = Transfer {
			source: caller,
			target: address,
			value,
		};
		let runtime_state = RuntimeState {
			context,
			transaction_context: Rc::new(transaction_context),
			retbuf: Vec::new(),
		};

		let work = || -> Result<(TransactInvoke, _), ExitError> {
			match args {
				TransactArgs::Call {
					caller,
					address,
					data,
					gas_limit,
					access_list,
					..
				} => {
					for (address, keys) in &access_list {
						handler.mark_hot(*address, None);
						for key in keys {
							handler.mark_hot(*address, Some(*key));
						}
					}

					let state = <R::State>::new_transact_call(
						runtime_state,
						gas_limit,
						&data,
						&access_list,
						self.config,
					)?;

					let machine = routines::make_enter_call_machine(
						self.config,
						self.resolver,
						address,
						data,
						Some(transfer),
						state,
						handler,
					)?;

					if self.config.increase_state_access_gas {
						if self.config.warm_coinbase_address {
							let coinbase = handler.block_coinbase();
							handler.mark_hot(coinbase, None);
						}
						handler.mark_hot(caller, None);
						handler.mark_hot(address, None);
					}

					Ok((invoke, machine))
				}
				TransactArgs::Create {
					caller,
					init_code,
					gas_limit,
					access_list,
					..
				} => {
					let state = <R::State>::new_transact_create(
						runtime_state,
						gas_limit,
						&init_code,
						&access_list,
						self.config,
					)?;

					let machine = routines::make_enter_create_machine(
						self.config,
						self.resolver,
						caller,
						init_code,
						transfer,
						state,
						handler,
					)?;

					Ok((invoke, machine))
				}
			}
		};

		work().map_err(|err| {
			handler.pop_substate(MergeStrategy::Discard);
			err
		})
	}

	fn finalize_transact(
		&self,
		invoke: &Self::TransactInvoke,
		result: ExitResult,
		(mut substate, retval): (R::State, Vec<u8>),
		handler: &mut H,
	) -> Result<Self::TransactValue, ExitError> {
		let left_gas = substate.effective_gas();
		let work = || -> Result<Self::TransactValue, ExitError> {
			if result.is_ok() {
				if let Some(address) = invoke.create_address {
					let retbuf = retval;
					routines::deploy_create_code(
						self.config,
						address,
						retbuf,
						&mut substate,
						handler,
					)?;
				}
			}

			result.map(|s| (s, invoke.create_address))
		};
		let result = work();
		let refunded_gas = match result {
			Ok(_) | Err(ExitError::Reverted) => left_gas,
			Err(_) => U256::zero(),
		};
		let refunded_fee = refunded_gas.saturating_mul(invoke.gas_price);
		let coinbase_reward = invoke.gas_fee.saturating_sub(refunded_fee);
		match &result {
			Ok(_) => {
				handler.pop_substate(MergeStrategy::Commit);
			}
			Err(_) => {
				handler.pop_substate(MergeStrategy::Discard);
			}
		}
		handler.deposit(invoke.caller, refunded_fee);
		handler.deposit(handler.block_coinbase(), coinbase_reward);
		result
	}

	fn enter_substack(
		&self,
		trap: Tr,
		machine: &mut Self::Interpreter,
		handler: &mut H,
		depth: usize,
	) -> Capture<
		Result<
			(
				Self::SubstackInvoke,
				InvokerControl<Self::Interpreter, (ExitResult, (R::State, Vec<u8>))>,
			),
			ExitError,
		>,
		Self::Interrupt,
	> {
		fn l64(gas: U256) -> U256 {
			gas - gas / U256::from(64)
		}

		let opcode = match trap.consume() {
			Ok(opcode) => opcode,
			Err(interrupt) => return Capture::Trap(interrupt),
		};
		if depth >= self.config.call_stack_limit {
			return Capture::Exit(Err(ExitException::CallTooDeep.into()));
		}
		let trap_data = match CallCreateTrapData::new_from(opcode, machine.machine_mut()) {
			Ok(trap_data) => trap_data,
			Err(err) => return Capture::Exit(Err(err)),
		};
		let after_gas = if self.config.call_l64_after_gas {
			l64(machine.machine().state.gas())
		} else {
			machine.machine().state.gas()
		};
		let target_gas = trap_data.target_gas().unwrap_or(after_gas);
		let gas_limit = min(after_gas, target_gas);
		let call_has_value =
			matches!(&trap_data, CallCreateTrapData::Call(call) if call.has_value());

		let is_static = if machine.machine().state.is_static() {
			true
		} else {
			match &trap_data {
				CallCreateTrapData::Call(CallTrapData { is_static, .. }) => *is_static,
				_ => false,
			}
		};
		let transaction_context = machine.machine().state.as_ref().transaction_context.clone();
		match trap_data {
			CallCreateTrapData::Call(call_trap_data) => {
				let substate = match machine.machine_mut().state.substate(
					RuntimeState {
						context: call_trap_data.context.clone(),
						transaction_context,
						retbuf: Vec::new(),
					},
					gas_limit,
					is_static,
					call_has_value,
				) {
					Ok(submeter) => submeter,
					Err(err) => return Capture::Exit(Err(err)),
				};

				let target = call_trap_data.target;

				Capture::Exit(routines::enter_call_substack(
					self.config,
					self.resolver,
					call_trap_data,
					target,
					substate,
					handler,
				))
			}
			CallCreateTrapData::Create(create_trap_data) => {
				let caller = create_trap_data.scheme.caller();
				let address = create_trap_data.scheme.address(handler);
				let code = create_trap_data.code.clone();

				let substate = match machine.machine_mut().state.substate(
					RuntimeState {
						context: Context {
							address,
							caller,
							apparent_value: create_trap_data.value,
						},
						transaction_context,
						retbuf: Vec::new(),
					},
					gas_limit,
					is_static,
					call_has_value,
				) {
					Ok(submeter) => submeter,
					Err(err) => return Capture::Exit(Err(err)),
				};
				Capture::Exit(routines::enter_create_substack(
					self.config,
					self.resolver,
					code,
					create_trap_data,
					substate,
					handler,
				))
			}
		}
	}

	fn exit_substack(
		&self,
		result: ExitResult,
		(mut substate, retval): (R::State, Vec<u8>),
		trap_data: Self::SubstackInvoke,
		parent: &mut Self::Interpreter,
		handler: &mut H,
	) -> Result<(), ExitError> {
		let strategy = match &result {
			Ok(_) => MergeStrategy::Commit,
			Err(ExitError::Reverted) => MergeStrategy::Revert,
			Err(_) => MergeStrategy::Discard,
		};
		match trap_data {
			SubstackInvoke::Create { address, trap } => {
				let retbuf = retval;
				let result = result.and_then(|_| {
					routines::deploy_create_code(
						self.config,
						address,
						retbuf.clone(),
						&mut substate,
						handler,
					)?;
					Ok(address)
				});
				parent.machine_mut().state.merge(substate, strategy);
				handler.pop_substate(strategy);
				trap.feedback(result, retbuf, parent)?;
				Ok(())
			}
			SubstackInvoke::Call { trap } => {
				let retbuf = retval;
				parent.machine_mut().state.merge(substate, strategy);
				handler.pop_substate(strategy);
				trap.feedback(result, retbuf, parent)?;
				Ok(())
			}
		}
	}
}

/// Run single test
pub fn run_test(test: TestData, debug: bool) -> Result<(), Error> {
	let config = match test.fork {
		Fork::Berlin => Config::berlin(),
		_ => return Err(Error::UnsupportedFork),
	};

	if test.post.expect_exception == Some(TestExpectException::TR_TypeNotSupported) {
		return Ok(());
	}

	let env = InMemoryEnvironment {
		block_hashes: BTreeMap::new(),
		block_number: test.env.current_number,
		block_coinbase: test.env.current_coinbase,
		block_timestamp: test.env.current_timestamp,
		block_difficulty: test.env.current_difficulty,
		block_randomness: Some(test.env.current_random),
		block_gas_limit: test.env.current_gas_limit,
		block_base_fee_per_gas: U256::zero(),
		chain_id: U256::zero(),
	};

	let state = test
		.pre
		.clone()
		.into_iter()
		.map(|(address, account)| {
			let storage = account
				.storage
				.into_iter()
				.filter(|(_, value)| *value != U256::zero())
				.map(|(key, value)| (u256_to_h256(key), u256_to_h256(value)))
				.collect::<BTreeMap<_, _>>();

			(
				address,
				InMemoryAccount {
					balance: account.balance,
					code: account.code.0,
					nonce: account.nonce,
					original_storage: storage.clone(),
					storage,
				},
			)
		})
		.collect::<BTreeMap<_, _>>();

	let gas_etable = Etable::single(evm::standard::eval_gasometer);
	let exec_etable = Etable::runtime();
	let etable = (gas_etable, exec_etable);
	let precompiles = StandardPrecompileSet::new(&config);
	let resolver = EtableResolver::new(&config, &precompiles, &etable);
	let invoker = Invoker::new(&config, &resolver);
	let args = TransactArgs::Call {
		caller: test.transaction.sender,
		address: test.transaction.to,
		value: test.transaction.value,
		data: test.transaction.data,
		gas_limit: test.transaction.gas_limit,
		gas_price: test.transaction.gas_price,
		access_list: test
			.transaction
			.access_list
			.into_iter()
			.map(|access| (access.address, access.storage_keys))
			.collect(),
	};

	let mut run_backend = InMemoryBackend {
		environment: env,
		layers: vec![InMemoryLayer {
			state,
			logs: Vec::new(),
			suicides: Vec::new(),
			hots: {
				let mut hots = BTreeSet::new();
				for i in 1..10 {
					hots.insert((u256_to_h256(U256::from(i)).into(), None));
				}
				hots
			},
		}],
	};
	let mut step_backend = run_backend.clone();

	let run_result = evm::transact(args.clone(), Some(4), &mut run_backend, &invoker);
	run_backend.layers[0].clear_pending();

	if debug {
		let _step_result = evm::HeapTransact::new(args, &invoker, &mut step_backend).and_then(
			|mut stepper| loop {
				{
					if let Some(machine) = stepper.last_interpreter() {
						println!(
							"pc: {}, opcode: {:?}, gas: 0x{:x}",
							machine.position(),
							machine.peek_opcode(),
							machine.machine().state.gas(),
						);
					}
				}
				if let Err(Capture::Exit(result)) = stepper.step() {
					break result;
				}
			},
		);
		step_backend.layers[0].clear_pending();
	}

	let state_root = crate::hash::state_root(&run_backend);

	if test.post.expect_exception.is_some() {
		if run_result.is_err() {
			return Ok(());
		} else {
			return Err(TestError::ExpectException.into());
		}
	}

	if state_root != test.post.hash {
		if debug {
			for (address, account) in &run_backend.layers[0].state {
				println!(
					"address: {:?}, balance: {}, nonce: {}, code: 0x{}, storage: {:?}",
					address,
					account.balance,
					account.nonce,
					hex::encode(&account.code),
					account.storage
				);
			}
		}

		return Err(TestError::StateMismatch.into());
	}

	Ok(())
}

fn run_file(filename: &str, debug: bool) -> Result<TestCompletionStatus, Error> {
	let test_multi: BTreeMap<String, TestMulti> =
		serde_json::from_reader(BufReader::new(File::open(filename)?))?;
	let mut tests_status = TestCompletionStatus::default();

	for (test_name, test_multi) in test_multi {
		let tests = test_multi.tests();
		let short_file_name = get_short_file_name(filename);
		for test in &tests {
			if debug {
				print!(
					"[{:?}] {} | {}/{} DEBUG: ",
					test.fork, short_file_name, test_name, test.index
				);
			} else {
				print!(
					"[{:?}] {} | {}/{}: ",
					test.fork, short_file_name, test_name, test.index
				);
			}
			match run_test(test.clone(), debug) {
				Ok(()) => {
					tests_status.inc_completed();
					println!("ok")
				}
				Err(Error::UnsupportedFork) => {
					tests_status.inc_skipped();
					println!("skipped")
				}
				Err(err) => {
					println!("ERROR: {:?}", err);
					return Err(err);
				}
			}
			if debug {
				println!();
			}
		}

		tests_status.print_completion();
	}

	Ok(tests_status)
}

/// Run test for single json file or directory
pub fn run_single(filename: &str, debug: bool) -> Result<TestCompletionStatus, Error> {
	if fs::metadata(filename)?.is_dir() {
		let mut tests_status = TestCompletionStatus::default();

		for filename in fs::read_dir(filename)? {
			let filepath = filename?.path();
			let filename = filepath.to_str().ok_or(Error::NonUtf8Filename)?;
			println!("RUM for: {filename}");
			tests_status += run_file(filename, debug)?;
		}
		tests_status.print_total_for_dir(filename);
		Ok(tests_status)
	} else {
		run_file(filename, debug)
	}
}

const BASIC_FILE_PATH_TO_TRIM: [&str; 2] = [
	"jsontests/res/ethtests/GeneralStateTests/",
	"res/ethtests/GeneralStateTests/",
];

fn get_short_file_name(filename: &str) -> String {
	let mut short_file_name = String::from(filename);
	for pattern in BASIC_FILE_PATH_TO_TRIM {
		short_file_name = short_file_name.replace(pattern, "");
	}
	short_file_name.clone().to_string()
}

#[test]
fn custom_st_args_zero_one_balance() {
	const JSON_FILENAME: &str = "res/ethtests/GeneralStateTests/stArgsZeroOneBalance/";
	let tests_status = run_single(JSON_FILENAME, false).unwrap();
	tests_status.print_total();
}
