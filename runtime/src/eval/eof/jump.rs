//! # EIP-6206: EOF - JUMPF and non-returning functions
//!
//! Introduces instruction for chaining function calls.
//! [EIP-6206](https://eips.ethereum.org/EIPS/eip-6206)
use crate::eval::Control;
use crate::{Handler, Runtime};
use evm_core::{ExitError, ExitFatal};

pub fn jumpf<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	// Immediate value after the instruction.
	// 16-bit unsigned big-endian value
	let raw_offset = try_or_fail!(runtime.machine.get_code_and_inc_pc(2));
	let target_section_index = usize::from(crate::eof::get_u16(raw_offset, 0));

	// Get target types
	let Some(types) = eof.body.types_section.get(target_section_index) else {
		return Control::Exit(ExitFatal::CallErrorAsFatal(ExitError::EOFUnexpectedCall).into());
	};

	// Check max stack height for target code section. It's safe to subtract
	// as max_stack_height is always more than inputs.
	if runtime.machine.stack().len() + usize::from(types.max_stack_size - u16::from(types.inputs))
		> 1024
	{
		return Control::Exit(ExitError::StackOverflow.into());
	}

	runtime
		.context
		.eof_function_stack
		.set_current_code_index(target_section_index);
	let Some(code_section) = eof.body.code_section.get(target_section_index) else {
		return Control::Exit(ExitFatal::CallErrorAsFatal(ExitError::EOFUnexpectedCall).into());
	};
	// Set machine code to target code section
	runtime.machine.set_code(code_section);
	// Set PC to position 0
	runtime.machine.set_pc(0);

	Control::Continue
}
