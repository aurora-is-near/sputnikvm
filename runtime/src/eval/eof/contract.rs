use crate::eof::Eof;
use crate::eval::Control;
use crate::{Handler, Runtime, Vec};
use evm_core::ExitError;

pub fn eof_create<H: Handler>(runtime: &mut Runtime, _handler: &mut H) -> Control<H> {
	let eof = require_eof!(runtime);
	// - check non-static call
	// - gas EOF_CREATE_GAS
	let imm = try_or_fail!(runtime.machine.get_code_and_inc_pc(1));
	let initcontainer_index = usize::from(imm[0]);
	pop_u256!(runtime, _value, _salt, data_offset, data_len);

	let _sub_container = eof
		.body
		.container_section
		.get(initcontainer_index)
		.copied()
		.expect("EOF is validated");
	// Cast to `usize` after length checking to avoid overflow
	let data_offset = if data_offset.is_zero() {
		usize::MAX
	} else {
		as_usize_or_fail!(data_offset)
	};
	let data_len = as_usize_or_fail!(data_len);
	try_or_fail!(runtime
		.machine
		.memory_mut()
		.resize_offset(data_offset, data_len));
	let input = if data_len == 0 {
		Vec::new()
	} else {
		runtime.machine.memory().get(data_offset, data_len)
	};
	let new_eof = Eof::decode(&input).expect("EOF is validated");
	if !new_eof.body.is_data_filled {
		// should be always false as it is verified by eof verification.
		unreachable!("EOF is validated");
	}
	// TODO: deduct gas for hash that is needed to calculate address.
	// TODO: align address
	#[allow(clippy::no_effect_underscore_binding)]
	let _created_address = runtime.context.address;
	// TODO: remaining_63_of_64_parts
	// let gas_limit = 0;
	// TODO: record gas
	Control::Continue
}
