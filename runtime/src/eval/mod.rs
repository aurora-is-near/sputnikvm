#[macro_use]
mod macros;
mod system;

use evm_core::Machine;
use crate::{CallScheme, ExitReason, Handler, Opcode, Runtime};

pub enum Control<H: Handler> {
	Continue,
	CallInterrupt(H::CallInterrupt),
	CreateInterrupt(H::CreateInterrupt),
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
		Opcode::SHA3 => system::sha3(&mut state.machine),
		Opcode::ADDRESS => system::address(&mut state.machine, &state.context.address),
		Opcode::BALANCE => system::balance(&mut state.machine, handler),
		Opcode::SELFBALANCE => {
			system::selfbalance(&mut state.machine, &state.context.address, handler)
		}
		Opcode::ORIGIN => system::origin(&mut state.machine, handler),
		Opcode::CALLER => system::caller(state),
		Opcode::CALLVALUE => system::callvalue(state),
		Opcode::GASPRICE => system::gasprice(&mut state.machine, handler),
		Opcode::EXTCODESIZE => system::extcodesize(&mut state.machine, handler),
		Opcode::EXTCODEHASH => system::extcodehash(&mut state.machine, handler),
		Opcode::EXTCODECOPY => system::extcodecopy(&mut state.machine, handler),
		Opcode::RETURNDATASIZE => system::returndatasize(state),
		Opcode::RETURNDATACOPY => system::returndatacopy(state),
		Opcode::BLOCKHASH => system::blockhash(&mut state.machine, handler),
		Opcode::COINBASE => system::coinbase(&mut state.machine, handler),
		Opcode::TIMESTAMP => system::timestamp(&mut state.machine, handler),
		Opcode::NUMBER => system::number(&mut state.machine, handler),
		Opcode::DIFFICULTY => system::difficulty(&mut state.machine, handler),
		Opcode::GASLIMIT => system::gaslimit(&mut state.machine, handler),
		Opcode::SLOAD => system::sload(&mut state.machine, &state.context.address, handler),
		Opcode::SSTORE => system::sstore(&mut state.machine, &state.context.address, handler),
		Opcode::GAS => system::gas(&mut state.machine, handler),
		Opcode::LOG0 => system::log(&mut state.machine, &state.context.address, 0, handler),
		Opcode::LOG1 => system::log(&mut state.machine, &state.context.address, 1, handler),
		Opcode::LOG2 => system::log(&mut state.machine, &state.context.address, 2, handler),
		Opcode::LOG3 => system::log(&mut state.machine, &state.context.address, 3, handler),
		Opcode::LOG4 => system::log(&mut state.machine, &state.context.address, 4, handler),
		Opcode::SUICIDE => system::suicide(&mut state.machine, &state.context.address, handler),
		Opcode::CREATE => system::create(state, false, handler),
		Opcode::CREATE2 => system::create(state, true, handler),
		Opcode::CALL => system::call(state, CallScheme::Call, handler),
		Opcode::CALLCODE => system::call(state, CallScheme::CallCode, handler),
		Opcode::DELEGATECALL => system::call(state, CallScheme::DelegateCall, handler),
		Opcode::STATICCALL => system::call(state, CallScheme::StaticCall, handler),
		Opcode::CHAINID => system::chainid(&mut state.machine, handler),
		Opcode::BASEFEE => system::base_fee(&mut state.machine, handler),
		_ => handle_other(state, opcode, handler),
	}
}

pub fn system_before_eval<H: Handler>(table: &mut [fn(_: &mut Machine, _: Opcode, _: usize, _: *mut u8) -> evm_core::eval::Control; 256]) {
	fn sha3_wrapper<H: Handler>(machine: &mut Machine, _op: Opcode, _pc: usize, _handler_raw: *mut u8) -> evm_core::eval::Control {
		system::sha3(machine)
	}
	table[Opcode::SHA3.as_usize()] = sha3_wrapper;
}
