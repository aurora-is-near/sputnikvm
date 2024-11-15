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

use crate::context::ExtCallScheme;
use crate::eval::{finish_call, Control};
use crate::{Context, ExitError, Handler, Runtime, Transfer, Vec};
use evm_core::Capture;
use primitive_types::{H160, H256, U256};

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

pub fn ext_call<H: Handler>(
	runtime: &mut Runtime,
	handler: &mut H,
	scheme: ExtCallScheme,
) -> Control<H> {
	require_eof!(runtime);
	pop_h256!(runtime, to);
	// Check if target is left padded with zeroes.
	if to.0[..12].iter().any(|i| *i != 0) {
		return Control::Exit(ExitError::InvalidEXTCALLTarget.into());
	}
	let to_address = H160::from_slice(&to.0[12..]);
	// Clear return data buffer
	runtime.return_data_buffer = Vec::new();

	let value = match scheme {
		ExtCallScheme::ExtCall => {
			pop_u256!(runtime, value);
			value
		}
		ExtCallScheme::ExtDelegateCall | ExtCallScheme::ExtStaticCall => U256::zero(),
	};
	pop_u256!(runtime, in_offset, in_len);
	// Cast to `usize` after length checking to avoid overflow
	let in_offset = if in_len == U256::zero() {
		usize::MAX
	} else {
		as_usize_or_fail!(in_offset)
	};
	let in_len = as_usize_or_fail!(in_len);

	try_or_fail!(runtime
		.machine
		.memory_mut()
		.resize_offset(in_offset, in_len));
	let input = if in_len == 0 {
		Vec::new()
	} else {
		runtime.machine.memory().get(in_offset, in_len)
	};
	let context = match scheme {
		ExtCallScheme::ExtCall | ExtCallScheme::ExtStaticCall => Context {
			address: to_address,
			caller: runtime.context.address,
			eof: runtime.context.eof.clone(),
			apparent_value: value,
		},
		ExtCallScheme::ExtDelegateCall => Context {
			address: runtime.context.address,
			caller: runtime.context.caller,
			eof: runtime.context.eof.clone(),
			apparent_value: runtime.context.apparent_value,
		},
	};
	let transfer = if scheme == ExtCallScheme::ExtCall {
		Some(Transfer {
			source: runtime.context.address,
			target: to_address,
			value,
		})
	} else {
		None
	};

	// Calculate the gas available to callee as caller’s
	// remaining gas reduced by max(ceil(gas/64), MIN_RETAINED_GAS) (MIN_RETAINED_GAS is 5000).
	let remain_gas = handler.gas_left().low_u64();
	let gas_limit = remain_gas.saturating_sub(core::cmp::max(
		remain_gas / 64,
		crate::eof::MIN_RETAINED_GAS,
	));
	// The MIN_CALLEE_GAS rule is a replacement for stipend:
	// it simplifies the reasoning about the gas costs and is
	// applied uniformly for all introduced EXT*CALL instructions.
	//
	// If Gas available to callee is less than MIN_CALLEE_GAS trigger light failure (Same as Revert).
	if gas_limit < crate::eof::MIN_CALLEE_GAS {
		// Push 1 to stack to indicate that call light failed.
		// It is safe to ignore stack overflow error as we already popped multiple values from stack.
		push_u256!(runtime, U256::from(1));
	}

	match handler.call(to_address, transfer, input, Some(gas_limit), false, context) {
		Capture::Exit((reason, return_data)) => {
			match finish_call(runtime, 0, 0, reason, return_data) {
				Ok(()) => Control::Continue,
				Err(e) => Control::Exit(e),
			}
		}
		Capture::Trap(interrupt) => {
			runtime.return_data_len = 0;
			runtime.return_data_offset = 0;
			Control::CallInterrupt(interrupt)
		}
	}
}
