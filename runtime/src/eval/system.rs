use super::Control;
use crate::{
	CallScheme, Capture, Context, CreateScheme, ExitError, ExitFatal, ExitReason, ExitSucceed,
	Handler, Runtime, Transfer,
};
use alloc::vec::Vec;
use core::cmp::min;
use evm_core::Machine;
use primitive_types::{H160, H256, U256};
use sha3::{Digest, Keccak256};

pub fn sha3<H: Handler>(machine: &mut Machine) -> Control<H> {
	pop_u256!(machine, from, len);

	try_or_fail!(machine.memory_mut().resize_offset(from, len));
	let data = if len == U256::zero() {
		Vec::new()
	} else {
		let from = as_usize_or_fail!(from);
		let len = as_usize_or_fail!(len);

		machine.memory_mut().get(from, len)
	};

	let ret = Keccak256::digest(data.as_slice());
	push_h256!(machine, H256::from_slice(ret.as_slice()));

	Control::Continue
}

pub fn chainid<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_u256!(machine, handler.chain_id());

	Control::Continue
}

pub fn address<H: Handler>(machine: &mut Machine, address: &H160) -> Control<H> {
	let ret = H256::from(*address);
	push_h256!(machine, ret);

	Control::Continue
}

pub fn balance<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	pop_h256!(machine, address);
	push_u256!(machine, handler.balance(address.into()));

	Control::Continue
}

pub fn selfbalance<H: Handler>(machine: &mut Machine, address: &H160, handler: &H) -> Control<H> {
	push_u256!(machine, handler.balance(*address));

	Control::Continue
}

pub fn origin<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	let ret = H256::from(handler.origin());
	push_h256!(machine, ret);

	Control::Continue
}

pub fn caller<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	let ret = H256::from(runtime.context.caller);
	push_h256!(runtime.machine, ret);

	Control::Continue
}

pub fn callvalue<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	let mut ret = H256::default();
	runtime.context.apparent_value.to_big_endian(&mut ret[..]);
	push_h256!(runtime.machine, ret);

	Control::Continue
}

pub fn gasprice<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	let mut ret = H256::default();
	handler.gas_price().to_big_endian(&mut ret[..]);
	push_h256!(machine, ret);

	Control::Continue
}

pub fn base_fee<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	let mut ret = H256::default();
	handler.block_base_fee_per_gas().to_big_endian(&mut ret[..]);
	push_h256!(machine, ret);

	Control::Continue
}

pub fn extcodesize<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	pop_h256!(machine, address);
	push_u256!(machine, handler.code_size(address.into()));

	Control::Continue
}

pub fn extcodehash<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	pop_h256!(machine, address);
	push_h256!(machine, handler.code_hash(address.into()));

	Control::Continue
}

pub fn extcodecopy<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	pop_h256!(machine, address);
	pop_u256!(machine, memory_offset, code_offset, len);

	try_or_fail!(machine.memory_mut().resize_offset(memory_offset, len));
	match machine.memory_mut().copy_large(
		memory_offset,
		code_offset,
		len,
		&handler.code(address.into()),
	) {
		Ok(()) => (),
		Err(e) => return Control::Exit(e.into()),
	};

	Control::Continue
}

pub fn returndatasize<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	let size = U256::from(runtime.return_data_buffer.len());
	push_u256!(runtime.machine, size);

	Control::Continue
}

pub fn returndatacopy<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	pop_u256!(runtime.machine, memory_offset, data_offset, len);

	try_or_fail!(runtime
		.machine
		.memory_mut()
		.resize_offset(memory_offset, len));
	if data_offset
		.checked_add(len)
		.map(|l| l > U256::from(runtime.return_data_buffer.len()))
		.unwrap_or(true)
	{
		return Control::Exit(ExitError::OutOfOffset.into());
	}

	match runtime.machine.memory_mut().copy_large(
		memory_offset,
		data_offset,
		len,
		&runtime.return_data_buffer,
	) {
		Ok(()) => Control::Continue,
		Err(e) => Control::Exit(e.into()),
	}
}

pub fn blockhash<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	pop_u256!(machine, number);
	push_h256!(machine, handler.block_hash(number));

	Control::Continue
}

pub fn coinbase<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_h256!(machine, handler.block_coinbase());
	Control::Continue
}

pub fn timestamp<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_u256!(machine, handler.block_timestamp());
	Control::Continue
}

pub fn number<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_u256!(machine, handler.block_number());
	Control::Continue
}

pub fn difficulty<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_u256!(machine, handler.block_difficulty());
	Control::Continue
}

pub fn gaslimit<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_u256!(machine, handler.block_gas_limit());
	Control::Continue
}

pub fn sload<H: Handler>(machine: &mut Machine, address: &H160, handler: &H) -> Control<H> {
	pop_h256!(machine, index);
	let value = handler.storage(*address, index);
	push_h256!(machine, value);

	event!(SLoad {
		address,
		index,
		value
	});

	Control::Continue
}

pub fn sstore<H: Handler>(machine: &mut Machine, address: &H160, handler: &mut H) -> Control<H> {
	pop_h256!(machine, index, value);

	event!(SStore {
		address,
		index,
		value
	});

	match handler.set_storage(*address, index, value) {
		Ok(()) => Control::Continue,
		Err(e) => Control::Exit(e.into()),
	}
}

pub fn gas<H: Handler>(machine: &mut Machine, handler: &H) -> Control<H> {
	push_u256!(machine, handler.gas_left());

	Control::Continue
}

pub fn log<H: Handler>(
	machine: &mut Machine,
	address: &H160,
	n: u8,
	handler: &mut H,
) -> Control<H> {
	pop_u256!(machine, offset, len);

	try_or_fail!(machine.memory_mut().resize_offset(offset, len));
	let data = if len == U256::zero() {
		Vec::new()
	} else {
		let offset = as_usize_or_fail!(offset);
		let len = as_usize_or_fail!(len);

		machine.memory().get(offset, len)
	};

	let mut topics = Vec::new();
	for _ in 0..(n as usize) {
		match machine.stack_mut().pop_h256() {
			Ok(value) => {
				topics.push(value);
			}
			Err(e) => return Control::Exit(e.into()),
		}
	}

	match handler.log(*address, topics, data) {
		Ok(()) => Control::Continue,
		Err(e) => Control::Exit(e.into()),
	}
}

pub fn suicide<H: Handler>(machine: &mut Machine, address: &H160, handler: &mut H) -> Control<H> {
	pop_h256!(machine, target);

	match handler.mark_delete(*address, target.into()) {
		Ok(()) => (),
		Err(e) => return Control::Exit(e.into()),
	}

	Control::Exit(ExitSucceed::Suicided.into())
}

pub fn create<H: Handler>(runtime: &mut Runtime, is_create2: bool, handler: &mut H) -> Control<H> {
	runtime.return_data_buffer = Vec::new();

	pop_u256!(runtime.machine, value, code_offset, len);

	try_or_fail!(runtime.machine.memory_mut().resize_offset(code_offset, len));
	let code = if len == U256::zero() {
		Vec::new()
	} else {
		let code_offset = as_usize_or_fail!(code_offset);
		let len = as_usize_or_fail!(len);

		runtime.machine.memory().get(code_offset, len)
	};

	let scheme = if is_create2 {
		pop_h256!(runtime.machine, salt);
		let code_hash = H256::from_slice(Keccak256::digest(&code).as_slice());
		CreateScheme::Create2 {
			caller: runtime.context.address,
			salt,
			code_hash,
		}
	} else {
		CreateScheme::Legacy {
			caller: runtime.context.address,
		}
	};

	match handler.create(runtime.context.address, scheme, value, code, None) {
		Capture::Exit((reason, address, return_data)) => {
			runtime.return_data_buffer = return_data;
			let create_address: H256 = address.map(|a| a.into()).unwrap_or_default();

			match reason {
				ExitReason::Succeed(_) => {
					push_h256!(runtime.machine, create_address);
					Control::Continue
				}
				ExitReason::Revert(_) => {
					push_h256!(runtime.machine, H256::default());
					Control::Continue
				}
				ExitReason::Error(_) => {
					push_h256!(runtime.machine, H256::default());
					Control::Continue
				}
				ExitReason::Fatal(e) => {
					push_h256!(runtime.machine, H256::default());
					Control::Exit(e.into())
				}
			}
		}
		Capture::Trap(interrupt) => {
			push_h256!(runtime.machine, H256::default());
			Control::CreateInterrupt(interrupt)
		}
	}
}

pub fn call<H: Handler>(runtime: &mut Runtime, scheme: CallScheme, handler: &mut H) -> Control<H> {
	runtime.return_data_buffer = Vec::new();

	pop_u256!(runtime.machine, gas);
	pop_h256!(runtime.machine, to);
	let gas = if gas > U256::from(u64::MAX) {
		None
	} else {
		Some(gas.as_u64())
	};

	let value = match scheme {
		CallScheme::Call | CallScheme::CallCode => {
			pop_u256!(runtime.machine, value);
			value
		}
		CallScheme::DelegateCall | CallScheme::StaticCall => U256::zero(),
	};

	pop_u256!(runtime.machine, in_offset, in_len, out_offset, out_len);

	try_or_fail!(runtime
		.machine
		.memory_mut()
		.resize_offset(in_offset, in_len));
	try_or_fail!(runtime
		.machine
		.memory_mut()
		.resize_offset(out_offset, out_len));

	let input = if in_len == U256::zero() {
		Vec::new()
	} else {
		let in_offset = as_usize_or_fail!(in_offset);
		let in_len = as_usize_or_fail!(in_len);

		runtime.machine.memory().get(in_offset, in_len)
	};

	let context = match scheme {
		CallScheme::Call | CallScheme::StaticCall => Context {
			address: to.into(),
			caller: runtime.context.address,
			apparent_value: value,
		},
		CallScheme::CallCode => Context {
			address: runtime.context.address,
			caller: runtime.context.address,
			apparent_value: value,
		},
		CallScheme::DelegateCall => Context {
			address: runtime.context.address,
			caller: runtime.context.caller,
			apparent_value: runtime.context.apparent_value,
		},
	};

	let transfer = if scheme == CallScheme::Call {
		Some(Transfer {
			source: runtime.context.address,
			target: to.into(),
			value,
		})
	} else if scheme == CallScheme::CallCode {
		Some(Transfer {
			source: runtime.context.address,
			target: runtime.context.address,
			value,
		})
	} else {
		None
	};

	match handler.call(
		to.into(),
		transfer,
		input,
		gas,
		scheme == CallScheme::StaticCall,
		context,
	) {
		Capture::Exit((reason, return_data)) => {
			runtime.return_data_buffer = return_data;
			let target_len = min(out_len, U256::from(runtime.return_data_buffer.len()));

			match reason {
				ExitReason::Succeed(_) => {
					match runtime.machine.memory_mut().copy_large(
						out_offset,
						U256::zero(),
						target_len,
						&runtime.return_data_buffer[..],
					) {
						Ok(()) => {
							push_u256!(runtime.machine, U256::one());
							Control::Continue
						}
						Err(_) => {
							push_u256!(runtime.machine, U256::zero());
							Control::Continue
						}
					}
				}
				ExitReason::Revert(_) => {
					push_u256!(runtime.machine, U256::zero());

					let _ = runtime.machine.memory_mut().copy_large(
						out_offset,
						U256::zero(),
						target_len,
						&runtime.return_data_buffer[..],
					);

					Control::Continue
				}
				ExitReason::Error(_) => {
					push_u256!(runtime.machine, U256::zero());

					Control::Continue
				}
				ExitReason::Fatal(e) => {
					push_u256!(runtime.machine, U256::zero());

					Control::Exit(e.into())
				}
			}
		}
		Capture::Trap(interrupt) => {
			push_h256!(runtime.machine, H256::default());
			Control::CallInterrupt(interrupt)
		}
	}
}

