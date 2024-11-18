use crate::eof;
use crate::eval::Control;
use crate::{Handler, Runtime};
use evm_core::ExitError;

#[allow(dead_code)]
pub fn rjump<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	// In spec it is +3 but pointer is already incremented in
	// `Interpreter::step` so for EVM is +2.
	let raw_offset = try_or_fail!(runtime.machine.get_code_and_inc_pc(2));
	// Immediate value after the instruction.
	// 16-bit unsigned big-endian value
	let offset = usize::from(eof::get_u16(raw_offset, 0));
	try_or_fail!(runtime.machine.inc_pc(offset));

	Control::Continue
}

#[allow(dead_code)]
pub fn rjumpi<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	pop_u256!(runtime, condition);
	// In spec it is +3 but pointer is already incremented in
	// `Interpreter::step` so for EVM is +2.
	let raw_offset = try_or_fail!(runtime.machine.get_code_and_inc_pc(2));
	if !condition.is_zero() {
		let offset = usize::from(eof::get_u16(raw_offset, 0));
		// Set PC offset to the new offset
		// It includes previous increment +2
		try_or_fail!(runtime.machine.inc_pc(offset));
	}

	Control::Continue
}

#[allow(dead_code)]
pub fn rjumpv<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	pop_u256!(runtime, case);
	let case = as_usize_saturated!(case);
	let raw_max_index = try_or_fail!(runtime.machine.get_code_and_inc_pc(1));
	// For number of items we are adding 1 to max_index, multiply by 2 as each offset is 2 bytes
	let max_index = usize::from(raw_max_index[0]);
	let mut offset = (max_index + 1) * 2;
	if case <= max_index {
		let raw_offset = try_or_fail!(runtime.machine.get_code_with_offset(1 + case * 2));
		offset += usize::from(eof::get_u16(raw_offset, 0));
	}
	// Set PC offset to the new offset
	try_or_fail!(runtime.machine.inc_pc(offset));

	Control::Continue
}
