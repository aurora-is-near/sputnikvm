//! # EIP-7069: Revamped CALL instructions
//! Introduce `EXTCALL`, `EXTDELEGATECALL` and `EXTSTATICCALL` with simplified semantics
//!
//! ## New Instructions
//! - `EXTCALL`
//! - `EXTDELEGATECALL`
//! - `EXTSTATICCALL`
//! - `RETURNDATALOAD`
//!
//! [EIP-7069](https://eips.ethereum.org/EIPS/eip-7069)
#![allow(clippy::module_name_repetitions)]

use crate::eval::Control;
use crate::{ExitError, Handler, Runtime};
use primitive_types::{H256, U256};

pub fn return_data_load<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	pop_u256!(runtime, offset);

	let offset_usize = as_usize_saturated!(offset);
	let buffer_len = runtime.return_data_buffer.len();
	let mut output = [0u8; 32];
	if let Some(result_len) = buffer_len.checked_sub(offset_usize) {
		let copy_len = result_len.min(32);
		output[..copy_len]
			.copy_from_slice(&runtime.return_data_buffer[offset_usize..offset_usize + copy_len]);
	}

	push_h256!(runtime, H256(output));
	Control::Continue
}

pub fn ext_call<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	pop_h256!(runtime, to);
	// Check if target is left padded with zeroes.
	if to.0[..12].iter().any(|i| *i != 0) {
		return Control::Exit(ExitError::InvalidEXTCALLTarget.into());
	}

	Control::Continue
}

pub fn ext_delegate_call<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);

	Control::Continue
}

pub fn ext_static_call<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);

	Control::Continue
}
