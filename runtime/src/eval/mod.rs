#[macro_use]
mod macros;
mod eof;
mod system;

use crate::context::ExtCallScheme;
use crate::prelude::*;
use crate::{CallScheme, ExitReason, Handler, Opcode, Runtime};
use core::cmp::min;
use primitive_types::{H160, H256, U256};

pub enum Control<H: Handler> {
	Continue,
	CallInterrupt(H::CallInterrupt),
	CreateInterrupt(H::CreateInterrupt),
	// TODO: resolve clippy
	#[allow(dead_code)]
	EOFCreateInterrupt(H::EOFCreateInterrupt),
	Exit(ExitReason),
}

fn handle_other<H: Handler>(state: &mut Runtime, opcode: Opcode, handler: &mut H) -> Control<H> {
	match handler.other(opcode, &mut state.machine) {
		Ok(()) => Control::Continue,
		Err(e) => Control::Exit(e.into()),
	}
}

pub fn eval<H: Handler>(state: &mut Runtime, opcode: Opcode, handler: &mut H) -> Control<H> {
	match opcode {
		Opcode::SHA3 => system::sha3(state),
		Opcode::ADDRESS => system::address(state),
		Opcode::BALANCE => system::balance(state, handler),
		Opcode::SELFBALANCE => system::selfbalance(state, handler),
		Opcode::ORIGIN => system::origin(state, handler),
		Opcode::CALLER => system::caller(state),
		Opcode::CALLVALUE => system::callvalue(state),
		Opcode::GASPRICE => system::gasprice(state, handler),
		Opcode::EXTCODESIZE => system::extcodesize(state, handler),
		Opcode::EXTCODEHASH => system::extcodehash(state, handler),
		Opcode::EXTCODECOPY => system::extcodecopy(state, handler),
		Opcode::RETURNDATASIZE => system::returndatasize(state),
		Opcode::RETURNDATACOPY => system::returndatacopy(state),
		Opcode::BLOCKHASH => system::blockhash(state, handler),
		Opcode::COINBASE => system::coinbase(state, handler),
		Opcode::TIMESTAMP => system::timestamp(state, handler),
		Opcode::NUMBER => system::number(state, handler),
		Opcode::PREVRANDAO => system::prevrandao(state, handler),
		Opcode::GASLIMIT => system::gaslimit(state, handler),
		Opcode::SLOAD => system::sload(state, handler),
		Opcode::SSTORE => system::sstore(state, handler),
		Opcode::GAS => system::gas(state, handler),
		Opcode::LOG0 => system::log(state, 0, handler),
		Opcode::LOG1 => system::log(state, 1, handler),
		Opcode::LOG2 => system::log(state, 2, handler),
		Opcode::LOG3 => system::log(state, 3, handler),
		Opcode::LOG4 => system::log(state, 4, handler),
		Opcode::SELFDESTRUCT => system::selfdestruct(state, handler),
		Opcode::CREATE => system::create(state, false, handler),
		Opcode::CREATE2 => system::create(state, true, handler),
		Opcode::CALL => system::call(state, CallScheme::Call, handler),
		Opcode::CALLCODE => system::call(state, CallScheme::CallCode, handler),
		Opcode::DELEGATECALL => system::call(state, CallScheme::DelegateCall, handler),
		Opcode::STATICCALL => system::call(state, CallScheme::StaticCall, handler),
		Opcode::CHAINID => system::chainid(state, handler),
		Opcode::BASEFEE => system::base_fee(state, handler),
		Opcode::BLOBBASEFEE => system::blob_base_fee(state, handler),
		Opcode::BLOBHASH => system::blob_hash(state, handler),
		Opcode::TLOAD => system::tload(state, handler),
		Opcode::TSTORE => system::tstore(state, handler),
		Opcode::MCOPY => system::mcopy(state, handler),
		//== EOF Related Opcodes ==//
		Opcode::DATALOAD => eof::data::data_load(state),
		Opcode::DATALOADN => eof::data::data_loadn(state),
		Opcode::DATASIZE => eof::data::data_size(state),
		Opcode::DATACOPY => eof::data::data_copy(state),
		Opcode::RJUMP => eof::control::rjump(state, handler),
		Opcode::RJUMPI => eof::control::rjumpi(state, handler),
		Opcode::RJUMPV => eof::control::rjumpv(state, handler),
		Opcode::CALLF => eof::call::callf(state, handler),
		Opcode::RETF => eof::call::retf(state, handler),
		Opcode::JUMPF => eof::jump::jumpf(state, handler),
		Opcode::DUPN => eof::stack::dupn(state),
		Opcode::SWAPN => eof::stack::swapn(state),
		Opcode::EXCHANGE => eof::stack::exchange(state),
		Opcode::EOFCREATE => eof::contract::eof_create(state, handler),
		Opcode::RETURNCONTRACT => system::return_contract(state, handler),
		Opcode::RETURNDATALOAD => eof::ext::return_data_load(state, handler),
		Opcode::EXTCALL => eof::ext::ext_call(state, handler, ExtCallScheme::ExtCall),
		Opcode::EXTDELEGATECALL => {
			eof::ext::ext_call(state, handler, ExtCallScheme::ExtDelegateCall)
		}
		Opcode::EXTSTATICCALL => eof::ext::ext_call(state, handler, ExtCallScheme::ExtStaticCall),
		_ => handle_other(state, opcode, handler),
	}
}

/// TODO: fix clippy
#[allow(clippy::needless_pass_by_value)]
pub fn finish_eof_create(
	_runtime: &mut Runtime,
	_reason: ExitReason,
	_address: Option<H160>,
	_return_data: Vec<u8>,
) -> Result<(), ExitReason> {
	todo!()
}

pub fn finish_create(
	runtime: &mut Runtime,
	reason: ExitReason,
	address: Option<H160>,
	return_data: Vec<u8>,
) -> Result<(), ExitReason> {
	runtime.return_data_buffer = return_data;
	let create_address: H256 = address.map(Into::into).unwrap_or_default();

	match reason {
		ExitReason::Succeed(_) => {
			runtime
				.machine
				.stack_mut()
				.push(U256::from_big_endian(&create_address[..]))?;
			Ok(())
		}
		ExitReason::Revert(_) => {
			runtime.machine.stack_mut().push(U256::zero())?;
			Ok(())
		}
		ExitReason::Error(_) => {
			runtime.machine.stack_mut().push(U256::zero())?;
			Ok(())
		}
		ExitReason::Fatal(e) => {
			runtime.machine.stack_mut().push(U256::zero())?;
			Err(e.into())
		}
	}
}

pub fn finish_call(
	runtime: &mut Runtime,
	out_len: usize,
	out_offset: usize,
	reason: ExitReason,
	return_data: Vec<u8>,
) -> Result<(), ExitReason> {
	runtime.return_data_buffer = return_data;
	let target_len = min(out_len, runtime.return_data_buffer.len());

	match reason {
		ExitReason::Succeed(_) => {
			let value_to_push = if runtime
				.machine
				.memory_mut()
				.copy_large(
					out_offset,
					U256::zero(),
					target_len,
					&runtime.return_data_buffer[..],
				)
				.is_ok()
			{
				U256::one()
			} else {
				U256::zero()
			};
			runtime
				.machine
				.stack_mut()
				.push(value_to_push)
				.map_err(Into::into)
		}
		ExitReason::Revert(_) => {
			runtime.machine.stack_mut().push(U256::zero())?;

			let _ = runtime.machine.memory_mut().copy_large(
				out_offset,
				U256::zero(),
				target_len,
				&runtime.return_data_buffer[..],
			);

			Ok(())
		}
		ExitReason::Error(_) => {
			runtime.machine.stack_mut().push(U256::zero())?;

			Ok(())
		}
		ExitReason::Fatal(e) => {
			runtime.machine.stack_mut().push(U256::zero())?;

			Err(e.into())
		}
	}
}
