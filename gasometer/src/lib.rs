//! VM gasometer.

#![deny(warnings)]
#![forbid(unsafe_code, unused_variables)]
#![deny(clippy::pedantic, clippy::nursery)]
#![deny(clippy::as_conversions)]
#![allow(clippy::module_name_repetitions)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
pub mod prelude {
	pub use alloc::vec::Vec;
}

#[cfg(feature = "std")]
pub mod prelude {
	pub use std::vec::Vec;
}

#[cfg(feature = "tracing")]
pub mod tracing;

#[cfg(feature = "tracing")]
macro_rules! event {
	($x:expr) => {
		use crate::tracing::Event::*;
		crate::tracing::with(|listener| listener.event($x));
	};
}
#[cfg(feature = "force-debug")]
macro_rules! log_gas {
	($self:expr, $($arg:tt)*) => (
	log::trace!(target: "evm", "Gasometer {} [Gas used: {}, Gas left: {}]", format_args!($($arg)*),
	$self.total_used_gas(), $self.gas());
	#[cfg(feature = "print-debug")]
	println!("\t# {} [{} | {}]", format_args!($($arg)*), $self.total_used_gas(), $self.gas());
	);
}

#[cfg(not(feature = "force-debug"))]
macro_rules! log_gas {
	($self:expr, $($arg:tt)*) => {};
}

#[cfg(not(feature = "tracing"))]
macro_rules! event {
	($x:expr) => {};
}

mod consts;
mod costs;
mod memory;
mod utils;

use crate::prelude::*;
use core::cmp::max;
use evm_core::{ExitError, Opcode, Stack};
use evm_runtime::{Config, Handler};
use primitive_types::{H160, H256, U256};

macro_rules! try_or_fail {
	( $inner:expr, $e:expr ) => {
		match $e {
			Ok(value) => value,
			Err(e) => {
				$inner = Err(e.clone());
				return Err(e);
			}
		}
	};
}

#[cfg(feature = "tracing")]
#[derive(Debug, Copy, Clone)]
pub struct Snapshot {
	pub gas_limit: u64,
	pub memory_gas: u64,
	pub used_gas: u64,
	pub refunded_gas: i64,
}

#[cfg(feature = "tracing")]
impl Snapshot {
	#[must_use]
	const fn new<'config>(gas_limit: u64, inner: &'config Inner<'config>) -> Self {
		Self {
			gas_limit,
			memory_gas: inner.memory_gas,
			used_gas: inner.used_gas,
			refunded_gas: inner.refunded_gas,
		}
	}
}

/// EVM gasometer.
#[derive(Clone, Debug)]
pub struct Gasometer<'config> {
	gas_limit: u64,
	config: &'config Config,
	inner: Result<Inner<'config>, ExitError>,
}

impl<'config> Gasometer<'config> {
	/// Create a new gasometer with given gas limit and config.
	#[must_use]
	pub const fn new(gas_limit: u64, config: &'config Config) -> Self {
		Self {
			gas_limit,
			config,
			inner: Ok(Inner {
				memory_gas: 0,
				used_gas: 0,
				refunded_gas: 0,
				floor_gas: 0,
				config,
			}),
		}
	}

	/// Returns the numerical gas cost value.
	///
	/// # Errors
	/// Return `ExitError`
	#[inline]
	pub fn gas_cost(&self, cost: GasCost, gas: u64) -> Result<u64, ExitError> {
		match self.inner.as_ref() {
			Ok(inner) => inner.gas_cost(cost, gas),
			Err(e) => Err(e.clone()),
		}
	}

	#[inline]
	fn inner_mut(&mut self) -> Result<&mut Inner<'config>, ExitError> {
		self.inner.as_mut().map_err(|e| e.clone())
	}

	/// Reference of the config.
	#[inline]
	#[must_use]
	pub const fn config(&self) -> &'config Config {
		self.config
	}

	/// Gas limit.
	#[inline]
	#[must_use]
	pub const fn gas_limit(&self) -> u64 {
		self.gas_limit
	}

	/// Gas limit.
	#[inline]
	#[must_use]
	pub fn floor_gas(&self) -> u64 {
		self.inner.as_ref().map_or(0, |inner| inner.floor_gas)
	}

	/// Remaining gas.
	#[inline]
	#[must_use]
	pub fn gas(&self) -> u64 {
		self.inner.as_ref().map_or(0, |inner| {
			self.gas_limit - inner.used_gas - inner.memory_gas
		})
	}

	/// Total used gas.
	#[inline]
	#[must_use]
	pub const fn total_used_gas(&self) -> u64 {
		match self.inner.as_ref() {
			Ok(inner) => inner.used_gas + inner.memory_gas,
			Err(_) => self.gas_limit,
		}
	}

	/// Refunded gas.
	#[inline]
	#[must_use]
	pub fn refunded_gas(&self) -> i64 {
		self.inner.as_ref().map_or(0, |inner| inner.refunded_gas)
	}

	/// Explicitly fail the gasometer with out of gas. Return `OutOfGas` error.
	pub fn fail(&mut self) -> ExitError {
		self.inner = Err(ExitError::OutOfGas);
		ExitError::OutOfGas
	}

	/// Record an explicit cost.
	///
	/// # Errors
	/// Return `ExitError`
	#[inline]
	pub fn record_cost(&mut self, cost: u64) -> Result<(), ExitError> {
		event!(RecordCost {
			cost,
			snapshot: self.snapshot(),
		});

		let all_gas_cost = self.total_used_gas() + cost;
		if self.gas_limit < all_gas_cost {
			self.inner = Err(ExitError::OutOfGas);
			return Err(ExitError::OutOfGas);
		}

		self.inner_mut()?.used_gas += cost;
		log_gas!(self, "record_cost: {}", cost);
		Ok(())
	}

	#[inline]
	/// Record an explicit refund.
	///
	/// # Errors
	/// Return `ExitError` that is thrown by gasometer gas calculation errors.
	pub fn record_refund(&mut self, refund: i64) -> Result<(), ExitError> {
		event!(RecordRefund {
			refund,
			snapshot: self.snapshot(),
		});
		log_gas!(self, "record_refund: -{}", refund);

		self.inner_mut()?.refunded_gas += refund;
		Ok(())
	}

	/// Record refund for `authority` - EIP-7702
	/// `refunded_accounts` represent count of valid `authority`  accounts.
	///
	/// ## Errors
	/// Return `ExitError` if `record_refund` operation fails.
	pub fn record_authority_refund(&mut self, refunded_accounts: u64) -> Result<(), ExitError> {
		let refund = i64::try_from(
			refunded_accounts
				* (self.config.gas_per_empty_account_cost - self.config.gas_per_auth_base_cost),
		)
		.unwrap_or(i64::MAX);
		self.record_refund(refund)
	}

	/// Record `CREATE` code deposit.
	///
	/// # Errors
	/// Return `ExitError`
	/// NOTE: in that context usize->u64 `as_conversions` is save
	#[allow(clippy::as_conversions)]
	#[inline]
	pub fn record_deposit(&mut self, len: usize) -> Result<(), ExitError> {
		let cost = len as u64 * u64::from(consts::G_CODEDEPOSIT);
		self.record_cost(cost)
	}

	/// Record opcode gas cost.
	///
	/// # Errors
	/// Return `ExitError`
	pub fn record_dynamic_cost(
		&mut self,
		cost: GasCost,
		memory: Option<MemoryCost>,
	) -> Result<(), ExitError> {
		let gas = self.gas();
		// Extract a mutable reference to `Inner` to avoid checking `Result`
		// repeatedly. Tuning performance as this function is on the hot path.
		let inner_mut = match &mut self.inner {
			Ok(inner) => inner,
			Err(err) => return Err(err.clone()),
		};

		let memory_gas = match memory {
			Some(memory) => try_or_fail!(self.inner, inner_mut.memory_gas(memory)),
			None => inner_mut.memory_gas,
		};
		let gas_cost = try_or_fail!(self.inner, inner_mut.gas_cost(cost, gas));
		let gas_refund = inner_mut.gas_refund(cost);
		let used_gas = inner_mut.used_gas;

		#[cfg(feature = "tracing")]
		let gas_limit = self.gas_limit;
		event!(RecordDynamicCost {
			gas_cost,
			memory_gas,
			gas_refund,
			snapshot: Some(Snapshot::new(gas_limit, inner_mut)),
		});

		let all_gas_cost = memory_gas
			.checked_add(used_gas.saturating_add(gas_cost))
			.ok_or(ExitError::OutOfGas)?;
		if self.gas_limit < all_gas_cost {
			self.inner = Err(ExitError::OutOfGas);
			return Err(ExitError::OutOfGas);
		}

		let after_gas = self.gas_limit - all_gas_cost;
		try_or_fail!(self.inner, inner_mut.extra_check(cost, after_gas));

		inner_mut.used_gas += gas_cost;
		inner_mut.memory_gas = memory_gas;
		inner_mut.refunded_gas += gas_refund;

		// NOTE Extended meesage: "Record dynamic cost {gas_cost} - memory_gas {} - gas_refund {}",
		log_gas!(
			self,
			"record_dynamic_cost: {gas_cost} - {memory_gas} - {gas_refund}"
		);

		Ok(())
	}

	/// Record opcode stipend.
	///
	/// # Errors
	/// Return `ExitError` that is thrown by gasometer gas calculation errors.
	#[inline]
	pub fn record_stipend(&mut self, stipend: u64) -> Result<(), ExitError> {
		event!(RecordStipend {
			stipend,
			snapshot: self.snapshot(),
		});

		self.inner_mut()?.used_gas -= stipend;
		log_gas!(self, "record_stipent: {}", stipend);
		Ok(())
	}

	/// Record transaction cost.
	/// Related EIPs:
	/// - [EIP-2028](https://eips.ethereum.org/EIPS/eip-2028)
	/// - [EIP-7623](https://eips.ethereum.org/EIPS/eip-7623)
	///
	/// # Errors
	/// Return `ExitError`
	pub fn record_transaction(&mut self, cost: TransactionCost) -> Result<(), ExitError> {
		let gas_cost = match cost {
			// NOTE: in that context usize->u64 `as_conversions` is safe
			#[allow(clippy::as_conversions)]
			TransactionCost::Call {
				zero_data_len,
				non_zero_data_len,
				access_list_address_len,
				access_list_storage_len,
				authorization_list_len,
			} => {
				#[deny(clippy::let_and_return)]
				let cost = self.config.gas_transaction_call
					+ zero_data_len as u64 * self.config.gas_transaction_zero_data
					+ non_zero_data_len as u64 * self.config.gas_transaction_non_zero_data
					+ access_list_address_len as u64 * self.config.gas_access_list_address
					+ access_list_storage_len as u64 * self.config.gas_access_list_storage_key
					+ authorization_list_len as u64 * self.config.gas_per_empty_account_cost;

				if self.config.has_floor_gas {
					// According to EIP-2028: non-zero byte = 16, zero-byte = 4
					// According to EIP-7623: tokens_in_calldata = zero_bytes_in_calldata + nonzero_bytes_in_calldata * 4
					let tokens_in_calldata = (zero_data_len + non_zero_data_len * 4) as u64;
					self.inner_mut()?.floor_gas = tokens_in_calldata
						* self.config.total_cost_floor_per_token
						+ self.config.gas_transaction_call;
				}

				log_gas!(
					self,
					"Record Call {} [gas_transaction_call: {}, zero_data_len: {}, non_zero_data_len: {}, access_list_address_len: {}, access_list_storage_len: {}, authorization_list_len: {}]",
					cost,
					self.config.gas_transaction_call,
					zero_data_len,
					non_zero_data_len,
					access_list_address_len,
					access_list_storage_len,
					authorization_list_len
				);

				cost
			}
			// NOTE: in that context usize->u64 `as_conversions` is safe
			#[allow(clippy::as_conversions)]
			TransactionCost::Create {
				zero_data_len,
				non_zero_data_len,
				access_list_address_len,
				access_list_storage_len,
				initcode_cost,
			} => {
				let mut cost = self.config.gas_transaction_create
					+ zero_data_len as u64 * self.config.gas_transaction_zero_data
					+ non_zero_data_len as u64 * self.config.gas_transaction_non_zero_data
					+ access_list_address_len as u64 * self.config.gas_access_list_address
					+ access_list_storage_len as u64 * self.config.gas_access_list_storage_key;
				if self.config.max_initcode_size.is_some() {
					cost += initcode_cost;
				}

				log_gas!(
					self,
					"Record Create {} [gas_transaction_create: {}, zero_data_len: {}, non_zero_data_len: {}, access_list_address_len: {}, access_list_storage_len: {}, initcode_cost: {}]",
					cost,
					self.config.gas_transaction_create,
					zero_data_len,
					non_zero_data_len,
					access_list_address_len,
					access_list_storage_len,
					initcode_cost
				);
				cost
			}
		};

		event!(RecordTransaction {
			cost: gas_cost,
			snapshot: self.snapshot(),
		});

		if self.gas() < gas_cost {
			self.inner = Err(ExitError::OutOfGas);
			return Err(ExitError::OutOfGas);
		}

		self.inner_mut()?.used_gas += gas_cost;
		Ok(())
	}

	#[cfg(feature = "tracing")]
	#[must_use]
	pub fn snapshot(&self) -> Option<Snapshot> {
		self.inner
			.as_ref()
			.ok()
			.map(|inner| Snapshot::new(self.gas_limit, inner))
	}
}

/// Calculate the call transaction cost.
#[allow(clippy::naive_bytecount)]
#[must_use]
pub fn call_transaction_cost(
	data: &[u8],
	access_list: &[(H160, Vec<H256>)],
	authorization_list_len: usize,
) -> TransactionCost {
	let zero_data_len = data.iter().filter(|v| **v == 0).count();
	let non_zero_data_len = data.len() - zero_data_len;
	let (access_list_address_len, access_list_storage_len) = count_access_list(access_list);

	TransactionCost::Call {
		zero_data_len,
		non_zero_data_len,
		access_list_address_len,
		access_list_storage_len,
		authorization_list_len,
	}
}

/// Calculate the create transaction cost.
#[allow(clippy::naive_bytecount)]
#[must_use]
pub fn create_transaction_cost(data: &[u8], access_list: &[(H160, Vec<H256>)]) -> TransactionCost {
	let zero_data_len = data.iter().filter(|v| **v == 0).count();
	let non_zero_data_len = data.len() - zero_data_len;
	let (access_list_address_len, access_list_storage_len) = count_access_list(access_list);
	let initcode_cost = init_code_cost(data);

	TransactionCost::Create {
		zero_data_len,
		non_zero_data_len,
		access_list_address_len,
		access_list_storage_len,
		initcode_cost,
	}
}

/// Init code cost, related to `EIP-3860`
/// NOTE: in that context `as-conversion` is safe for `usize->u64`
#[allow(clippy::as_conversions)]
#[must_use]
pub const fn init_code_cost(data: &[u8]) -> u64 {
	// As per EIP-3860:
	// > We define initcode_cost(initcode) to equal INITCODE_WORD_COST * ceil(len(initcode) / 32).
	// where INITCODE_WORD_COST is 2.
	2 * ((data.len() as u64 + 31) / 32)
}

/// Counts the number of addresses and storage keys in the access list
fn count_access_list(access_list: &[(H160, Vec<H256>)]) -> (usize, usize) {
	let access_list_address_len = access_list.len();
	let access_list_storage_len = access_list.iter().map(|(_, keys)| keys.len()).sum();

	(access_list_address_len, access_list_storage_len)
}

#[allow(clippy::too_many_lines)]
#[inline]
#[must_use]
pub fn static_opcode_cost(opcode: Opcode) -> Option<u32> {
	static TABLE: [Option<u32>; 256] = {
		let mut table = [None; 256];

		table[Opcode::STOP.as_usize()] = Some(consts::G_ZERO);
		table[Opcode::CALLDATASIZE.as_usize()] = Some(consts::G_BASE);
		table[Opcode::CODESIZE.as_usize()] = Some(consts::G_BASE);
		table[Opcode::POP.as_usize()] = Some(consts::G_BASE);
		table[Opcode::PC.as_usize()] = Some(consts::G_BASE);
		table[Opcode::MSIZE.as_usize()] = Some(consts::G_BASE);

		table[Opcode::ADDRESS.as_usize()] = Some(consts::G_BASE);
		table[Opcode::ORIGIN.as_usize()] = Some(consts::G_BASE);
		table[Opcode::CALLER.as_usize()] = Some(consts::G_BASE);
		table[Opcode::CALLVALUE.as_usize()] = Some(consts::G_BASE);
		table[Opcode::COINBASE.as_usize()] = Some(consts::G_BASE);
		table[Opcode::TIMESTAMP.as_usize()] = Some(consts::G_BASE);
		table[Opcode::NUMBER.as_usize()] = Some(consts::G_BASE);
		table[Opcode::PREVRANDAO.as_usize()] = Some(consts::G_BASE);
		table[Opcode::GASLIMIT.as_usize()] = Some(consts::G_BASE);
		table[Opcode::GASPRICE.as_usize()] = Some(consts::G_BASE);
		table[Opcode::GAS.as_usize()] = Some(consts::G_BASE);

		table[Opcode::ADD.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SUB.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::NOT.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::LT.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::GT.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SLT.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SGT.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::EQ.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::ISZERO.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::AND.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::OR.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::XOR.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::BYTE.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::CALLDATALOAD.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH1.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH2.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH3.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH4.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH5.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH6.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH7.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH8.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH9.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH10.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH11.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH12.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH13.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH14.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH15.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH16.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH17.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH18.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH19.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH20.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH21.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH22.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH23.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH24.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH25.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH26.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH27.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH28.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH29.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH30.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH31.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::PUSH32.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP1.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP2.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP3.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP4.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP5.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP6.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP7.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP8.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP9.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP10.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP11.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP12.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP13.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP14.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP15.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::DUP16.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP1.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP2.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP3.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP4.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP5.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP6.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP7.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP8.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP9.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP10.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP11.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP12.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP13.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP14.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP15.as_usize()] = Some(consts::G_VERYLOW);
		table[Opcode::SWAP16.as_usize()] = Some(consts::G_VERYLOW);

		table[Opcode::MUL.as_usize()] = Some(consts::G_LOW);
		table[Opcode::DIV.as_usize()] = Some(consts::G_LOW);
		table[Opcode::SDIV.as_usize()] = Some(consts::G_LOW);
		table[Opcode::MOD.as_usize()] = Some(consts::G_LOW);
		table[Opcode::SMOD.as_usize()] = Some(consts::G_LOW);
		table[Opcode::SIGNEXTEND.as_usize()] = Some(consts::G_LOW);

		table[Opcode::ADDMOD.as_usize()] = Some(consts::G_MID);
		table[Opcode::MULMOD.as_usize()] = Some(consts::G_MID);
		table[Opcode::JUMP.as_usize()] = Some(consts::G_MID);

		table[Opcode::JUMPI.as_usize()] = Some(consts::G_HIGH);
		table[Opcode::JUMPDEST.as_usize()] = Some(consts::G_JUMPDEST);

		table
	};

	TABLE[opcode.as_usize()]
}

/// Get and set warm address if it's not warmed.
fn get_and_set_warm<H: Handler>(handler: &mut H, target: H160) -> (bool, Option<bool>) {
	let delegated_designator_is_cold =
		handler
			.get_authority_target(target)
			.map(|authority_target| {
				if handler.is_cold(authority_target, None) {
					handler.warm_target((authority_target, None));
					true
				} else {
					false
				}
			});
	let target_is_cold = handler.is_cold(target, None);
	if target_is_cold {
		handler.warm_target((target, None));
	}
	(target_is_cold, delegated_designator_is_cold)
}

/// Get and set warm address if it's not warmed for non-delegated opcodes like `EXT*`.
/// NOTE: Related to EIP-7702
fn get_and_set_non_delegated_warm<H: Handler>(handler: &mut H, target: H160) -> bool {
	let target_is_cold = handler.is_cold(target, None);
	if target_is_cold {
		handler.warm_target((target, None));
	}
	target_is_cold
}

/// Calculate the opcode cost.
///
/// # Errors
/// Return `ExitError`
#[allow(
	clippy::nonminimal_bool,
	clippy::cognitive_complexity,
	clippy::too_many_lines,
	clippy::match_same_arms
)]
pub fn dynamic_opcode_cost<H: Handler>(
	address: H160,
	opcode: Opcode,
	stack: &Stack,
	is_static: bool,
	config: &Config,
	handler: &mut H,
) -> Result<(GasCost, Option<MemoryCost>), ExitError> {
	let gas_cost = match opcode {
		Opcode::RETURN => GasCost::Zero,

		Opcode::MLOAD | Opcode::MSTORE | Opcode::MSTORE8 => GasCost::VeryLow,

		Opcode::REVERT if config.has_revert => GasCost::Zero,
		Opcode::REVERT => GasCost::Invalid(opcode),

		Opcode::CHAINID if config.has_chain_id => GasCost::Base,
		Opcode::CHAINID => GasCost::Invalid(opcode),

		Opcode::SHL | Opcode::SHR | Opcode::SAR if config.has_bitwise_shifting => GasCost::VeryLow,
		Opcode::SHL | Opcode::SHR | Opcode::SAR => GasCost::Invalid(opcode),

		Opcode::SELFBALANCE if config.has_self_balance => GasCost::Low,
		Opcode::SELFBALANCE => GasCost::Invalid(opcode),

		Opcode::BASEFEE if config.has_base_fee => GasCost::Base,
		Opcode::BASEFEE => GasCost::Invalid(opcode),

		Opcode::BLOBBASEFEE if config.has_blob_base_fee => GasCost::Base,
		Opcode::BLOBBASEFEE => GasCost::Invalid(opcode),

		Opcode::BLOBHASH if config.has_shard_blob_transactions => GasCost::VeryLow,
		Opcode::BLOBHASH => GasCost::Invalid(opcode),

		Opcode::TLOAD if config.has_transient_storage => GasCost::WarmStorageRead,
		Opcode::TLOAD => GasCost::Invalid(opcode),

		Opcode::TSTORE if !is_static && config.has_transient_storage => GasCost::WarmStorageRead,
		Opcode::TSTORE => GasCost::Invalid(opcode),

		Opcode::MCOPY if config.has_mcopy => GasCost::VeryLowCopy {
			len: stack.peek(2)?,
		},
		Opcode::MCOPY => GasCost::Invalid(opcode),

		Opcode::EXTCODESIZE => {
			let target = stack.peek_h256(0)?.into();
			let target_is_cold = get_and_set_non_delegated_warm(handler, target);
			GasCost::ExtCodeSize { target_is_cold }
		}
		Opcode::BALANCE => {
			let target = stack.peek_h256(0)?.into();
			let target_is_cold = handler.is_cold(target, None);
			if target_is_cold {
				handler.warm_target((target, None));
			}
			GasCost::Balance { target_is_cold }
		}
		Opcode::BLOCKHASH => GasCost::BlockHash,

		Opcode::EXTCODEHASH if config.has_ext_code_hash => {
			let target = stack.peek_h256(0)?.into();
			let target_is_cold = get_and_set_non_delegated_warm(handler, target);
			GasCost::ExtCodeHash { target_is_cold }
		}
		Opcode::EXTCODEHASH => GasCost::Invalid(opcode),

		Opcode::CALLCODE => {
			let target = stack.peek_h256(1)?.into();
			let (target_is_cold, delegated_designator_is_cold) = get_and_set_warm(handler, target);
			GasCost::CallCode {
				value: stack.peek(2)?,
				gas: stack.peek(0)?,
				target_is_cold,
				delegated_designator_is_cold,
				target_exists: {
					handler.record_external_operation(evm_core::ExternalOperation::IsEmpty)?;
					handler.exists(target)
				},
			}
		}
		Opcode::STATICCALL => {
			let target = stack.peek_h256(1)?.into();
			let (target_is_cold, delegated_designator_is_cold) = get_and_set_warm(handler, target);
			GasCost::StaticCall {
				gas: stack.peek(0)?,
				target_is_cold,
				delegated_designator_is_cold,
				target_exists: {
					handler.record_external_operation(evm_core::ExternalOperation::IsEmpty)?;
					handler.exists(target)
				},
			}
		}
		Opcode::SHA3 => GasCost::Sha3 {
			len: stack.peek(1)?,
		},
		Opcode::EXTCODECOPY => {
			let target = stack.peek_h256(0)?.into();
			let target_is_cold = get_and_set_non_delegated_warm(handler, target);
			GasCost::ExtCodeCopy {
				target_is_cold,
				len: stack.peek(3)?,
			}
		}
		Opcode::CALLDATACOPY | Opcode::CODECOPY => GasCost::VeryLowCopy {
			len: stack.peek(2)?,
		},
		Opcode::EXP => GasCost::Exp {
			power: stack.peek(1)?,
		},
		Opcode::SLOAD => {
			let index = stack.peek_h256(0)?;
			let target_is_cold = handler.is_cold(address, Some(index));
			if target_is_cold {
				handler.warm_target((address, Some(index)));
			}
			GasCost::SLoad { target_is_cold }
		}

		Opcode::DELEGATECALL if config.has_delegate_call => {
			let target = stack.peek_h256(1)?.into();
			let (target_is_cold, delegated_designator_is_cold) = get_and_set_warm(handler, target);
			GasCost::DelegateCall {
				gas: stack.peek(0)?,
				target_is_cold,
				delegated_designator_is_cold,
				target_exists: {
					handler.record_external_operation(evm_core::ExternalOperation::IsEmpty)?;
					handler.exists(target)
				},
			}
		}
		Opcode::DELEGATECALL => GasCost::Invalid(opcode),

		Opcode::RETURNDATASIZE if config.has_return_data => GasCost::Base,
		Opcode::RETURNDATACOPY if config.has_return_data => GasCost::VeryLowCopy {
			len: stack.peek(2)?,
		},
		Opcode::RETURNDATASIZE | Opcode::RETURNDATACOPY => GasCost::Invalid(opcode),

		Opcode::SSTORE if !is_static => {
			let index = stack.peek_h256(0)?;
			let value = stack.peek_h256(1)?;
			let target_is_cold = handler.is_cold(address, Some(index));
			if target_is_cold {
				handler.warm_target((address, Some(index)));
			}
			GasCost::SStore {
				original: handler.original_storage(address, index),
				current: handler.storage(address, index),
				new: value,
				target_is_cold,
			}
		}
		Opcode::LOG0 if !is_static => GasCost::Log {
			n: 0,
			len: stack.peek(1)?,
		},
		Opcode::LOG1 if !is_static => GasCost::Log {
			n: 1,
			len: stack.peek(1)?,
		},
		Opcode::LOG2 if !is_static => GasCost::Log {
			n: 2,
			len: stack.peek(1)?,
		},
		Opcode::LOG3 if !is_static => GasCost::Log {
			n: 3,
			len: stack.peek(1)?,
		},
		Opcode::LOG4 if !is_static => GasCost::Log {
			n: 4,
			len: stack.peek(1)?,
		},
		Opcode::CREATE if !is_static => GasCost::Create,
		Opcode::CREATE2 if !is_static && config.has_create2 => GasCost::Create2 {
			len: stack.peek(2)?,
		},
		Opcode::SELFDESTRUCT if !is_static => {
			let target = stack.peek_h256(0)?.into();
			let target_is_cold = handler.is_cold(target, None);
			if target_is_cold {
				handler.warm_target((target, None));
			}
			GasCost::Suicide {
				value: handler.balance(address),
				target_is_cold,
				target_exists: {
					handler.record_external_operation(evm_core::ExternalOperation::IsEmpty)?;
					handler.exists(target)
				},
				already_removed: handler.deleted(address),
			}
		}
		Opcode::CALL if !is_static || (is_static && stack.peek(2)? == U256::zero()) => {
			let target = stack.peek_h256(1)?.into();
			let (target_is_cold, delegated_designator_is_cold) = get_and_set_warm(handler, target);
			GasCost::Call {
				value: stack.peek(2)?,
				gas: stack.peek(0)?,
				target_is_cold,
				delegated_designator_is_cold,
				target_exists: {
					handler.record_external_operation(evm_core::ExternalOperation::IsEmpty)?;
					handler.exists(target)
				},
			}
		}

		Opcode::PUSH0 if config.has_push0 => GasCost::Base,

		_ => GasCost::Invalid(opcode),
	};

	let memory_cost = match opcode {
		Opcode::SHA3
		| Opcode::RETURN
		| Opcode::REVERT
		| Opcode::LOG0
		| Opcode::LOG1
		| Opcode::LOG2
		| Opcode::LOG3
		| Opcode::LOG4 => Some(peek_memory_cost(stack, 0, 1)?),

		Opcode::CODECOPY | Opcode::CALLDATACOPY | Opcode::RETURNDATACOPY => {
			Some(peek_memory_cost(stack, 0, 2)?)
		}

		Opcode::EXTCODECOPY => Some(peek_memory_cost(stack, 1, 3)?),

		Opcode::MLOAD | Opcode::MSTORE => Some(MemoryCost {
			offset: stack.peek_usize(0)?,
			len: 32,
		}),

		Opcode::MCOPY => {
			let len = stack.peek_usize(2)?;
			if len == 0 {
				None
			} else {
				Some(MemoryCost {
					offset: {
						let src = stack.peek_usize(0)?;
						let dst = stack.peek_usize(1)?;
						max(src, dst)
					},
					len,
				})
			}
		}

		Opcode::MSTORE8 => Some(MemoryCost {
			offset: stack.peek_usize(0)?,
			len: 1,
		}),

		Opcode::CREATE | Opcode::CREATE2 => Some(peek_memory_cost(stack, 1, 2)?),

		Opcode::CALL | Opcode::CALLCODE => {
			Some(peek_memory_cost(stack, 3, 4)?.join(peek_memory_cost(stack, 5, 6)?))
		}

		Opcode::DELEGATECALL | Opcode::STATICCALL => {
			Some(peek_memory_cost(stack, 2, 3)?.join(peek_memory_cost(stack, 4, 5)?))
		}

		_ => None,
	};

	Ok((gas_cost, memory_cost))
}

fn peek_memory_cost(
	stack: &Stack,
	offset_index: usize,
	len_index: usize,
) -> Result<MemoryCost, ExitError> {
	let len = stack.peek_usize(len_index)?;

	if len == 0 {
		return Ok(MemoryCost {
			offset: usize::MAX,
			len,
		});
	}

	let offset = stack.peek_usize(offset_index)?;
	Ok(MemoryCost { offset, len })
}

/// Holds the gas consumption for a Gasometer instance.
#[derive(Clone, Debug)]
struct Inner<'config> {
	memory_gas: u64,
	used_gas: u64,
	refunded_gas: i64,
	config: &'config Config,
	floor_gas: u64,
}

impl Inner<'_> {
	fn memory_gas(&self, memory: MemoryCost) -> Result<u64, ExitError> {
		let from = memory.offset;
		let len = memory.len;

		if len == 0 {
			return Ok(self.memory_gas);
		}

		let end = from.checked_add(len).ok_or(ExitError::OutOfGas)?;

		let rem = end % 32;
		let new = if rem == 0 { end / 32 } else { end / 32 + 1 };

		Ok(max(self.memory_gas, memory::memory_gas(new)?))
	}

	fn extra_check(&self, cost: GasCost, after_gas: u64) -> Result<(), ExitError> {
		match cost {
			GasCost::Call { gas, .. }
			| GasCost::CallCode { gas, .. }
			| GasCost::DelegateCall { gas, .. }
			| GasCost::StaticCall { gas, .. } => costs::call_extra_check(gas, after_gas, self.config),
			_ => Ok(()),
		}
	}

	/// Returns the gas cost numerical value.
	#[allow(clippy::too_many_lines)]
	fn gas_cost(&self, cost: GasCost, gas: u64) -> Result<u64, ExitError> {
		Ok(match cost {
			GasCost::Call {
				value,
				target_is_cold,
				delegated_designator_is_cold,
				target_exists,
				..
			} => costs::call_cost(
				value,
				target_is_cold,
				delegated_designator_is_cold,
				true,
				true,
				!target_exists,
				self.config,
			),
			GasCost::CallCode {
				value,
				target_is_cold,
				delegated_designator_is_cold,
				target_exists,
				..
			} => costs::call_cost(
				value,
				target_is_cold,
				delegated_designator_is_cold,
				true,
				false,
				!target_exists,
				self.config,
			),
			GasCost::DelegateCall {
				target_is_cold,
				delegated_designator_is_cold,
				target_exists,
				..
			} => costs::call_cost(
				U256::zero(),
				target_is_cold,
				delegated_designator_is_cold,
				false,
				false,
				!target_exists,
				self.config,
			),
			GasCost::StaticCall {
				target_is_cold,
				delegated_designator_is_cold,
				target_exists,
				..
			} => costs::call_cost(
				U256::zero(),
				target_is_cold,
				delegated_designator_is_cold,
				false,
				true,
				!target_exists,
				self.config,
			),

			GasCost::Suicide {
				value,
				target_is_cold,
				target_exists,
				..
			} => costs::suicide_cost(value, target_is_cold, target_exists, self.config),
			GasCost::SStore {
				original,
				current,
				new,
				target_is_cold,
			} => costs::sstore_cost(original, current, new, gas, target_is_cold, self.config)?,

			GasCost::Sha3 { len } => costs::sha3_cost(len)?,
			GasCost::Log { n, len } => costs::log_cost(n, len)?,
			GasCost::VeryLowCopy { len } => costs::verylowcopy_cost(len)?,
			GasCost::Exp { power } => costs::exp_cost(power, self.config)?,
			GasCost::Create => u64::from(consts::G_CREATE),
			GasCost::Create2 { len } => costs::create2_cost(len)?,
			GasCost::SLoad { target_is_cold } => costs::sload_cost(target_is_cold, self.config),

			GasCost::Zero => u64::from(consts::G_ZERO),
			GasCost::Base => u64::from(consts::G_BASE),
			GasCost::VeryLow => u64::from(consts::G_VERYLOW),
			GasCost::Low => u64::from(consts::G_LOW),
			GasCost::Invalid(opcode) => return Err(ExitError::InvalidCode(opcode)),

			GasCost::ExtCodeSize { target_is_cold } => costs::address_access_cost(
				target_is_cold,
				None,
				self.config.gas_ext_code,
				self.config,
			),
			GasCost::ExtCodeCopy {
				target_is_cold,
				len,
			} => costs::extcodecopy_cost(len, target_is_cold, None, self.config)?,
			GasCost::Balance { target_is_cold } => costs::address_access_cost(
				target_is_cold,
				None,
				self.config.gas_balance,
				self.config,
			),
			GasCost::BlockHash => u64::from(consts::G_BLOCKHASH),
			GasCost::ExtCodeHash { target_is_cold } => costs::address_access_cost(
				target_is_cold,
				None,
				self.config.gas_ext_code_hash,
				self.config,
			),
			GasCost::WarmStorageRead => costs::storage_read_warm(self.config),
		})
	}

	fn gas_refund(&self, cost: GasCost) -> i64 {
		match cost {
			_ if self.config.estimate => 0,

			GasCost::SStore {
				original,
				current,
				new,
				..
			} => costs::sstore_refund(original, current, new, self.config),
			GasCost::Suicide {
				already_removed, ..
			} if !self.config.decrease_clears_refund => costs::suicide_refund(already_removed),
			_ => 0,
		}
	}
}

/// Gas cost.
#[derive(Debug, Clone, Copy)]
pub enum GasCost {
	/// Zero gas cost.
	Zero,
	/// Base gas cost.
	Base,
	/// Very low gas cost.
	VeryLow,
	/// Low gas cost.
	Low,
	/// Fail the gasometer.
	Invalid(Opcode),

	/// Gas cost for `EXTCODESIZE`.
	ExtCodeSize {
		/// True if address has not been previously accessed in this transaction
		target_is_cold: bool,
	},
	/// Gas cost for `BALANCE`.
	Balance {
		/// True if address has not been previously accessed in this transaction
		target_is_cold: bool,
	},
	/// Gas cost for `BLOCKHASH`.
	BlockHash,
	/// Gas cost for `EXTBLOCKHASH`.
	ExtCodeHash {
		/// True if address has not been previously accessed in this transaction
		target_is_cold: bool,
	},

	/// Gas cost for `CALL`.
	Call {
		/// Call value.
		value: U256,
		/// Call gas.
		gas: U256,
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
		/// True if delegated designator of authority has not been previously accessed in this transaction (EIP-7702)
		delegated_designator_is_cold: Option<bool>,
		/// Whether the target exists.
		target_exists: bool,
	},
	/// Gas cost for `CALLCODE`.
	CallCode {
		/// Call value.
		value: U256,
		/// Call gas.
		gas: U256,
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
		/// True if delegated designator of authority has not been previously accessed in this transaction (EIP-7702)
		delegated_designator_is_cold: Option<bool>,
		/// Whether the target exists.
		target_exists: bool,
	},
	/// Gas cost for `DELEGATECALL`.
	DelegateCall {
		/// Call gas.
		gas: U256,
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
		/// True if delegated designator of authority has not been previously accessed in this transaction (EIP-7702)
		delegated_designator_is_cold: Option<bool>,
		/// Whether the target exists.
		target_exists: bool,
	},
	/// Gas cost for `STATICCALL`.
	StaticCall {
		/// Call gas.
		gas: U256,
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
		/// True if delegated designator of authority has not been previously accessed in this transaction (EIP-7702)
		delegated_designator_is_cold: Option<bool>,
		/// Whether the target exists.
		target_exists: bool,
	},
	/// Gas cost for `SUICIDE`.
	Suicide {
		/// Value.
		value: U256,
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
		/// Whether the target exists.
		target_exists: bool,
		/// Whether the target has already been removed.
		already_removed: bool,
	},
	/// Gas cost for `SSTORE`.
	SStore {
		/// Original value.
		original: H256,
		/// Current value.
		current: H256,
		/// New value.
		new: H256,
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
	},
	/// Gas cost for `SHA3`.
	Sha3 {
		/// Length of the data.
		len: U256,
	},
	/// Gas cost for `LOG`.
	Log {
		/// Topic length.
		n: u8,
		/// Data length.
		len: U256,
	},
	/// Gas cost for `EXTCODECOPY`.
	ExtCodeCopy {
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
		/// Length.
		len: U256,
	},
	/// Gas cost for some copy opcodes that is documented as `VERYLOW`.
	VeryLowCopy {
		/// Length.
		len: U256,
	},
	/// Gas cost for `EXP`.
	Exp {
		/// Power of `EXP`.
		power: U256,
	},
	/// Gas cost for `CREATE`.
	Create,
	/// Gas cost for `CREATE2`.
	Create2 {
		/// Length.
		len: U256,
	},
	/// Gas cost for `SLOAD`.
	SLoad {
		/// True if target has not been previously accessed in this transaction
		target_is_cold: bool,
	},
	WarmStorageRead,
}

/// Storage opcode will access. Used for tracking accessed storage (EIP-2929).
#[derive(Debug, Clone, Copy)]
pub enum StorageTarget {
	/// No storage access
	None,
	/// Accessing address
	Address(H160),
	/// Accessing storage slot within an address
	Slot(H160, H256),
}

/// Memory cost.
#[derive(Debug, Clone, Copy)]
pub struct MemoryCost {
	/// Affected memory offset.
	pub offset: usize,
	/// Affected length.
	pub len: usize,
}

/// Transaction cost.
#[derive(Debug, Clone, Copy)]
pub enum TransactionCost {
	/// Call transaction cost.
	Call {
		/// Length of zeros in transaction data.
		zero_data_len: usize,
		/// Length of non-zeros in transaction data.
		non_zero_data_len: usize,
		/// Number of addresses in transaction access list (see EIP-2930)
		access_list_address_len: usize,
		/// Total number of storage keys in transaction access list (see EIP-2930)
		access_list_storage_len: usize,
		/// Number of authorities in transaction authorization list (see EIP-7702)
		authorization_list_len: usize,
	},
	/// Create transaction cost.
	Create {
		/// Length of zeros in transaction data.
		zero_data_len: usize,
		/// Length of non-zeros in transaction data.
		non_zero_data_len: usize,
		/// Number of addresses in transaction access list (see EIP-2930)
		access_list_address_len: usize,
		/// Total number of storage keys in transaction access list (see EIP-2930)
		access_list_storage_len: usize,
		/// Cost of initcode = 2 * ceil(len(initcode) / 32) (see EIP-3860)
		initcode_cost: u64,
	},
}

impl MemoryCost {
	/// Join two memory cost together.
	#[must_use]
	pub const fn join(self, other: Self) -> Self {
		if self.len == 0 {
			return other;
		}

		if other.len == 0 {
			return self;
		}

		let self_end = self.offset.saturating_add(self.len);
		let other_end = other.offset.saturating_add(other.len);

		if self_end >= other_end {
			self
		} else {
			other
		}
	}
}
