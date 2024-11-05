//! # EIP-7480: EOF - Data section access instructions.
//! Instructions to read data section of EOF container.
//!
//! ## New Instructions
//! - `DATALOAD`
//! - `DATALOADN`
//! - `DATASIZE`
//! - `DATACOPY`
//!
//!
//! [EIP-7480](https://eips.ethereum.org/EIPS/eip-7480)

#![allow(clippy::module_name_repetitions, clippy::doc_lazy_continuation)]

use super::Control;
use crate::{eof, ExitError, Handler, Runtime};
use primitive_types::U256;

/// Loads a 32-byte value from the data section of the EOF container.
///
/// 1. Pops one value, `offset`, from the stack.
/// 2. Reads `[offset:offset+32]` segment from the data section and pushes it as 32-byte value to the stack.
/// 3. If `offset + 32` is greater than the data section size, bytes after the end of data section are set to 0.
pub fn data_load<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	pop_u256!(runtime, offset);

	let offset_usize = as_usize_saturated!(offset);
	let slice = eof.data_slice(offset_usize, 32);

	// If data less than 32 bytes, fill the rest with zeros
	let mut data = [0u8; 32];
	data[..slice.len()].copy_from_slice(slice);

	push_u256!(runtime, U256::from(data));
	Control::Continue
}

/// Loads a 32-byte value from the data section with immediate value of the EOF container.
///
/// 1. Has one immediate argument `offset`, encoded as a 16-bit unsigned big-endian value.
/// 2. Pops nothing from the stack.
/// 3. Reads `[offset:offset+32]` segment from the data section and pushes it as 32-byte value to the stack.
///
/// `[offset:offset+32]` is guaranteed to be within data bounds by code validation.
pub fn data_loadn<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	// Immediate value after the instruction.
	// 16-bit unsigned big-endian value
	let raw_offset = try_or_fail!(runtime.machine.get_code_and_inc_pc(2));
	let offset = usize::from(eof::get_u16(raw_offset, 0));
	let data = eof.data_slice(offset, 32);

	push_u256!(runtime, U256::from(data));
	Control::Continue
}

/// Returns the size of the data section of the EOF container.
///
/// 1. Pops nothing from the stack.
/// 2. Pushes the size of the data section of the active container to the stack.
pub fn data_size<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	let data_size = eof.header.data_size;

	push_u256!(runtime, U256::from(data_size));
	Control::Continue
}

/// Copies a range of data from the data section of the EOF container to memory.
///
/// 1. Pops three values from the stack: `mem_offset, offset, size`.
/// 2, Performs memory expansion to `mem_offset + size` and deducts memory expansion cost.
/// 3. Reads `[offset:offset+size]` segment from the data section and writes it to memory starting at offset `mem_offset`.
/// 4. If `offset + size` is greater than data section size, 0 bytes will be copied for bytes after the end of the data section.
pub fn data_copy<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	unreachable!()
}

#[cfg(test)]
mod tests {
	#![allow(unused_variables)]

	use super::*;
	use crate::eof::{Eof, EofHeader};
	use crate::{Context, CreateScheme, Transfer};
	use evm_core::{Capture, ExitReason, ExternalOperation};
	use primitive_types::{H160, H256, U256};
	use std::rc::Rc;

	struct MockHandler;

	impl Handler for MockHandler {
		type CreateInterrupt = ();
		type EOFCreateInterrupt = ();
		type CreateFeedback = ();
		type CallInterrupt = ();
		type CallFeedback = ();
		fn balance(&self, _address: H160) -> U256 {
			unreachable!()
		}
		fn code_size(&mut self, _address: H160) -> U256 {
			unreachable!()
		}
		fn code_hash(&mut self, address: H160) -> H256 {
			unreachable!()
		}
		fn code(&self, address: H160) -> Vec<u8> {
			unreachable!()
		}
		fn storage(&self, address: H160, index: H256) -> H256 {
			unreachable!()
		}
		fn is_empty_storage(&self, address: H160) -> bool {
			unreachable!()
		}
		fn original_storage(&self, address: H160, index: H256) -> H256 {
			unreachable!()
		}
		fn gas_left(&self) -> U256 {
			unreachable!()
		}
		fn gas_price(&self) -> U256 {
			unreachable!()
		}
		fn origin(&self) -> H160 {
			unreachable!()
		}
		fn block_hash(&self, number: U256) -> H256 {
			unreachable!()
		}
		fn block_number(&self) -> U256 {
			unreachable!()
		}
		fn block_coinbase(&self) -> H160 {
			unreachable!()
		}
		fn block_timestamp(&self) -> U256 {
			unreachable!()
		}
		fn block_difficulty(&self) -> U256 {
			unreachable!()
		}
		fn block_randomness(&self) -> Option<H256> {
			unreachable!()
		}
		fn block_gas_limit(&self) -> U256 {
			unreachable!()
		}
		fn block_base_fee_per_gas(&self) -> U256 {
			unreachable!()
		}
		fn chain_id(&self) -> U256 {
			unreachable!()
		}
		fn exists(&self, address: H160) -> bool {
			unreachable!()
		}
		fn deleted(&self, address: H160) -> bool {
			unreachable!()
		}
		fn is_cold(&mut self, address: H160, index: Option<H256>) -> bool {
			unreachable!()
		}
		fn set_storage(
			&mut self,
			address: H160,
			index: H256,
			value: H256,
		) -> Result<(), ExitError> {
			unreachable!()
		}
		fn log(
			&mut self,
			address: H160,
			topics: Vec<H256>,
			data: Vec<u8>,
		) -> Result<(), ExitError> {
			unreachable!()
		}
		fn mark_delete(&mut self, address: H160, target: H160) -> Result<(), ExitError> {
			unreachable!()
		}
		fn create(
			&mut self,
			caller: H160,
			scheme: CreateScheme,
			value: U256,
			init_code: Vec<u8>,
			target_gas: Option<u64>,
		) -> Capture<(ExitReason, Vec<u8>), Self::CreateInterrupt> {
			unreachable!()
		}
		fn call(
			&mut self,
			code_address: H160,
			transfer: Option<Transfer>,
			input: Vec<u8>,
			target_gas: Option<u64>,
			is_static: bool,
			context: Context,
		) -> Capture<(ExitReason, Vec<u8>), Self::CallInterrupt> {
			unreachable!()
		}
		fn record_external_operation(&mut self, op: ExternalOperation) -> Result<(), ExitError> {
			unreachable!()
		}
		fn blob_base_fee(&self) -> Option<u128> {
			unreachable!()
		}
		fn get_blob_hash(&self, index: usize) -> Option<U256> {
			unreachable!()
		}
		fn tstore(&mut self, address: H160, index: H256, value: U256) -> Result<(), ExitError> {
			unreachable!()
		}
		fn tload(&mut self, _address: H160, _index: H256) -> Result<U256, ExitError> {
			unreachable!()
		}
		fn get_authority_target(&mut self, _address: H160) -> Option<H160> {
			unreachable!()
		}
		fn authority_code(&mut self, _authority: H160) -> Vec<u8> {
			unreachable!()
		}
		fn warm_target(&mut self, _target: (H160, Option<H256>)) {
			unreachable!()
		}
	}

	fn create_eof() -> Eof {
		let header = EofHeader {
			types_size: 8,
			code_sizes: vec![2, 4],
			container_sizes: vec![1, 3],
			data_size: 3,
			sum_code_sizes: 6,
			sum_container_sizes: 4,
			header_size: 24,
		};
		let input = vec![
			0xEF, 0x00, 0x01, // HEADER: meta information
			0x01, 0x00, 0x08, // Types size
			0x02, 0x00, 0x02, // Code size
			0x00, 0x02, 0x00, 0x04, // Code section
			0x03, 0x00, 0x02, // Container size
			0x00, 0x01, 0x00, 0x03, // Container section
			0x04, 0x00, 0x03, // Data size
			0x00, // Terminator
			0x1A, 0x0C, 0x01, 0xFD, // BODY: types section data [1]
			0x3E, 0x6D, 0x02, 0x9A, // types section data [2]
			0xA9, 0xE0, // Code size data [1]
			0xCF, 0x39, 0x8A, 0x3B, // Code size data [2]
			0xB8, // Container size data [1]
			0xE7, 0xB3, 0x7C, // Container size data [2]
			0x3B, 0x5F, 0xE3, // Data size data
		];
		let decoded_header = EofHeader::decode(&input);
		assert_eq!(Ok(header.clone()), decoded_header);

		let eof = Eof::decode(&input).expect("Decode EOF");
		assert_eq!(eof.header, header);
		eof
	}

	fn init_runtime(code: Vec<u8>, eof: Option<Eof>) -> Runtime {
		Runtime::new(
			Rc::new(code),
			Rc::new(vec![]),
			Context {
				address: H160::zero(),
				caller: H160::zero(),
				eof,
				apparent_value: U256::zero(),
			},
			1024,
			10000,
		)
	}

	#[test]
	fn test_data_size() {
		let mut runtime = init_runtime(vec![], Some(create_eof()));

		let control = data_size(&mut runtime, &mut MockHandler);
		assert!(matches!(control, Control::Continue));
		assert_eq!(runtime.machine.stack().peek(0).unwrap(), U256::from(3));
	}

	#[test]
	fn test_data_size_not_eof() {
		let mut runtime = init_runtime(vec![], None);

		let control = data_size(&mut runtime, &mut MockHandler);
		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::EOFOpcodeDisabledInLegacy))
		));
	}
}
