#![allow(unused_variables)]
#![allow(
	clippy::cast_possible_truncation,
	clippy::as_conversions,
	clippy::module_name_repetitions
)]

use crate::eof::{Eof, EofHeader};
use crate::{Context, CreateScheme, Handler, Runtime, Transfer};
use evm_core::{Capture, ExitError, ExitReason, ExternalOperation};
use primitive_types::{H160, H256, U256};
use std::rc::Rc;

pub struct MockHandler;

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
	fn set_storage(&mut self, address: H160, index: H256, value: H256) -> Result<(), ExitError> {
		unreachable!()
	}
	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) -> Result<(), ExitError> {
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

pub fn create_eof(data: Vec<u8>) -> Eof {
	let header = EofHeader {
		types_size: 8,
		code_sizes: vec![2, 4],
		container_sizes: vec![1, 3],
		data_size: data.len() as u16,
		sum_code_sizes: 6,
		sum_container_sizes: 4,
		header_size: 24,
	};
	let input = vec![
		0xEF,
		0x00,
		0x01, // HEADER: meta information
		0x01,
		0x00,
		0x08, // Types size
		0x02,
		0x00,
		0x02, // Code size
		0x00,
		0x02,
		0x00,
		0x04, // Code section
		0x03,
		0x00,
		0x02, // Container size
		0x00,
		0x01,
		0x00,
		0x03, // Container section
		0x04,
		0x00,
		data.len() as u8, // Data size
		0x00,             // Terminator
		0x1A,
		0x0C,
		0x01,
		0xFD, // BODY: types section data [1]
		0x3E,
		0x6D,
		0x02,
		0x9A, // types section data [2]
		0xA9,
		0xE0, // Code size data [1]
		0xCF,
		0x39,
		0x8A,
		0x3B, // Code size data [2]
		0xB8, // Container size data [1]
		0xE7,
		0xB3,
		0x7C, // Container size data [2]
	]
	.into_iter()
	.chain(data)
	.collect::<Vec<_>>();

	let decoded_header = EofHeader::decode(&input);
	assert_eq!(Ok(header.clone()), decoded_header);

	let eof = Eof::decode(&input).expect("Decode EOF");
	assert_eq!(eof.header, header);
	eof
}

pub fn init_runtime(code: Vec<u8>, eof: Option<Eof>) -> Runtime {
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
		32 * 1024,
	)
}
