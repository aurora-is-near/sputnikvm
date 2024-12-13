//! # EIP-4750: EOF - Functions
//!
//! Individual sections for functions with `CALLF` and `RETF` instructions.
//! [EIP-4750](https://eips.ethereum.org/EIPS/eip-4750)
use crate::eval::Control;
use crate::{Handler, Runtime};
use evm_core::{ExitError, ExitFatal};

pub fn callf<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	// Immediate value after the instruction.
	// 16-bit unsigned big-endian value
	let raw_offset = try_or_fail!(runtime.machine.get_code_and_inc_pc(2));
	let target_section_index = usize::from(crate::eof::get_u16(raw_offset, 0));

	if runtime.context.eof_function_stack.return_stack_len() >= 1024 {
		return Control::Exit(ExitError::EOFFunctionStackOverflow.into());
	}

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

	// Push current offset and PC to the `callf` stack. PC is incremented by 2
	// to point to the next instruction after `callf`.
	runtime.context.eof_function_stack.push(
		runtime.machine.position().clone().unwrap_or_default(),
		target_section_index,
	);
	let Some(code_section) = eof.body.code_section.get(target_section_index) else {
		return Control::Exit(ExitFatal::CallErrorAsFatal(ExitError::EOFUnexpectedCall).into());
	};
	// Set machine code to target code section
	runtime.machine.set_code(code_section);
	// Set PC to position 0
	runtime.machine.set_pc(0);

	Control::Continue
}

pub fn retf<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	let Some(function_return_state) = runtime.context.eof_function_stack.pop() else {
		return Control::Exit(ExitFatal::CallErrorAsFatal(ExitError::EOFUnexpectedCall).into());
	};

	let Some(code_section) = eof.body.code_section.get(function_return_state.index) else {
		return Control::Exit(ExitFatal::CallErrorAsFatal(ExitError::EOFUnexpectedCall).into());
	};
	// Set machine code to target code section
	runtime.machine.set_code(code_section);
	// Set PC to position
	runtime.machine.set_pc(function_return_state.pc);

	Control::Continue
}
