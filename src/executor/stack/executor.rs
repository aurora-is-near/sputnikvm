use crate::backend::Backend;
use crate::executor::stack::precompile::{
	PrecompileFailure, PrecompileHandle, PrecompileOutput, PrecompileSet,
};
use crate::executor::stack::tagged_runtime::{RuntimeKind, TaggedRuntime};
use crate::gasometer::{self, Gasometer, StorageTarget};
use crate::maybe_borrowed::MaybeBorrowed;
use crate::prelude::*;
use crate::{
	Capture, Config, Context, CreateScheme, ExitError, ExitReason, Handler, Opcode, Runtime,
	Transfer,
};
use core::{cmp::min, convert::Infallible};
use evm_core::utils::U64_MAX;
use evm_core::{ExitFatal, InterpreterHandler, Machine, Trap};
use evm_runtime::Resolve;
use primitive_types::{H160, H256, U256};
use sha3::{Digest, Keccak256};
use smallvec::{smallvec, SmallVec};

macro_rules! emit_exit {
	($reason:expr) => {{
		let reason = $reason;
		event!(Exit {
			reason: &reason,
			return_value: &Vec::new(),
		});
		reason
	}};
	($reason:expr, $return_value:expr) => {{
		let reason = $reason;
		let return_value = $return_value;
		event!(Exit {
			reason: &reason,
			return_value: &return_value,
		});
		(reason, return_value)
	}};
}
macro_rules! try_or_fail {
	( $e:expr ) => {
		match $e {
			Ok(v) => v,
			Err(e) => return Capture::Exit((e.into(), Vec::new())),
		}
	};
}

const DEFAULT_CALL_STACK_CAPACITY: usize = 4;
// For EIP-7702 Hash of `EF01` bytes that is used for `EXTCODEHASH` when called from delegated address.
const EIP7702_MAGIC_HASH: [u8; 32] = [
	0xEA, 0xDC, 0xDB, 0xA6, 0x6A, 0x79, 0xAB, 0x5D, 0xCE, 0x91, 0x62, 0x2D, 0x1D, 0x75, 0xC8, 0xCF,
	0xF5, 0xCF, 0xF0, 0xB9, 0x69, 0x44, 0xC3, 0xBF, 0x10, 0x72, 0xCD, 0x08, 0xCE, 0x01, 0x83, 0x29,
];

const fn l64(gas: u64) -> u64 {
	gas - gas / 64
}

pub enum StackExitKind {
	Succeeded,
	Reverted,
	Failed,
}

/// `Authorization` contains already prepared data for EIP-7702.
/// - `authority`is `ecrecovered` authority address.
/// - `address` is delegation destination address.
/// - `nonce` is the `nonce` value which `authority.nonce` should be equal.
/// - `is_valid` is the flag that indicates the validity of the authorization. It is used to
///   charge gas for each authorization item, but if it's invalid exclude from EVM `authority_list` flow.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Authorization {
	pub authority: H160,
	pub address: H160,
	pub nonce: u64,
	pub is_valid: bool,
}

impl Authorization {
	/// Create a new `Authorization` with given `authority`, `address`, and `nonce`.
	#[must_use]
	pub const fn new(authority: H160, address: H160, nonce: u64, is_valid: bool) -> Self {
		Self {
			authority,
			address,
			nonce,
			is_valid,
		}
	}

	/// Returns `true` if `authority` is delegated to `address`.
	/// `0xef0100 ++ address`, and it is always 23 bytes.
	#[must_use]
	pub fn is_delegated(code: &[u8]) -> bool {
		code.len() == 23 && code.starts_with(&[0xEF, 0x01, 0x00])
	}

	/// Get `authority` delegated `address`.
	/// It checks, is it delegation designation (EIP-7702).
	#[must_use]
	pub fn get_delegated_address(code: &[u8]) -> Option<H160> {
		if Self::is_delegated(code) {
			// `code` size is always 23 bytes.
			Some(H160::from_slice(&code[3..]))
		} else {
			None
		}
	}

	/// Returns the delegation code as composing: `0xef0100 ++ address`.
	/// Result code is always 23 bytes.
	#[must_use]
	pub fn delegation_code(&self) -> Vec<u8> {
		let mut code = Vec::with_capacity(23);
		code.extend(&[0xEF, 0x01, 0x00]);
		code.extend(self.address.as_bytes());
		code
	}
}

#[derive(Default, Clone, Debug)]
pub struct Accessed {
	pub accessed_addresses: BTreeSet<H160>,
	pub accessed_storage: BTreeSet<(H160, H256)>,
	pub authority: BTreeMap<H160, H160>,
}

impl Accessed {
	pub fn access_address(&mut self, address: H160) {
		self.accessed_addresses.insert(address);
	}

	pub fn access_addresses<I>(&mut self, addresses: I)
	where
		I: Iterator<Item = H160>,
	{
		self.accessed_addresses.extend(addresses);
	}

	pub fn access_storages<I>(&mut self, storages: I)
	where
		I: Iterator<Item = (H160, H256)>,
	{
		for storage in storages {
			self.accessed_storage.insert((storage.0, storage.1));
		}
	}

	/// Add authority to the accessed authority list (EIP-7702).
	pub fn add_authority(&mut self, authority: H160, address: H160) {
		self.authority.insert(authority, address);
	}

	/// Get authority from the accessed authority list (EIP-7702).
	#[must_use]
	pub fn get_authority_target(&self, authority: H160) -> Option<H160> {
		self.authority.get(&authority).copied()
	}

	/// Check if authority is in the accessed authority list (EIP-7702).
	#[must_use]
	pub fn is_authority(&self, authority: H160) -> bool {
		self.authority.contains_key(&authority)
	}
}

#[derive(Clone, Debug)]
pub struct StackSubstateMetadata<'config> {
	gasometer: Gasometer<'config>,
	is_static: bool,
	depth: Option<usize>,
	accessed: Option<Accessed>,
}

impl<'config> StackSubstateMetadata<'config> {
	#[must_use]
	pub fn new(gas_limit: u64, config: &'config Config) -> Self {
		let accessed = if config.increase_state_access_gas {
			Some(Accessed::default())
		} else {
			None
		};
		Self {
			gasometer: Gasometer::new(gas_limit, config),
			is_static: false,
			depth: None,
			accessed,
		}
	}

	/// Swallow commit implements part of logic for `exit_commit`:
	/// - Record opcode stipend.
	/// - Record an explicit refund.
	/// - Merge warmed accounts and storages
	///
	/// # Errors
	/// Return `ExitError` that is thrown by gasometer gas calculation errors.
	pub fn swallow_commit(&mut self, other: Self) -> Result<(), ExitError> {
		self.gasometer.record_stipend(other.gasometer.gas())?;
		self.gasometer
			.record_refund(other.gasometer.refunded_gas())?;

		// Merge warmed accounts and storages
		if let (Some(mut other_accessed), Some(self_accessed)) =
			(other.accessed, self.accessed.as_mut())
		{
			self_accessed
				.accessed_addresses
				.append(&mut other_accessed.accessed_addresses);
			self_accessed
				.accessed_storage
				.append(&mut other_accessed.accessed_storage);
			self_accessed
				.authority
				.append(&mut other_accessed.authority);
		}

		Ok(())
	}

	/// Swallow revert implements part of logic for `exit_commit`:
	/// - Record opcode stipend.
	///
	/// # Errors
	/// Return `ExitError` that is thrown by gasometer gas calculation errors.
	pub fn swallow_revert(&mut self, other: &Self) -> Result<(), ExitError> {
		self.gasometer.record_stipend(other.gasometer.gas())
	}

	/// Swallow revert implements part of logic for `exit_commit`:
	/// At the moment, it does nothing.
	pub const fn swallow_discard(&self, _other: &Self) {}

	#[must_use]
	pub fn spit_child(&self, gas_limit: u64, is_static: bool) -> Self {
		Self {
			gasometer: Gasometer::new(gas_limit, self.gasometer.config()),
			is_static: is_static || self.is_static,
			depth: self.depth.map_or(Some(0), |n| Some(n + 1)),
			accessed: self.accessed.as_ref().map(|_| Accessed::default()),
		}
	}

	#[must_use]
	pub const fn gasometer(&self) -> &Gasometer<'config> {
		&self.gasometer
	}

	pub fn gasometer_mut(&mut self) -> &mut Gasometer<'config> {
		&mut self.gasometer
	}

	#[must_use]
	pub const fn is_static(&self) -> bool {
		self.is_static
	}

	#[must_use]
	pub const fn depth(&self) -> Option<usize> {
		self.depth
	}

	pub fn access_address(&mut self, address: H160) {
		if let Some(accessed) = &mut self.accessed {
			accessed.access_address(address);
		}
	}

	pub fn access_addresses<I>(&mut self, addresses: I)
	where
		I: Iterator<Item = H160>,
	{
		if let Some(accessed) = &mut self.accessed {
			accessed.access_addresses(addresses);
		}
	}

	pub fn access_storage(&mut self, address: H160, key: H256) {
		if let Some(accessed) = &mut self.accessed {
			accessed.accessed_storage.insert((address, key));
		}
	}

	pub fn access_storages<I>(&mut self, storages: I)
	where
		I: Iterator<Item = (H160, H256)>,
	{
		if let Some(accessed) = &mut self.accessed {
			accessed.access_storages(storages);
		}
	}

	/// Used for gas calculation logic.
	/// It's most significant for `cold/warm` gas calculation as warmed addresses spent less gas.
	#[must_use]
	pub const fn accessed(&self) -> &Option<Accessed> {
		&self.accessed
	}

	/// Add authority to accessed list (related to EIP-7702)
	pub fn add_authority(&mut self, authority: H160, address: H160) {
		if let Some(accessed) = &mut self.accessed {
			accessed.add_authority(authority, address);
		}
	}
}

#[auto_impl::auto_impl(& mut, Box)]
pub trait StackState<'config>: Backend {
	fn metadata(&self) -> &StackSubstateMetadata<'config>;
	fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config>;

	fn enter(&mut self, gas_limit: u64, is_static: bool);
	/// # Errors
	/// Return `ExitError`
	fn exit_commit(&mut self) -> Result<(), ExitError>;
	/// # Errors
	/// Return `ExitError`
	fn exit_revert(&mut self) -> Result<(), ExitError>;
	/// # Errors
	/// Return `ExitError`
	fn exit_discard(&mut self) -> Result<(), ExitError>;

	fn is_empty(&self, address: H160) -> bool;
	fn deleted(&self, address: H160) -> bool;
	fn is_created(&self, address: H160) -> bool;
	fn is_cold(&self, address: H160) -> bool;
	fn is_storage_cold(&self, address: H160, key: H256) -> bool;

	/// # Errors
	/// Return `ExitError`
	fn inc_nonce(&mut self, address: H160) -> Result<(), ExitError>;
	fn set_storage(&mut self, address: H160, key: H256, value: H256);
	fn reset_storage(&mut self, address: H160);
	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>);
	fn set_deleted(&mut self, address: H160);
	fn set_created(&mut self, address: H160);
	fn set_code(&mut self, address: H160, code: Vec<u8>);
	/// # Errors
	/// Return `ExitError`
	fn transfer(&mut self, transfer: Transfer) -> Result<(), ExitError>;
	fn reset_balance(&mut self, address: H160);
	fn touch(&mut self, address: H160);

	/// # Errors
	/// Return `ExitError`
	fn record_external_operation(
		&mut self,
		#[allow(clippy::used_underscore_binding)] _op: crate::ExternalOperation,
	) -> Result<(), ExitError> {
		Ok(())
	}

	/// # Errors
	/// Return `ExitError`
	fn record_external_dynamic_opcode_cost(
		&mut self,
		#[allow(clippy::used_underscore_binding)] _opcode: Opcode,
		#[allow(clippy::used_underscore_binding)] _gas_cost: gasometer::GasCost,
		#[allow(clippy::used_underscore_binding)] _target: StorageTarget,
	) -> Result<(), ExitError> {
		Ok(())
	}

	/// # Errors
	/// Return `ExitError`
	fn record_external_cost(
		&mut self,
		#[allow(clippy::used_underscore_binding)] _ref_time: Option<u64>,
		#[allow(clippy::used_underscore_binding)] _proof_size: Option<u64>,
		#[allow(clippy::used_underscore_binding)] _storage_growth: Option<u64>,
	) -> Result<(), ExitError> {
		Ok(())
	}

	fn refund_external_cost(
		&mut self,
		#[allow(clippy::used_underscore_binding)] _ref_time: Option<u64>,
		#[allow(clippy::used_underscore_binding)] _proof_size: Option<u64>,
	) {
	}

	/// Set tstorage value of address at index.
	/// EIP-1153: Transient storage
	///
	/// # Errors
	/// Return `ExitError`
	fn tstore(&mut self, address: H160, index: H256, value: U256) -> Result<(), ExitError>;
	/// Get tstorage value of address at index.
	/// EIP-1153: Transient storage
	///
	/// # Errors
	/// Return `ExitError`
	fn tload(&mut self, address: H160, index: H256) -> Result<U256, ExitError>;

	/// EIP-7702 - check is authority cold.
	fn is_authority_cold(&mut self, address: H160) -> Option<bool>;

	/// EIP-7702 - get authority target address.
	fn get_authority_target(&mut self, address: H160) -> Option<H160>;
}

/// Stack-based executor.
pub struct StackExecutor<'config, 'precompiles, S, P> {
	config: &'config Config,
	state: S,
	precompile_set: &'precompiles P,
}

impl<'config, 'precompiles, S: StackState<'config>, P: PrecompileSet>
	StackExecutor<'config, 'precompiles, S, P>
{
	/// Return a reference of the Config.
	pub const fn config(&self) -> &'config Config {
		self.config
	}

	/// Return a reference to the precompile set.
	pub const fn precompiles(&self) -> &'precompiles P {
		self.precompile_set
	}

	/// Create a new stack-based executor with given precompiles.
	pub const fn new_with_precompiles(
		state: S,
		config: &'config Config,
		precompile_set: &'precompiles P,
	) -> Self {
		Self {
			config,
			state,
			precompile_set,
		}
	}

	pub const fn state(&self) -> &S {
		&self.state
	}

	pub fn state_mut(&mut self) -> &mut S {
		&mut self.state
	}

	#[allow(clippy::missing_const_for_fn)]
	pub fn into_state(self) -> S {
		self.state
	}

	/// Create a substate executor from the current executor.
	pub fn enter_substate(&mut self, gas_limit: u64, is_static: bool) {
		self.state.enter(gas_limit, is_static);
	}

	/// Exit a substate.
	///
	/// # Panics
	/// Panic occurs if a result is an empty `substate` stack.
	///
	/// # Errors
	/// Return `ExitError`
	pub fn exit_substate(&mut self, kind: &StackExitKind) -> Result<(), ExitError> {
		match kind {
			StackExitKind::Succeeded => self.state.exit_commit(),
			StackExitKind::Reverted => self.state.exit_revert(),
			StackExitKind::Failed => self.state.exit_discard(),
		}
	}

	/// Execute the runtime until it returns.
	pub fn execute(&mut self, runtime: &mut Runtime) -> ExitReason {
		let mut call_stack: SmallVec<[TaggedRuntime; DEFAULT_CALL_STACK_CAPACITY]> =
			smallvec!(TaggedRuntime {
				kind: RuntimeKind::Execute,
				inner: MaybeBorrowed::Borrowed(runtime),
			});
		let (reason, _, _) = self.execute_with_call_stack(&mut call_stack);
		reason
	}

	/// Execute using Runtimes on the `call_stack` until it returns.
	fn execute_with_call_stack(
		&mut self,
		call_stack: &mut SmallVec<[TaggedRuntime<'_>; DEFAULT_CALL_STACK_CAPACITY]>,
	) -> (ExitReason, Option<H160>, Vec<u8>) {
		// This `interrupt_runtime` is used to pass the runtime obtained from the
		// `Capture::Trap` branch in the match below back to the top of the call stack.
		// The reason we can't simply `push` the runtime directly onto the stack in the
		// `Capture::Trap` branch is because the borrow-checker complains that the stack
		// is already borrowed as long as we hold a pointer on the last element
		// (i.e. the currently executing runtime).
		let mut interrupt_runtime = None;
		loop {
			if let Some(rt) = interrupt_runtime.take() {
				call_stack.push(rt);
			}
			let Some(runtime) = call_stack.last_mut() else {
				return (
					ExitReason::Fatal(ExitFatal::UnhandledInterrupt),
					None,
					Vec::new(),
				);
			};
			let reason = {
				let inner_runtime = &mut runtime.inner;
				match inner_runtime.run(self) {
					Capture::Exit(reason) => reason,
					Capture::Trap(Resolve::Call(rt, _)) => {
						interrupt_runtime = Some(rt.0);
						continue;
					}
					Capture::Trap(Resolve::Create(rt, _)) => {
						interrupt_runtime = Some(rt.0);
						continue;
					}
				}
			};
			let runtime_kind = runtime.kind;
			let (reason, maybe_address, return_data) = match runtime_kind {
				RuntimeKind::Create(created_address) => {
					let (reason, maybe_address, return_data) = self.cleanup_for_create(
						created_address,
						reason,
						runtime.inner.machine().return_value(),
					);
					(reason, maybe_address, return_data)
				}
				RuntimeKind::Call(code_address) => {
					let return_data = self.cleanup_for_call(
						code_address,
						&reason,
						runtime.inner.machine().return_value(),
					);
					(reason, None, return_data)
				}
				RuntimeKind::Execute => (reason, None, runtime.inner.machine().return_value()),
			};
			// We're done with that runtime now, so can pop it off the call stack
			call_stack.pop();
			// Now pass the results from that runtime on to the next one in the stack
			let Some(runtime) = call_stack.last_mut() else {
				return (reason, None, return_data);
			};
			emit_exit!(&reason, &return_data);
			let inner_runtime = &mut runtime.inner;
			let maybe_error = match runtime_kind {
				RuntimeKind::Create(_) => {
					inner_runtime.finish_create(reason, maybe_address, return_data)
				}
				RuntimeKind::Call(_) | RuntimeKind::Execute => {
					inner_runtime.finish_call(reason, return_data)
				}
			};
			// Early exit if passing on the result caused an error
			if let Err(e) = maybe_error {
				return (e, None, Vec::new());
			}
		}
	}

	/// Get remaining gas.
	pub fn gas(&self) -> u64 {
		self.state.metadata().gasometer.gas()
	}

	fn record_create_transaction_cost(
		&mut self,
		init_code: &[u8],
		access_list: &[(H160, Vec<H256>)],
	) -> Result<(), ExitError> {
		let transaction_cost = gasometer::create_transaction_cost(init_code, access_list);
		let gasometer = &mut self.state.metadata_mut().gasometer;
		gasometer.record_transaction(transaction_cost)
	}

	fn maybe_record_init_code_cost(&mut self, init_code: &[u8]) -> Result<(), ExitError> {
		if let Some(limit) = self.config.max_initcode_size {
			// EIP-3860
			if init_code.len() > limit {
				self.state.metadata_mut().gasometer.fail();
				return Err(ExitError::CreateContractLimit);
			}
			return self
				.state
				.metadata_mut()
				.gasometer
				.record_cost(gasometer::init_code_cost(init_code));
		}
		Ok(())
	}

	/// Execute a `CREATE` transaction.
	pub fn transact_create(
		&mut self,
		caller: H160,
		value: U256,
		init_code: Vec<u8>,
		gas_limit: u64,
		access_list: Vec<(H160, Vec<H256>)>, // See EIP-2930
	) -> (ExitReason, Vec<u8>) {
		if self.nonce(caller) >= U64_MAX {
			return (ExitError::MaxNonce.into(), Vec::new());
		}

		let address = self.create_address(CreateScheme::Legacy { caller });

		event!(TransactCreate {
			caller,
			value,
			init_code: &init_code,
			gas_limit,
			address,
		});

		if let Some(limit) = self.config.max_initcode_size {
			if init_code.len() > limit {
				self.state.metadata_mut().gasometer.fail();
				return emit_exit!(ExitError::CreateContractLimit.into(), Vec::new());
			}
		}

		if let Err(e) = self.record_create_transaction_cost(&init_code, &access_list) {
			return emit_exit!(e.into(), Vec::new());
		}

		self.warm_addresses_and_storage(caller, address, access_list);

		match self.create_inner(
			caller,
			CreateScheme::Legacy { caller },
			value,
			init_code,
			Some(gas_limit),
			false,
		) {
			Capture::Exit((s, v)) => emit_exit!(s, v),
			Capture::Trap(rt) => {
				let mut cs: SmallVec<[TaggedRuntime<'_>; DEFAULT_CALL_STACK_CAPACITY]> =
					smallvec!(rt.0);
				let (s, _, v) = self.execute_with_call_stack(&mut cs);
				emit_exit!(s, v)
			}
		}
	}

	/// Same as `CREATE` but uses a specified address for created smart contract.
	#[cfg(feature = "create-fixed")]
	pub fn transact_create_fixed(
		&mut self,
		caller: H160,
		address: H160,
		value: U256,
		init_code: Vec<u8>,
		gas_limit: u64,
		access_list: Vec<(H160, Vec<H256>)>, // See EIP-2930
	) -> (ExitReason, Vec<u8>) {
		let address = self.create_address(CreateScheme::Fixed(address));

		event!(TransactCreate {
			caller,
			value,
			init_code: &init_code,
			gas_limit,
			address
		});

		if let Err(e) = self.record_create_transaction_cost(&init_code, &access_list) {
			return emit_exit!(e.into(), Vec::new());
		}

		self.warm_addresses_and_storage(caller, address, access_list);

		match self.create_inner(
			caller,
			CreateScheme::Fixed(address),
			value,
			init_code,
			Some(gas_limit),
			false,
		) {
			Capture::Exit((s, v)) => emit_exit!(s, v),
			Capture::Trap(rt) => {
				let mut cs: SmallVec<[TaggedRuntime<'_>; DEFAULT_CALL_STACK_CAPACITY]> =
					smallvec!(rt.0);
				let (s, _, v) = self.execute_with_call_stack(&mut cs);
				emit_exit!(s, v)
			}
		}
	}

	/// Execute a `CREATE2` transaction.
	#[allow(clippy::too_many_arguments)]
	pub fn transact_create2(
		&mut self,
		caller: H160,
		value: U256,
		init_code: Vec<u8>,
		salt: H256,
		gas_limit: u64,
		access_list: Vec<(H160, Vec<H256>)>, // See EIP-2930
	) -> (ExitReason, Vec<u8>) {
		if let Some(limit) = self.config.max_initcode_size {
			if init_code.len() > limit {
				self.state.metadata_mut().gasometer.fail();
				return emit_exit!(ExitError::CreateContractLimit.into(), Vec::new());
			}
		}

		let code_hash = H256::from_slice(Keccak256::digest(&init_code).as_slice());
		let address = self.create_address(CreateScheme::Create2 {
			caller,
			code_hash,
			salt,
		});
		event!(TransactCreate2 {
			caller,
			value,
			init_code: &init_code,
			salt,
			gas_limit,
			address,
		});

		if let Err(e) = self.record_create_transaction_cost(&init_code, &access_list) {
			return emit_exit!(e.into(), Vec::new());
		}

		self.warm_addresses_and_storage(caller, address, access_list);

		match self.create_inner(
			caller,
			CreateScheme::Create2 {
				caller,
				code_hash,
				salt,
			},
			value,
			init_code,
			Some(gas_limit),
			false,
		) {
			Capture::Exit((s, v)) => emit_exit!(s, v),
			Capture::Trap(rt) => {
				let mut cs: SmallVec<[TaggedRuntime<'_>; DEFAULT_CALL_STACK_CAPACITY]> =
					smallvec!(rt.0);
				let (s, _, v) = self.execute_with_call_stack(&mut cs);
				emit_exit!(s, v)
			}
		}
	}

	/// Execute a `CALL` transaction with a given parameters
	///
	/// ## Notes
	/// - `access_list` associated to [EIP-2930: Optional access lists](https://eips.ethereum.org/EIPS/eip-2930)
	/// - `authorization_list` associated to [EIP-7702: Authorized accounts](https://eips.ethereum.org/EIPS/eip-7702)
	#[allow(clippy::too_many_arguments)]
	pub fn transact_call(
		&mut self,
		caller: H160,
		address: H160,
		value: U256,
		data: Vec<u8>,
		gas_limit: u64,
		access_list: Vec<(H160, Vec<H256>)>,
		authorization_list: Vec<Authorization>,
	) -> (ExitReason, Vec<u8>) {
		event!(TransactCall {
			caller,
			address,
			value,
			data: &data,
			gas_limit,
		});

		if self.nonce(caller) >= U64_MAX {
			return (ExitError::MaxNonce.into(), Vec::new());
		}

		let transaction_cost =
			gasometer::call_transaction_cost(&data, &access_list, authorization_list.len());
		let gasometer = &mut self.state.metadata_mut().gasometer;
		match gasometer.record_transaction(transaction_cost) {
			Ok(()) => (),
			Err(e) => return emit_exit!(e.into(), Vec::new()),
		}

		if let Err(e) = self.state.inc_nonce(caller) {
			return (e.into(), Vec::new());
		}

		self.warm_addresses_and_storage(caller, address, access_list);
		// EIP-7702. authorized accounts
		// NOTE: it must be after `inc_nonce`
		if let Err(e) = self.authorized_accounts(authorization_list) {
			return (e.into(), Vec::new());
		}

		let context = Context {
			caller,
			address,
			apparent_value: value,
		};

		match self.call_inner(
			address,
			Some(Transfer {
				source: caller,
				target: address,
				value,
			}),
			data,
			Some(gas_limit),
			false,
			false,
			false,
			context,
		) {
			Capture::Exit((s, v)) => emit_exit!(s, v),
			Capture::Trap(rt) => {
				let mut cs: SmallVec<[TaggedRuntime<'_>; DEFAULT_CALL_STACK_CAPACITY]> =
					smallvec!(rt.0);
				let (s, _, v) = self.execute_with_call_stack(&mut cs);
				emit_exit!(s, v)
			}
		}
	}

	/// Get used gas for the current executor, given the price.
	pub fn used_gas(&mut self) -> u64 {
		// Avoid uncontrolled `u64` casting
		let refunded_gas =
			u64::try_from(self.state.metadata().gasometer.refunded_gas()).unwrap_or_default();
		let total_used_gas = self.state.metadata().gasometer.total_used_gas();
		// EIP-7623: max(total_used_gas, floor_gas)
		if self.config.has_floor_gas {
			let floor_gas = self.state.metadata().gasometer.floor_gas();
			// We explicitly record difference between: max(total_used_gas, floor_gas)
			if total_used_gas < floor_gas {
				let _ = self
					.state
					.metadata_mut()
					.gasometer
					.record_cost(floor_gas - total_used_gas);
			}
		}
		// TODOFEE
		// println!(
		// 	"TOTAL: {} | {refunded_gas}",
		// 	self.state.metadata().gasometer.total_used_gas()
		// );
		total_used_gas
			- min(
				total_used_gas / self.config.max_refund_quotient,
				refunded_gas,
			)
	}

	/// Get fee needed for the current executor, given the price.
	pub fn fee(&mut self, price: U256) -> U256 {
		let used_gas = self.used_gas();
		// TODOFEE
		// println!("FEE: {}", used_gas);
		U256::from(used_gas).saturating_mul(price)
	}

	/// Get account nonce.
	/// NOTE: we don't need to cache it as by default it's `MemoryStackState` with cache flow
	pub fn nonce(&self, address: H160) -> U256 {
		self.state.basic(address).nonce
	}

	/// Check if the existing account is "create collision".    
	/// [EIP-7610](https://eips.ethereum.org/EIPS/eip-7610)
	pub fn is_create_collision(&self, address: H160) -> bool {
		!self.code(address).is_empty()
			|| self.nonce(address) > U256::zero()
			|| !self.state.is_empty_storage(address)
	}

	/// Get the created address from given scheme.
	pub fn create_address(&self, scheme: CreateScheme) -> H160 {
		match scheme {
			CreateScheme::Create2 {
				caller,
				code_hash,
				salt,
			} => {
				let mut hasher = Keccak256::new();
				hasher.update([0xff]);
				hasher.update(&caller[..]);
				hasher.update(&salt[..]);
				hasher.update(&code_hash[..]);
				H256::from_slice(hasher.finalize().as_slice()).into()
			}
			CreateScheme::Legacy { caller } => {
				let nonce = self.nonce(caller);
				let mut stream = rlp::RlpStream::new_list(2);
				stream.append(&caller);
				stream.append(&nonce);
				H256::from_slice(Keccak256::digest(stream.out()).as_slice()).into()
			}
			CreateScheme::Fixed(address) => address,
		}
	}

	/// According to `EIP-2930` - `access_list` should be warmed.
	/// This function warms addresses and storage keys.
	///
	/// [EIP-2930: Optional access lists](https://eips.ethereum.org/EIPS/eip-2930)
	pub fn warm_access_list(&mut self, access_list: Vec<(H160, Vec<H256>)>) {
		let addresses = access_list.iter().map(|a| a.0);
		self.state.metadata_mut().access_addresses(addresses);

		let storage_keys = access_list
			.into_iter()
			.flat_map(|(address, keys)| keys.into_iter().map(move |key| (address, key)));
		self.state.metadata_mut().access_storages(storage_keys);
	}

	/// Warm addresses and storage keys.
	/// - According to `EIP-2929` the addresses should be warmed:
	///   1. caller (tx.sender)
	///   2. address (tx.to or the address being created if it is a contract creation transaction)
	/// - Warm coinbase according to `EIP-3651`
	/// - Warm `access_list` according to `EIP-2931`
	///
	/// ## References
	/// - [EIP-2929: Gas cost increases for state access opcodes](https://eips.ethereum.org/EIPS/eip-2929)
	/// - [EIP-2930: Optional access lists](https://eips.ethereum.org/EIPS/eip-2930)
	/// - [EIP-3651: Warm COINBASE](https://eips.ethereum.org/EIPS/eip-3651)
	fn warm_addresses_and_storage(
		&mut self,
		caller: H160,
		address: H160,
		access_list: Vec<(H160, Vec<H256>)>,
	) {
		if self.config.increase_state_access_gas {
			if self.config.warm_coinbase_address {
				// Warm coinbase address for EIP-3651
				let coinbase = self.block_coinbase();
				self.state
					.metadata_mut()
					.access_addresses([caller, address, coinbase].iter().copied());
			} else {
				self.state
					.metadata_mut()
					.access_addresses([caller, address].iter().copied());
			};

			self.warm_access_list(access_list);
		}
	}

	/// Authorized accounts behavior.
	///
	/// According to `EIP-7702` behavior section should be several steps of verifications.
	/// Current function includes steps 2.4-9 from the spec:
	/// 2. Verify the `nonce` is less than `2**64 - 1`.
	/// 4. Add `authority` to `accessed_addresses`
	/// 5. Verify the code of `authority` is either empty or already delegated.
	/// 6. Verify the `nonce` of `authority` is equal to `nonce` (of address).
	/// 7. Add `PER_EMPTY_ACCOUNT_COST - PER_AUTH_BASE_COST` gas to the global refund counter if authority exists in the trie.
	/// 8. Set the code of `authority` to be `0xef0100 || address`. This is a delegation designation.
	/// 9. Increase the `nonce` of `authority` by one.
	///
	/// It means, that steps 1,3 of spec must be passed before calling this function:
	/// 1. Verify the chain id is either 0 or the chain’s current ID.
	/// 3. `authority = ecrecover(...)`
	///
	/// See: [EIP-7702](https://eips.ethereum.org/EIPS/eip-7702#behavior)
	///
	/// ## Errors
	/// Return error if nonce increment return error.
	fn authorized_accounts(
		&mut self,
		authorization_list: Vec<Authorization>,
	) -> Result<(), ExitError> {
		if !self.config.has_authorization_list {
			return Ok(());
		}
		let mut refunded_accounts = 0;

		let state = self.state_mut();
		let mut warm_authority: Vec<H160> = Vec::with_capacity(authorization_list.len());
		for authority in authorization_list {
			// If EIP-7702 Spec validation steps 1, 3 return false.
			if !authority.is_valid {
				continue;
			}

			// 2. Verify the `nonce` is less than `2**64 - 1`.
			if U256::from(authority.nonce) >= U64_MAX {
				continue;
			}

			// 4. Add authority to accessed_addresses (as defined in EIP-2929)
			warm_authority.push(authority.authority);
			// 5. Verify the code of authority is either empty or already delegated.
			let authority_code = state.code(authority.authority);
			if !authority_code.is_empty() && !Authorization::is_delegated(&authority_code) {
				continue;
			}

			// 6. Verify the nonce of authority is equal to nonce.
			if state.basic(authority.authority).nonce != U256::from(authority.nonce) {
				continue;
			}

			// 7. Add PER_EMPTY_ACCOUNT_COST - PER_AUTH_BASE_COST gas to the global refund counter if authority exists in the trie.
			if !state.is_empty(authority.authority) {
				refunded_accounts += 1;
			}
			// 8. Set the code of authority to be `0xef0100 || address`. This is a delegation designation.
			// * As a special case, if address is 0x0000000000000000000000000000000000000000 do not write the designation.
			//   Clear the account’s code.
			let delegation_clearing = if authority.address.is_zero() {
				state.set_code(authority.authority, Vec::new());
				true
			} else {
				state.set_code(authority.authority, authority.delegation_code());
				false
			};
			// 9. Increase the nonce of authority by one.
			state.inc_nonce(authority.authority)?;

			// Add to authority access list cache
			if !delegation_clearing {
				state
					.metadata_mut()
					.add_authority(authority.authority, authority.address);
			}
		}
		// Warm addresses for [Step 4].
		self.state
			.metadata_mut()
			.access_addresses(warm_authority.into_iter());

		self.state
			.metadata_mut()
			.gasometer
			.record_authority_refund(refunded_accounts)
	}

	/// Calculate gas limit and record it in the gasometer.
	fn calc_gas_limit_and_record(
		&mut self,
		target_gas: Option<u64>,
		take_l64: bool,
	) -> Result<u64, ExitError> {
		let initial_after_gas = self.state.metadata().gasometer.gas();
		let after_gas = if take_l64 && self.config.call_l64_after_gas {
			if self.config.estimate {
				let diff = initial_after_gas - l64(initial_after_gas);
				self.state.metadata_mut().gasometer.record_cost(diff)?;
				initial_after_gas
			} else {
				l64(initial_after_gas)
			}
		} else {
			initial_after_gas
		};
		let target_gas = target_gas.unwrap_or(after_gas);
		let gas_limit = min(target_gas, after_gas);
		self.state.metadata_mut().gasometer.record_cost(gas_limit)?;
		Ok(gas_limit)
	}

	fn create_inner(
		&mut self,
		caller: H160,
		scheme: CreateScheme,
		value: U256,
		init_code: Vec<u8>,
		target_gas: Option<u64>,
		take_l64: bool,
	) -> Capture<(ExitReason, Vec<u8>), StackExecutorCreateInterrupt<'static>> {
		if self.nonce(caller) >= U64_MAX {
			return Capture::Exit((ExitError::MaxNonce.into(), Vec::new()));
		}

		// Warm address for EIP-2929
		let address = self.create_address(scheme);
		self.state
			.metadata_mut()
			.access_addresses([caller, address].iter().copied());

		event!(Create {
			caller,
			address,
			scheme,
			value,
			init_code: &init_code,
			target_gas
		});

		if let Some(depth) = self.state.metadata().depth {
			// As Depth incremented in `enter_substate` we must check depth counter
			// early to verify exceeding Stack limit. It allows avoid
			// issue with wrong detection `CallTooDeep` for Create.
			if depth + 1 > self.config.call_stack_limit {
				return Capture::Exit((ExitError::CallTooDeep.into(), Vec::new()));
			}
		}

		// Check is transfer value is enough
		if self.balance(caller) < value {
			return Capture::Exit((ExitError::OutOfFund.into(), Vec::new()));
		}

		let gas_limit = try_or_fail!(self.calc_gas_limit_and_record(target_gas, take_l64));

		// Check nonce and increment it for caller
		try_or_fail!(self.state.inc_nonce(caller));

		// Check create collision: EIP-7610
		if self.is_create_collision(address) {
			return Capture::Exit((ExitError::CreateCollision.into(), Vec::new()));
		}

		// Enter to execution substate
		self.enter_substate(gas_limit, false);

		// Check nonce and increment it for created address after  entering substate
		if self.config.create_increase_nonce {
			try_or_fail!(self.state.inc_nonce(address));
		}

		// Transfer funds if needed
		let transfer = Transfer {
			source: caller,
			target: address,
			value,
		};
		match self.state.transfer(transfer) {
			Ok(()) => (),
			Err(e) => {
				let _ = self.exit_substate(&StackExitKind::Reverted);
				return Capture::Exit((ExitReason::Error(e), Vec::new()));
			}
		}
		// It needed for CANCUN hard fork EIP-6780 we should mark account as created
		// to handle SELFDESTRUCT in the same transaction
		self.state.set_created(address);

		// Init EVM runtime in Context
		let context = Context {
			address,
			caller,
			apparent_value: value,
		};
		let runtime = Runtime::new(
			Rc::new(init_code),
			Rc::new(Vec::new()),
			context,
			self.config.stack_limit,
			self.config.memory_limit,
		);

		// Set Runtime kind with pre-init Runtime and return Trap, that mean continue execution
		Capture::Trap(StackExecutorCreateInterrupt(TaggedRuntime {
			kind: RuntimeKind::Create(address),
			inner: MaybeBorrowed::Owned(runtime),
		}))
	}

	#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
	fn call_inner(
		&mut self,
		code_address: H160,
		transfer: Option<Transfer>,
		input: Vec<u8>,
		target_gas: Option<u64>,
		is_static: bool,
		take_l64: bool,
		take_stipend: bool,
		context: Context,
	) -> Capture<(ExitReason, Vec<u8>), StackExecutorCallInterrupt<'static>> {
		event!(Call {
			code_address,
			transfer: &transfer,
			input: &input,
			target_gas,
			is_static,
			context: &context,
		});

		let mut gas_limit = try_or_fail!(self.calc_gas_limit_and_record(target_gas, take_l64));

		if let Some(transfer) = transfer.as_ref() {
			if take_stipend && transfer.value != U256::zero() {
				gas_limit = gas_limit.saturating_add(self.config.call_stipend);
			}
		}

		// EIP-7702 - get delegated designation address code
		// Detect loop for Delegated designation
		let code = self.authority_code(code_address);
		// Warm Delegated address after access
		if let Some(target_address) = self.get_authority_target(code_address) {
			self.warm_target((target_address, None));
		}

		self.enter_substate(gas_limit, is_static);
		self.state.touch(context.address);

		if let Some(depth) = self.state.metadata().depth {
			if depth > self.config.call_stack_limit {
				let _ = self.exit_substate(&StackExitKind::Reverted);
				return Capture::Exit((ExitError::CallTooDeep.into(), Vec::new()));
			}
		}

		// Transfer funds if needed
		if let Some(transfer) = transfer {
			match self.state.transfer(transfer) {
				Ok(()) => (),
				Err(e) => {
					let _ = self.exit_substate(&StackExitKind::Reverted);
					return Capture::Exit((ExitReason::Error(e), Vec::new()));
				}
			}
		}

		// At this point, the state has been modified in enter_substate to
		// reflect both the is_static parameter of this call and the is_static
		// of the caller context.
		let precompile_is_static = self.state.metadata().is_static();
		if let Some(result) = self.precompile_set.execute(&mut StackExecutorHandle {
			executor: self,
			code_address,
			input: &input,
			gas_limit: Some(gas_limit),
			context: &context,
			is_static: precompile_is_static,
		}) {
			return match result {
				Ok(PrecompileOutput {
					exit_status,
					output,
				}) => {
					let _ = self.exit_substate(&StackExitKind::Succeeded);
					Capture::Exit((ExitReason::Succeed(exit_status), output))
				}
				Err(PrecompileFailure::Error { exit_status }) => {
					let _ = self.exit_substate(&StackExitKind::Failed);
					Capture::Exit((ExitReason::Error(exit_status), Vec::new()))
				}
				Err(PrecompileFailure::Revert {
					exit_status,
					output,
				}) => {
					let _ = self.exit_substate(&StackExitKind::Reverted);
					Capture::Exit((ExitReason::Revert(exit_status), output))
				}
				Err(PrecompileFailure::Fatal { exit_status }) => {
					self.state.metadata_mut().gasometer.fail();
					let _ = self.exit_substate(&StackExitKind::Failed);
					Capture::Exit((ExitReason::Fatal(exit_status), Vec::new()))
				}
			};
		}

		let runtime = Runtime::new(
			Rc::new(code),
			Rc::new(input),
			context,
			self.config.stack_limit,
			self.config.memory_limit,
		);

		Capture::Trap(StackExecutorCallInterrupt(TaggedRuntime {
			kind: RuntimeKind::Call(code_address),
			inner: MaybeBorrowed::Owned(runtime),
		}))
	}

	fn cleanup_for_create(
		&mut self,
		created_address: H160,
		reason: ExitReason,
		return_data: Vec<u8>,
	) -> (ExitReason, Option<H160>, Vec<u8>) {
		// EIP-3541: Reject new contract code starting with the 0xEF byte (EOF Magic)
		fn check_first_byte_eof_magic(config: &Config, code: &[u8]) -> Result<(), ExitError> {
			if config.disallow_executable_format && Some(&0xEF) == code.first() {
				return Err(ExitError::CreateContractStartingWithEF);
			}
			Ok(())
		}

		log::debug!(target: "evm", "Create execution using address {}: {:?}", created_address, reason);

		match reason {
			ExitReason::Succeed(s) => {
				let out = return_data;
				let address = created_address;
				// As of EIP-3541 code starting with 0xef cannot be deployed
				if let Err(e) = check_first_byte_eof_magic(self.config, &out) {
					self.state.metadata_mut().gasometer.fail();
					let _ = self.exit_substate(&StackExitKind::Failed);
					return (e.into(), None, Vec::new());
				}

				if let Some(limit) = self.config.create_contract_limit {
					if out.len() > limit {
						self.state.metadata_mut().gasometer.fail();
						let _ = self.exit_substate(&StackExitKind::Failed);
						return (ExitError::CreateContractLimit.into(), None, Vec::new());
					}
				}

				match self
					.state
					.metadata_mut()
					.gasometer
					.record_deposit(out.len())
				{
					Ok(()) => {
						let exit_result = self.exit_substate(&StackExitKind::Succeeded);
						event!(CreateOutput {
							address,
							code: &out,
						});
						self.state.set_code(address, out);
						if let Err(e) = exit_result {
							return (e.into(), None, Vec::new());
						}
						(ExitReason::Succeed(s), Some(address), Vec::new())
					}
					Err(e) => {
						let _ = self.exit_substate(&StackExitKind::Failed);
						(ExitReason::Error(e), None, Vec::new())
					}
				}
			}
			ExitReason::Error(e) => {
				self.state.metadata_mut().gasometer.fail();
				let _ = self.exit_substate(&StackExitKind::Failed);
				(ExitReason::Error(e), None, Vec::new())
			}
			ExitReason::Revert(e) => {
				let _ = self.exit_substate(&StackExitKind::Reverted);
				(ExitReason::Revert(e), None, return_data)
			}
			ExitReason::Fatal(e) => {
				self.state.metadata_mut().gasometer.fail();
				let _ = self.exit_substate(&StackExitKind::Failed);
				(ExitReason::Fatal(e), None, Vec::new())
			}
		}
	}

	fn cleanup_for_call(
		&mut self,
		code_address: H160,
		reason: &ExitReason,
		return_data: Vec<u8>,
	) -> Vec<u8> {
		log::debug!(target: "evm", "Call execution using address {}: {:?}", code_address, reason);
		match reason {
			ExitReason::Succeed(_) => {
				let _ = self.exit_substate(&StackExitKind::Succeeded);
				return_data
			}
			ExitReason::Error(_) => {
				let _ = self.exit_substate(&StackExitKind::Failed);
				Vec::new()
			}
			ExitReason::Revert(_) => {
				let _ = self.exit_substate(&StackExitKind::Reverted);
				return_data
			}
			ExitReason::Fatal(_) => {
				self.state.metadata_mut().gasometer.fail();
				let _ = self.exit_substate(&StackExitKind::Failed);
				Vec::new()
			}
		}
	}

	/// Check whether an address has already been created.
	fn is_created(&self, address: H160) -> bool {
		self.state.is_created(address)
	}
}

impl<'config, S: StackState<'config>, P: PrecompileSet> InterpreterHandler
	for StackExecutor<'config, '_, S, P>
{
	#[inline]
	fn before_eval(&mut self) {}

	#[inline]
	fn after_eval(&mut self) {}

	#[inline]
	fn before_bytecode(
		&mut self,
		opcode: Opcode,
		_pc: usize,
		machine: &Machine,
		address: &H160,
	) -> Result<(), ExitError> {
		#[cfg(feature = "tracing")]
		{
			use evm_runtime::tracing::Event::Step;
			evm_runtime::tracing::with(|listener| {
				#[allow(clippy::used_underscore_binding)]
				listener.event(Step {
					address: *address,
					opcode,
					position: &Ok(_pc),
					stack: machine.stack(),
					memory: machine.memory(),
				});
			});
		}

		#[cfg(feature = "print-debug")]
		println!("### {opcode}");
		if let Some(cost) = gasometer::static_opcode_cost(opcode) {
			self.state
				.metadata_mut()
				.gasometer
				.record_cost(u64::from(cost))?;
		} else {
			let is_static = self.state.metadata().is_static;
			let (gas_cost, memory_cost) = gasometer::dynamic_opcode_cost(
				*address,
				opcode,
				machine.stack(),
				is_static,
				self.config,
				self,
			)?;

			self.state
				.metadata_mut()
				.gasometer
				.record_dynamic_cost(gas_cost, memory_cost)?;
		}
		Ok(())
	}

	#[inline]
	fn after_bytecode(
		&mut self,
		_result: &Result<(), Capture<ExitReason, Trap>>,
		_machine: &Machine,
	) {
		#[cfg(feature = "tracing")]
		{
			use evm_runtime::tracing::Event::StepResult;
			evm_runtime::tracing::with(|listener| {
				#[allow(clippy::used_underscore_binding)]
				listener.event(StepResult {
					result: _result,
					return_value: _machine.return_value().as_slice(),
				});
			});
		}
	}
}

pub struct StackExecutorCallInterrupt<'borrow>(TaggedRuntime<'borrow>);

pub struct StackExecutorCreateInterrupt<'borrow>(TaggedRuntime<'borrow>);

impl<'config, S: StackState<'config>, P: PrecompileSet> Handler
	for StackExecutor<'config, '_, S, P>
{
	type CreateInterrupt = StackExecutorCreateInterrupt<'static>;
	type CreateFeedback = Infallible;
	type CallInterrupt = StackExecutorCallInterrupt<'static>;
	type CallFeedback = Infallible;

	/// Get account balance
	/// NOTE: we don't need to cache it as by default it's `MemoryStackState` with cache flow
	fn balance(&self, address: H160) -> U256 {
		self.state.basic(address).balance
	}

	/// Fetch the code size of an address.
	/// Provide a default implementation by fetching the code.
	///
	/// According to EIP-7702, the code size of an address is the size of the
	/// `0xef01` - `2`.
	/// <https://eips.ethereum.org/EIPS/eip-7702#delegation-designation>
	fn code_size(&mut self, address: H160) -> U256 {
		//  If deleted account, return size equal to 2 bytes
		if self.get_authority_target(address).is_some() {
			U256::from(2)
		} else {
			let target_code = self.authority_code(address);
			U256::from(target_code.len())
		}
	}

	/// Fetch the code hash of an address.
	/// Provide a default implementation by fetching the code.
	///
	/// According to EIP-7702, the code hash of an address is the hash of the
	/// `keccak256(0xef01): 0xeadcdba66a79ab5dce91622d1d75c8cff5cff0b96944c3bf1072cd08ce018329`.
	/// <https://eips.ethereum.org/EIPS/eip-7702#delegation-designation>
	fn code_hash(&mut self, address: H160) -> H256 {
		if !self.exists(address) {
			return H256::default();
		}
		if self.get_authority_target(address).is_some() {
			H256::from(EIP7702_MAGIC_HASH)
		} else {
			H256::from_slice(Keccak256::digest(self.code(address)).as_slice())
		}
	}

	/// Get account code
	fn code(&self, address: H160) -> Vec<u8> {
		self.state.code(address)
	}

	/// Get account storage by index
	fn storage(&self, address: H160, index: H256) -> H256 {
		self.state.storage(address, index)
	}

	/// Check is account storage empty
	fn is_empty_storage(&self, address: H160) -> bool {
		self.state.is_empty(address)
	}

	fn original_storage(&self, address: H160, index: H256) -> H256 {
		self.state
			.original_storage(address, index)
			.unwrap_or_default()
	}

	/// Check is account exists on backend side
	fn exists(&self, address: H160) -> bool {
		if self.config.empty_considered_exists {
			self.state.exists(address)
		} else {
			self.state.exists(address) && !self.state.is_empty(address)
		}
	}

	fn is_cold(&mut self, address: H160, maybe_index: Option<H256>) -> bool {
		match maybe_index {
			None => !self.precompile_set.is_precompile(address) && self.state.is_cold(address),
			Some(index) => self.state.is_storage_cold(address, index),
		}
	}

	fn gas_left(&self) -> U256 {
		U256::from(self.state.metadata().gasometer.gas())
	}

	fn gas_price(&self) -> U256 {
		self.state.gas_price()
	}

	fn origin(&self) -> H160 {
		self.state.origin()
	}

	fn block_hash(&self, number: U256) -> H256 {
		self.state.block_hash(number)
	}
	fn block_number(&self) -> U256 {
		self.state.block_number()
	}
	fn block_coinbase(&self) -> H160 {
		self.state.block_coinbase()
	}
	fn block_timestamp(&self) -> U256 {
		self.state.block_timestamp()
	}
	fn block_difficulty(&self) -> U256 {
		self.state.block_difficulty()
	}
	fn block_randomness(&self) -> Option<H256> {
		self.state.block_randomness()
	}
	fn block_gas_limit(&self) -> U256 {
		self.state.block_gas_limit()
	}
	fn block_base_fee_per_gas(&self) -> U256 {
		self.state.block_base_fee_per_gas()
	}
	fn chain_id(&self) -> U256 {
		self.state.chain_id()
	}
	fn deleted(&self, address: H160) -> bool {
		self.state.deleted(address)
	}

	fn set_storage(&mut self, address: H160, index: H256, value: H256) -> Result<(), ExitError> {
		self.state.set_storage(address, index, value);
		Ok(())
	}

	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) -> Result<(), ExitError> {
		self.state.log(address, topics, data);
		Ok(())
	}

	/// Mark account as deleted
	/// - SELFDESTRUCT - CANCUN hard fork: EIP-6780
	fn mark_delete(&mut self, address: H160, target: H160) -> Result<(), ExitError> {
		let is_created = self.is_created(address);
		// SELFDESTRUCT - CANCUN hard fork: EIP-6780 - selfdestruct only if contract is created in the same tx
		if self.config.has_restricted_selfdestruct && !is_created && address == target {
			// State is not changed:
			// * if we are after Cancun upgrade specify the target is
			// same as selfdestructed account. The balance stays unchanged.
			return Ok(());
		}

		let balance = self.balance(address);

		event!(Suicide {
			target,
			address,
			balance,
		});

		self.state.transfer(Transfer {
			source: address,
			target,
			value: balance,
		})?;
		self.state.reset_balance(address);
		// For CANCUN hard fork SELFDESTRUCT (EIP-6780) state is not changed
		// or if SELFDESTRUCT in the same TX - account should selfdestruct
		if !self.config.has_restricted_selfdestruct || self.is_created(address) {
			self.state.set_deleted(address);
		}

		Ok(())
	}

	#[cfg(not(feature = "tracing"))]
	fn create(
		&mut self,
		caller: H160,
		scheme: CreateScheme,
		value: U256,
		init_code: Vec<u8>,
		target_gas: Option<u64>,
	) -> Capture<(ExitReason, Vec<u8>), Self::CreateInterrupt> {
		if let Err(e) = self.maybe_record_init_code_cost(&init_code) {
			let reason: ExitReason = e.into();
			emit_exit!(reason.clone());
			return Capture::Exit((reason, Vec::new()));
		}
		self.create_inner(caller, scheme, value, init_code, target_gas, true)
	}

	#[cfg(feature = "tracing")]
	fn create(
		&mut self,
		caller: H160,
		scheme: CreateScheme,
		value: U256,
		init_code: Vec<u8>,
		target_gas: Option<u64>,
	) -> Capture<(ExitReason, Vec<u8>), Self::CreateInterrupt> {
		if let Err(e) = self.maybe_record_init_code_cost(&init_code) {
			let reason: ExitReason = e.into();
			emit_exit!(reason.clone());
			return Capture::Exit((reason, Vec::new()));
		}

		let capture = self.create_inner(caller, scheme, value, init_code, target_gas, true);

		if let Capture::Exit((ref reason, ref return_value)) = capture {
			emit_exit!(reason, return_value);
		}

		capture
	}

	#[cfg(not(feature = "tracing"))]
	fn call(
		&mut self,
		code_address: H160,
		transfer: Option<Transfer>,
		input: Vec<u8>,
		target_gas: Option<u64>,
		is_static: bool,
		context: Context,
	) -> Capture<(ExitReason, Vec<u8>), Self::CallInterrupt> {
		self.call_inner(
			code_address,
			transfer,
			input,
			target_gas,
			is_static,
			true,
			true,
			context,
		)
	}

	#[cfg(feature = "tracing")]
	fn call(
		&mut self,
		code_address: H160,
		transfer: Option<Transfer>,
		input: Vec<u8>,
		target_gas: Option<u64>,
		is_static: bool,
		context: Context,
	) -> Capture<(ExitReason, Vec<u8>), Self::CallInterrupt> {
		let capture = self.call_inner(
			code_address,
			transfer,
			input,
			target_gas,
			is_static,
			true,
			true,
			context,
		);

		if let Capture::Exit((ref reason, ref return_value)) = capture {
			emit_exit!(reason, return_value);
		}

		capture
	}

	fn record_external_operation(&mut self, op: crate::ExternalOperation) -> Result<(), ExitError> {
		self.state.record_external_operation(op)
	}

	/// Returns `None` if `Cancun` hard fork is not enabled
	/// via `has_blob_base_fee` config.
	///
	/// [EIP-4844]: Shard Blob Transactions
	/// [EIP-7516]: BLOBBASEFEE instruction
	fn blob_base_fee(&self) -> Option<u128> {
		if self.config.has_blob_base_fee {
			self.state.blob_gas_price()
		} else {
			None
		}
	}

	fn get_blob_hash(&self, index: usize) -> Option<U256> {
		if self.config.has_shard_blob_transactions {
			self.state.get_blob_hash(index)
		} else {
			None
		}
	}

	fn tstore(&mut self, address: H160, index: H256, value: U256) -> Result<(), ExitError> {
		if self.config.has_transient_storage {
			self.state.tstore(address, index, value)
		} else {
			Err(ExitError::InvalidCode(Opcode::TSTORE))
		}
	}

	fn tload(&mut self, address: H160, index: H256) -> Result<U256, ExitError> {
		if self.config.has_transient_storage {
			self.state.tload(address, index)
		} else {
			Err(ExitError::InvalidCode(Opcode::TLOAD))
		}
	}

	/// Return the target address of the authority delegation designation (EIP-7702).
	fn get_authority_target(&mut self, address: H160) -> Option<H160> {
		if self.config.has_authorization_list {
			self.state.get_authority_target(address)
		} else {
			None
		}
	}

	/// Get delegation designator code for the authority code.
	/// If the code of address is delegation designator, then retrieve code
	/// from the designation address for the `authority`.
	/// Detect delegated designation loop and return basic byte code for loop.
	///
	/// It's related to [EIP-7702 Delegation Designation](https://eips.ethereum.org/EIPS/eip-7702#delegation-designation)
	/// When authority code is found, it should set delegated address to `authority_access` array for
	/// calculating additional gas cost. Gas must be charged for the authority address and
	/// for delegated address, for detection is address warm or cold.
	fn authority_code(&mut self, authority: H160) -> Vec<u8> {
		if !self.config.has_authorization_list {
			return self.code(authority);
		}
		// Check if it is a loop for Delegated designation
		self.get_authority_target(authority).map_or_else(
			|| self.code(authority),
			|target_address| self.code(target_address),
		)
	}

	// Warm target according to EIP-2929
	// It warm up the target address or storage value by key. If in the target tuple
	// the storage is `None` then it's warming up the address.
	fn warm_target(&mut self, target: (H160, Option<H256>)) {
		match target {
			(address, None) => self.state.metadata_mut().access_address(address),
			(address, Some(key)) => self.state.metadata_mut().access_storage(address, key),
		}
	}
}

struct StackExecutorHandle<'inner, 'config, 'precompiles, S, P> {
	executor: &'inner mut StackExecutor<'config, 'precompiles, S, P>,
	code_address: H160,
	input: &'inner [u8],
	gas_limit: Option<u64>,
	context: &'inner Context,
	is_static: bool,
}

impl<'config, S: StackState<'config>, P: PrecompileSet> PrecompileHandle
	for StackExecutorHandle<'_, 'config, '_, S, P>
{
	// Perform subcall in provided context.
	/// Precompile specifies in which context the subcall is executed.
	fn call(
		&mut self,
		code_address: H160,
		transfer: Option<Transfer>,
		input: Vec<u8>,
		gas_limit: Option<u64>,
		is_static: bool,
		context: &Context,
	) -> (ExitReason, Vec<u8>) {
		// For normal calls the cost is recorded at opcode level.
		// Since we don't go through opcodes we need manually record the call
		// cost. Not doing so will make the code panic as recording the call stipend
		// will do an underflow.
		let target_is_cold = self.executor.is_cold(code_address, None);
		let delegated_designator_is_cold = self
			.executor
			.get_authority_target(code_address)
			.map(|target| self.executor.is_cold(target, None));

		let gas_cost = gasometer::GasCost::Call {
			value: transfer.clone().map_or_else(U256::zero, |x| x.value),
			gas: U256::from(gas_limit.unwrap_or(u64::MAX)),
			target_is_cold,
			delegated_designator_is_cold,
			target_exists: self.executor.exists(code_address),
		};

		// We record the length of the input.
		let memory_cost = Some(gasometer::MemoryCost {
			offset: 0,
			len: input.len(),
		});

		if let Err(error) = self
			.executor
			.state
			.metadata_mut()
			.gasometer
			.record_dynamic_cost(gas_cost, memory_cost)
		{
			return (ExitReason::Error(error), Vec::new());
		}

		event!(PrecompileSubcall {
			code_address,
			transfer: &transfer,
			input: &input,
			target_gas: gas_limit,
			is_static,
			context
		});

		// Perform the subcall
		match Handler::call(
			self.executor,
			code_address,
			transfer,
			input,
			gas_limit,
			is_static,
			context.clone(),
		) {
			Capture::Exit((s, v)) => (s, v),
			Capture::Trap(rt) => {
				// Ideally this would pass the interrupt back to the executor so it could be
				// handled like any other call, however the type signature of this function does
				// not allow it. For now we'll make a recursive call instead of making a breaking
				// change to the precompile API. But this means a custom precompile could still
				// potentially cause a stack overflow if you're not careful.
				let mut call_stack: SmallVec<[TaggedRuntime; DEFAULT_CALL_STACK_CAPACITY]> =
					smallvec!(rt.0);
				let (reason, _, return_data) =
					self.executor.execute_with_call_stack(&mut call_stack);
				emit_exit!(reason, return_data)
			}
		}
	}

	/// Record cost to the Runtime gasometer.
	fn record_cost(&mut self, cost: u64) -> Result<(), ExitError> {
		self.executor
			.state
			.metadata_mut()
			.gasometer
			.record_cost(cost)
	}

	/// Record Substrate specific cost.
	fn record_external_cost(
		&mut self,
		ref_time: Option<u64>,
		proof_size: Option<u64>,
		storage_growth: Option<u64>,
	) -> Result<(), ExitError> {
		self.executor
			.state
			.record_external_cost(ref_time, proof_size, storage_growth)
	}

	/// Refund Substrate specific cost.
	fn refund_external_cost(&mut self, ref_time: Option<u64>, proof_size: Option<u64>) {
		self.executor
			.state
			.refund_external_cost(ref_time, proof_size);
	}

	/// Retrieve the remaining gas.
	fn remaining_gas(&self) -> u64 {
		self.executor.state.metadata().gasometer.gas()
	}

	/// Record a log.
	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) -> Result<(), ExitError> {
		Handler::log(self.executor, address, topics, data)
	}

	/// Retrieve the code address (what is the address of the precompile being called).
	fn code_address(&self) -> H160 {
		self.code_address
	}

	/// Retrieve the input data the precompile is called with.
	fn input(&self) -> &[u8] {
		self.input
	}

	/// Retrieve the context in which the precompile is executed.
	fn context(&self) -> &Context {
		self.context
	}

	/// Is the precompile call is done statically.
	fn is_static(&self) -> bool {
		self.is_static
	}

	/// Retrieve the gas limit of this call.
	fn gas_limit(&self) -> Option<u64> {
		self.gas_limit
	}
}

#[cfg(test)]
mod tests {
	use super::EIP7702_MAGIC_HASH;
	use sha3::{Digest, Keccak256};
	#[test]
	fn test_ef01_hash() {
		let hash = Keccak256::digest([0xEF, 0x01]);
		assert_eq!(hash.as_slice(), EIP7702_MAGIC_HASH);
	}
}
