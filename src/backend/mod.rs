//! # EVM backends
//!
//! Backends store state information of the VM, and exposes it to runtime.
use crate::prelude::*;
use primitive_types::{H160, H256, U256};

pub use self::memory::{MemoryAccount, MemoryBackend, MemoryVicinity};

mod memory;

/// Basic account information.
///
#[derive(Clone, Eq, PartialEq, Debug, Default)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Basic {
	/// Account balance.
	pub balance: U256,
	/// Account nonce.
	pub nonce: U256,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, rlp::RlpEncodable, rlp::RlpDecodable)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Log {
	pub address: H160,
	pub topics: Vec<H256>,
	pub data: Vec<u8>,
}

/// Apply state operation.
#[derive(Clone, Debug)]
pub enum Apply<I> {
	/// Modify or create at address.
	Modify {
		/// Address.
		address: H160,
		/// Basic information of the address.
		basic: Basic,
		/// Code. `None` means leaving it unchanged.
		code: Option<Vec<u8>>,
		/// Storage iterator.
		storage: I,
		/// Whether storage should be wiped empty before applying the storage
		/// iterator.
		reset_storage: bool,
	},
	/// Delete address.
	Delete {
		/// Address.
		address: H160,
	},
}

/// EVM backend.
#[auto_impl::auto_impl(&, Arc, Box)]
pub trait Backend {
	/// Gas price. Unused for London.
	fn gas_price(&self) -> U256;
	/// Origin.
	fn origin(&self) -> H160;
	/// Environmental block hash.
	fn block_hash(&self, number: U256) -> H256;
	/// Environmental block number.
	fn block_number(&self) -> U256;
	/// Environmental coinbase.
	fn block_coinbase(&self) -> H160;
	/// Environmental block timestamp.
	fn block_timestamp(&self) -> U256;
	/// Environmental block difficulty.
	fn block_difficulty(&self) -> U256;
	/// Get environmental block randomness.
	fn block_randomness(&self) -> Option<H256>;
	/// Environmental block gas limit.
	fn block_gas_limit(&self) -> U256;
	/// Environmental block base fee.
	fn block_base_fee_per_gas(&self) -> U256;
	/// Environmental chain ID.
	fn chain_id(&self) -> U256;

	/// Whether account at address exists.
	fn exists(&self, address: H160) -> bool;
	/// Get basic account information.
	fn basic(&self, address: H160) -> Basic;
	/// Get account code.
	fn code(&self, address: H160) -> Vec<u8>;
	/// Get storage value of address at index.
	fn storage(&self, address: H160, index: H256) -> H256;
	/// Check if the storage of the address is empty.
	fn is_empty_storage(&self, address: H160) -> bool;
	/// Get original storage value of address at index, if available.
	fn original_storage(&self, address: H160, index: H256) -> Option<H256>;
	/// CANCUN hard fork
	/// [EIP-4844]: Shard Blob Transactions
	/// [EIP-7516]: BLOBBASEFEE instruction
	fn blob_gas_price(&self) -> Option<u128>;
	/// Get `blob_hash` from `blob_versioned_hashes` by index
	/// [EIP-4844]: BLOBHASH - https://eips.ethereum.org/EIPS/eip-4844#opcode-to-get-versioned-hashes
	fn get_blob_hash(&self, index: usize) -> Option<U256>;
}

/// EVM backend that can apply changes.
pub trait ApplyBackend {
	/// Apply given values and logs at backend.
	fn apply<A, I, L>(&mut self, values: A, logs: L, delete_empty: bool)
	where
		A: IntoIterator<Item = Apply<I>>,
		I: IntoIterator<Item = (H256, H256)>,
		L: IntoIterator<Item = Log>;
}
