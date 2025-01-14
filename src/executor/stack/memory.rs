use crate::backend::{Apply, Backend, Basic, Log};
use crate::executor::stack::executor::{
	Accessed, Authorization, StackState, StackSubstateMetadata,
};
use crate::prelude::*;
use crate::{ExitError, Transfer};
use core::mem;
use evm_core::utils::U64_MAX;
use primitive_types::{H160, H256, U256};

#[derive(Clone, Debug)]
pub struct MemoryStackAccount {
	pub basic: Basic,
	pub code: Option<Vec<u8>>,
	pub reset: bool,
}

#[derive(Clone, Debug)]
pub struct MemoryStackSubstate<'config> {
	metadata: StackSubstateMetadata<'config>,
	parent: Option<Box<MemoryStackSubstate<'config>>>,
	logs: Vec<Log>,
	accounts: BTreeMap<H160, MemoryStackAccount>,
	storages: BTreeMap<(H160, H256), H256>,
	tstorages: BTreeMap<(H160, H256), U256>,
	deletes: BTreeSet<H160>,
	creates: BTreeSet<H160>,
}

impl<'config> MemoryStackSubstate<'config> {
	#[must_use]
	pub const fn new(metadata: StackSubstateMetadata<'config>) -> Self {
		Self {
			metadata,
			parent: None::<Box<_>>,
			logs: Vec::new(),
			accounts: BTreeMap::new(),
			storages: BTreeMap::new(),
			tstorages: BTreeMap::new(),
			deletes: BTreeSet::new(),
			creates: BTreeSet::new(),
		}
	}

	#[must_use]
	pub fn logs(&self) -> &[Log] {
		&self.logs
	}

	pub fn logs_mut(&mut self) -> &mut Vec<Log> {
		&mut self.logs
	}

	#[must_use]
	pub const fn metadata(&self) -> &StackSubstateMetadata<'config> {
		&self.metadata
	}

	pub fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		&mut self.metadata
	}

	/// Deconstruct the memory stack substate, return state to be applied. Panic if the
	/// substate is not in the top-level substate.
	///
	/// # Panics
	/// Panic if parent presents
	#[must_use]
	pub fn deconstruct<B: Backend>(
		mut self,
		backend: &B,
	) -> (
		impl IntoIterator<Item = Apply<impl IntoIterator<Item = (H256, H256)>>>,
		impl IntoIterator<Item = Log>,
	) {
		assert!(self.parent.is_none());

		let mut applies = Vec::<Apply<BTreeMap<H256, H256>>>::new();

		let mut addresses = BTreeSet::new();

		for address in self.accounts.keys() {
			addresses.insert(*address);
		}

		for (address, _) in self.storages.keys() {
			addresses.insert(*address);
		}

		for address in addresses {
			if self.deletes.contains(&address) {
				continue;
			}

			let mut storage = BTreeMap::new();
			for ((oa, ok), ov) in &self.storages {
				if *oa == address {
					storage.insert(*ok, *ov);
				}
			}

			let apply = {
				let account = if self.is_created(address) {
					let account = self
						.accounts
						.get_mut(&address)
						.expect("New account was just inserted");
					// Reset storage for CREATE call as initially it's always should be empty.
					// NOTE: related to `ethereum-tests`: `stSStoreTest/InitCollisionParis.json`
					account.reset = true;
					account
				} else {
					self.account_mut(address, backend)
				};

				Apply::Modify {
					address,
					basic: account.basic.clone(),
					code: account.code.clone(),
					storage,
					reset_storage: account.reset,
				}
			};

			applies.push(apply);
		}

		for address in self.deletes {
			applies.push(Apply::Delete { address });
		}

		(applies, self.logs)
	}

	pub fn enter(&mut self, gas_limit: u64, is_static: bool) {
		let mut entering = Self {
			metadata: self.metadata.spit_child(gas_limit, is_static),
			parent: None,
			logs: Vec::new(),
			accounts: BTreeMap::new(),
			storages: BTreeMap::new(),
			tstorages: BTreeMap::new(),
			deletes: BTreeSet::new(),
			creates: BTreeSet::new(),
		};
		mem::swap(&mut entering, self);

		self.parent = Some(Box::new(entering));
	}

	/// Exit commit represent successful execution of the `substate`.
	///
	/// It includes:
	/// - swallow commit
	///   - gas recording
	///   - warmed accesses merging
	/// - logs merging
	/// - for account existed from substate with reset flag, remove storages by keys
	/// - merge substate data: accounts, storages, tstorages, deletes, creates
	///
	/// # Errors
	/// Return `ExitError` that is thrown by gasometer gas calculation errors.
	///
	/// # Panics
	/// Cannot commit on root `substate` i.e. it forces to panic.
	pub fn exit_commit(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot commit on root substate");
		mem::swap(&mut exited, self);

		self.metadata.swallow_commit(exited.metadata)?;
		self.logs.append(&mut exited.logs);

		let mut resets = BTreeSet::new();
		for (address, account) in &exited.accounts {
			if account.reset {
				resets.insert(*address);
			}
		}
		let mut reset_keys = BTreeSet::new();
		for (address, key) in self.storages.keys() {
			if resets.contains(address) {
				reset_keys.insert((*address, *key));
			}
		}
		for (address, key) in reset_keys {
			self.storages.remove(&(address, key));
		}

		self.accounts.append(&mut exited.accounts);
		self.storages.append(&mut exited.storages);
		self.tstorages.append(&mut exited.tstorages);
		self.deletes.append(&mut exited.deletes);
		self.creates.append(&mut exited.creates);
		Ok(())
	}

	/// Exit revert. Represents revert execution of the `substate`.
	///
	/// # Errors
	/// Return `ExitError`
	///
	/// # Panics
	/// Cannot discard on root substate
	pub fn exit_revert(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);
		self.metadata.swallow_revert(&exited.metadata)?;
		Ok(())
	}

	/// Exit discard. Represents discard execution of the `substate`.
	///
	/// # Errors
	/// Return `ExitError`. At the momoet it's not throwing any real error.
	///
	/// # Panics
	/// Cannot discard on root substate
	pub fn exit_discard(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);
		self.metadata.swallow_discard(&exited.metadata);
		Ok(())
	}

	pub fn known_account(&self, address: H160) -> Option<&MemoryStackAccount> {
		self.accounts.get(&address).map_or_else(
			|| {
				self.parent
					.as_ref()
					.and_then(|parent| parent.known_account(address))
			},
			Some,
		)
	}

	#[must_use]
	pub fn known_basic(&self, address: H160) -> Option<Basic> {
		self.known_account(address).map(|acc| acc.basic.clone())
	}

	#[must_use]
	pub fn known_code(&self, address: H160) -> Option<Vec<u8>> {
		self.known_account(address).and_then(|acc| acc.code.clone())
	}

	#[must_use]
	pub fn known_empty(&self, address: H160) -> Option<bool> {
		if let Some(account) = self.known_account(address) {
			if account.basic.balance != U256::zero() {
				return Some(false);
			}

			if account.basic.nonce != U256::zero() {
				return Some(false);
			}

			if let Some(code) = &account.code {
				return Some(
					account.basic.balance == U256::zero()
						&& account.basic.nonce == U256::zero()
						&& code.is_empty(),
				);
			}
		}

		None
	}

	#[must_use]
	pub fn known_storage(&self, address: H160, key: H256) -> Option<H256> {
		if let Some(value) = self.storages.get(&(address, key)) {
			return Some(*value);
		}

		if let Some(account) = self.accounts.get(&address) {
			if account.reset {
				return Some(H256::default());
			}
		}

		if let Some(parent) = self.parent.as_ref() {
			return parent.known_storage(address, key);
		}

		None
	}

	#[must_use]
	pub fn known_original_storage(&self, address: H160) -> Option<H256> {
		if let Some(account) = self.accounts.get(&address) {
			if account.reset {
				return Some(H256::default());
			}
		}

		if let Some(parent) = self.parent.as_ref() {
			return parent.known_original_storage(address);
		}

		None
	}

	#[must_use]
	pub fn is_cold(&self, address: H160) -> bool {
		self.recursive_is_cold(&|a| a.accessed_addresses.contains(&address))
	}

	#[must_use]
	pub fn is_storage_cold(&self, address: H160, key: H256) -> bool {
		self.recursive_is_cold(&|a: &Accessed| a.accessed_storage.contains(&(address, key)))
	}

	fn recursive_is_cold<F: Fn(&Accessed) -> bool>(&self, f: &F) -> bool {
		let local_is_accessed = self.metadata.accessed().as_ref().is_some_and(f);
		if local_is_accessed {
			false
		} else {
			self.parent
				.as_ref()
				.map_or(true, |p| p.recursive_is_cold(f))
		}
	}

	#[must_use]
	pub fn deleted(&self, address: H160) -> bool {
		if self.deletes.contains(&address) {
			return true;
		}

		if let Some(parent) = self.parent.as_ref() {
			return parent.deleted(address);
		}

		false
	}

	#[allow(clippy::map_entry)]
	fn account_mut<B: Backend>(&mut self, address: H160, backend: &B) -> &mut MemoryStackAccount {
		if !self.accounts.contains_key(&address) {
			let account = self.known_account(address).cloned().map_or_else(
				|| MemoryStackAccount {
					basic: backend.basic(address),
					code: None::<Vec<_>>,
					reset: false,
				},
				|mut v| {
					v.reset = false;
					v
				},
			);
			self.accounts.insert(address, account);
		}

		self.accounts
			.get_mut(&address)
			.expect("New account was just inserted")
	}

	/// # Errors
	/// Return `ExitError`
	pub fn inc_nonce<B: Backend>(&mut self, address: H160, backend: &B) -> Result<(), ExitError> {
		let nonce = &mut self.account_mut(address, backend).basic.nonce;
		if *nonce >= U64_MAX {
			return Err(ExitError::MaxNonce);
		}
		*nonce += U256::one();
		Ok(())
	}

	pub fn set_storage(&mut self, address: H160, key: H256, value: H256) {
		#[cfg(feature = "print-debug")]
		println!("    [SSTORE {address:?}] {key:?}:{value:?}");
		self.storages.insert((address, key), value);
	}

	pub fn reset_storage<B: Backend>(&mut self, address: H160, backend: &B) {
		let mut removing = Vec::new();

		for (oa, ok) in self.storages.keys() {
			if *oa == address {
				removing.push(*ok);
			}
		}

		for ok in removing {
			self.storages.remove(&(address, ok));
		}
		self.account_mut(address, backend).reset = true;
	}

	pub fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) {
		self.logs.push(Log {
			address,
			topics,
			data,
		});
	}

	pub fn set_deleted(&mut self, address: H160) {
		self.deletes.insert(address);
	}

	pub fn set_created(&mut self, address: H160) {
		self.creates.insert(address);
	}

	#[must_use]
	pub fn is_created(&self, address: H160) -> bool {
		if self.creates.contains(&address) {
			return true;
		}

		if let Some(parent) = self.parent.as_ref() {
			return parent.is_created(address);
		}

		false
	}

	pub fn set_code<B: Backend>(&mut self, address: H160, code: Vec<u8>, backend: &B) {
		self.account_mut(address, backend).code = Some(code);
	}

	/// # Errors
	/// Return `ExitError`
	pub fn transfer<B: Backend>(
		&mut self,
		transfer: &Transfer,
		backend: &B,
	) -> Result<(), ExitError> {
		{
			let source = self.account_mut(transfer.source, backend);
			if source.basic.balance < transfer.value {
				return Err(ExitError::OutOfFund);
			}
			source.basic.balance -= transfer.value;
		}

		{
			let target = self.account_mut(transfer.target, backend);
			target.basic.balance = target.basic.balance.saturating_add(transfer.value);
		}

		Ok(())
	}

	/// Only needed for jsontests.
	/// # Errors
	/// Return `ExitError`
	pub fn withdraw<B: Backend>(
		&mut self,
		address: H160,
		value: U256,
		backend: &B,
	) -> Result<(), ExitError> {
		let source = self.account_mut(address, backend);
		if source.basic.balance < value {
			return Err(ExitError::OutOfFund);
		}
		source.basic.balance -= value;

		Ok(())
	}

	// Only needed for jsontests.
	pub fn deposit<B: Backend>(&mut self, address: H160, value: U256, backend: &B) {
		let target = self.account_mut(address, backend);
		target.basic.balance = target.basic.balance.saturating_add(value);
	}

	pub fn reset_balance<B: Backend>(&mut self, address: H160, backend: &B) {
		self.account_mut(address, backend).basic.balance = U256::zero();
	}

	pub fn touch<B: Backend>(&mut self, address: H160, backend: &B) {
		self.account_mut(address, backend);
	}

	#[must_use]
	pub fn get_tstorage(&self, address: H160, key: H256) -> U256 {
		self.known_tstorage(address, key).unwrap_or_default()
	}

	#[must_use]
	pub fn known_tstorage(&self, address: H160, key: H256) -> Option<U256> {
		if let Some(value) = self.tstorages.get(&(address, key)) {
			return Some(*value);
		}
		if let Some(parent) = self.parent.as_ref() {
			return parent.known_tstorage(address, key);
		}
		None
	}

	pub fn set_tstorage(&mut self, address: H160, key: H256, value: U256) {
		self.tstorages.insert((address, key), value);
	}

	/// Get authority target from the current state. If it's `None` just take a look
	/// recursively in the parent state.
	fn get_authority_target_recursive(&self, authority: H160) -> Option<H160> {
		if let Some(target) = self
			.metadata
			.accessed()
			.as_ref()
			.and_then(|accessed| accessed.get_authority_target(authority))
		{
			return Some(target);
		}
		self.parent
			.as_ref()
			.and_then(|p| p.get_authority_target_recursive(authority))
	}
}

#[derive(Clone, Debug)]
pub struct MemoryStackState<'backend, 'config, B> {
	backend: &'backend B,
	substate: MemoryStackSubstate<'config>,
}

impl<B: Backend> Backend for MemoryStackState<'_, '_, B> {
	fn gas_price(&self) -> U256 {
		self.backend.gas_price()
	}
	fn origin(&self) -> H160 {
		self.backend.origin()
	}
	fn block_hash(&self, number: U256) -> H256 {
		self.backend.block_hash(number)
	}
	fn block_number(&self) -> U256 {
		self.backend.block_number()
	}
	fn block_coinbase(&self) -> H160 {
		self.backend.block_coinbase()
	}
	fn block_timestamp(&self) -> U256 {
		self.backend.block_timestamp()
	}
	fn block_difficulty(&self) -> U256 {
		self.backend.block_difficulty()
	}
	fn block_randomness(&self) -> Option<H256> {
		self.backend.block_randomness()
	}
	fn block_gas_limit(&self) -> U256 {
		self.backend.block_gas_limit()
	}
	fn block_base_fee_per_gas(&self) -> U256 {
		self.backend.block_base_fee_per_gas()
	}

	fn chain_id(&self) -> U256 {
		self.backend.chain_id()
	}

	fn exists(&self, address: H160) -> bool {
		self.substate.known_account(address).is_some() || self.backend.exists(address)
	}

	fn basic(&self, address: H160) -> Basic {
		self.substate
			.known_basic(address)
			.unwrap_or_else(|| self.backend.basic(address))
	}

	fn code(&self, address: H160) -> Vec<u8> {
		self.substate
			.known_code(address)
			.unwrap_or_else(|| self.backend.code(address))
	}

	fn storage(&self, address: H160, key: H256) -> H256 {
		self.substate
			.known_storage(address, key)
			.unwrap_or_else(|| self.backend.storage(address, key))
	}

	fn is_empty_storage(&self, address: H160) -> bool {
		self.backend.is_empty_storage(address)
	}

	fn original_storage(&self, address: H160, key: H256) -> Option<H256> {
		if let Some(value) = self.substate.known_original_storage(address) {
			return Some(value);
		}

		self.backend.original_storage(address, key)
	}
	fn blob_gas_price(&self) -> Option<u128> {
		self.backend.blob_gas_price()
	}
	fn get_blob_hash(&self, index: usize) -> Option<U256> {
		self.backend.get_blob_hash(index)
	}
}

impl<'config, B: Backend> StackState<'config> for MemoryStackState<'_, 'config, B> {
	fn metadata(&self) -> &StackSubstateMetadata<'config> {
		self.substate.metadata()
	}

	fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		self.substate.metadata_mut()
	}

	fn enter(&mut self, gas_limit: u64, is_static: bool) {
		self.substate.enter(gas_limit, is_static);
	}

	fn exit_commit(&mut self) -> Result<(), ExitError> {
		self.substate.exit_commit()
	}

	fn exit_revert(&mut self) -> Result<(), ExitError> {
		self.substate.exit_revert()
	}

	fn exit_discard(&mut self) -> Result<(), ExitError> {
		self.substate.exit_discard()
	}

	fn is_empty(&self, address: H160) -> bool {
		if let Some(known_empty) = self.substate.known_empty(address) {
			return known_empty;
		}

		self.backend.basic(address).balance == U256::zero()
			&& self.backend.basic(address).nonce == U256::zero()
			&& self.backend.code(address).is_empty()
	}

	fn deleted(&self, address: H160) -> bool {
		self.substate.deleted(address)
	}

	fn is_cold(&self, address: H160) -> bool {
		self.substate.is_cold(address)
	}

	fn is_storage_cold(&self, address: H160, key: H256) -> bool {
		self.substate.is_storage_cold(address, key)
	}

	fn inc_nonce(&mut self, address: H160) -> Result<(), ExitError> {
		self.substate.inc_nonce(address, self.backend)
	}

	fn set_storage(&mut self, address: H160, key: H256, value: H256) {
		self.substate.set_storage(address, key, value);
	}

	fn reset_storage(&mut self, address: H160) {
		self.substate.reset_storage(address, self.backend);
	}

	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) {
		self.substate.log(address, topics, data);
	}

	fn set_deleted(&mut self, address: H160) {
		self.substate.set_deleted(address);
	}

	fn set_created(&mut self, address: H160) {
		self.substate.set_created(address);
	}

	fn is_created(&self, address: H160) -> bool {
		self.substate.is_created(address)
	}

	fn set_code(&mut self, address: H160, code: Vec<u8>) {
		self.substate.set_code(address, code, self.backend);
	}

	fn transfer(&mut self, transfer: Transfer) -> Result<(), ExitError> {
		self.substate.transfer(&transfer, self.backend)
	}

	fn reset_balance(&mut self, address: H160) {
		self.substate.reset_balance(address, self.backend);
	}

	fn touch(&mut self, address: H160) {
		self.substate.touch(address, self.backend);
	}

	fn tload(&mut self, address: H160, index: H256) -> Result<U256, ExitError> {
		Ok(self.substate.get_tstorage(address, index))
	}

	fn tstore(&mut self, address: H160, index: H256, value: U256) -> Result<(), ExitError> {
		self.substate.set_tstorage(address, index, value);
		Ok(())
	}

	/// EIP-7702 - check is authority cold.
	fn is_authority_cold(&mut self, address: H160) -> Option<bool> {
		self.get_authority_target(address)
			.map(|target| self.is_cold(target))
	}

	/// Get authority target (EIP-7702) - delegated address.
	/// First we're trying to get authority target from the cache recursively with parent state,
	/// if it's not found we get code for the authority address and check if it's delegation
	/// designator. If it's true, we add result to cache and return delegated target address.
	fn get_authority_target(&mut self, authority: H160) -> Option<H160> {
		// Read from cache
		if let Some(target_address) = self.substate.get_authority_target_recursive(authority) {
			Some(target_address)
		} else {
			// If not found in the cache
			// Get code for delegated address
			let authority_code = self.code(authority);
			if let Some(target) = Authorization::get_delegated_address(&authority_code) {
				// Add to cache
				self.metadata_mut().add_authority(authority, target);
				return Some(target);
			}
			None
		}
	}
}

impl<'backend, 'config, B: Backend> MemoryStackState<'backend, 'config, B> {
	pub const fn new(metadata: StackSubstateMetadata<'config>, backend: &'backend B) -> Self {
		Self {
			backend,
			substate: MemoryStackSubstate::new(metadata),
		}
	}

	/// Returns a mutable reference to an account given its address
	pub fn account_mut(&mut self, address: H160) -> &mut MemoryStackAccount {
		self.substate.account_mut(address, self.backend)
	}

	#[must_use]
	pub fn deconstruct(
		self,
	) -> (
		impl IntoIterator<Item = Apply<impl IntoIterator<Item = (H256, H256)>>>,
		impl IntoIterator<Item = Log>,
	) {
		self.substate.deconstruct(self.backend)
	}

	/// # Errors
	/// Return `ExitError`
	pub fn withdraw(&mut self, address: H160, value: U256) -> Result<(), ExitError> {
		self.substate.withdraw(address, value, self.backend)
	}

	pub fn deposit(&mut self, address: H160, value: U256) {
		self.substate.deposit(address, value, self.backend);
	}
}
