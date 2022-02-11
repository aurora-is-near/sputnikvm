#[macro_use]
mod macros;
mod arithmetic;
mod bitwise;
mod misc;

use crate::{ExitError, ExitReason, ExitSucceed, InterpreterHandler, Machine, Opcode};
use core::ops::{BitAnd, BitOr, BitXor};
use primitive_types::{H160, H256, U256};

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Control {
	Continue(usize),
	Exit(ExitReason),
	Jump(usize),
	Trap(Opcode),
}

#[inline]
pub fn eval<H: InterpreterHandler>(
	state: &mut Machine,
	position: usize,
	handler: &mut H,
	address: &H160,
) -> Control {
	#[cfg(feature = "match-interpreter")]
	{
		eval_match(state, position, handler, address)
	}
	#[cfg(not(feature = "match-interpreter"))]
	{
		eval_table(state, position, handler, address)
	}
}

#[inline]
#[cfg(feature = "match-interpreter")]
fn eval_match<'a, H: InterpreterHandler>(
	state: &mut Machine,
	position: usize,
	handler: &mut H,
	address: &H160,
) -> Control {
	let mut pc = position;
	loop {
		let op = match state.code.get(pc) {
			Some(v) => Opcode(*v),
			None => {
				state.position = Err(ExitSucceed::Stopped.into());
				return Control::Exit(ExitSucceed::Stopped.into());
			}
		};
		match handler.before_bytecode(op, pc, state, address) {
			Ok(()) => (),
			Err(e) => {
				state.exit(e.clone().into());
				return Control::Exit(ExitReason::Error(e));
			}
		};
		let control = match op {
			Opcode::ADD => op2_u256_tuple!(state, overflowing_add),
			Opcode::MUL => op2_u256_tuple!(state, overflowing_mul),
			Opcode::SUB => op2_u256_tuple!(state, overflowing_sub),
			Opcode::DIV => op2_u256_fn!(state, self::arithmetic::div),
			Opcode::SDIV => op2_u256_fn!(state, self::arithmetic::sdiv),
			Opcode::EXP => op2_u256_fn!(state, self::arithmetic::exp),
			Opcode::SIGNEXTEND => op2_u256_fn!(state, self::arithmetic::signextend),
			Opcode::LT => op2_u256_bool_ref!(state, lt),
			Opcode::GT => op2_u256_bool_ref!(state, gt),
			Opcode::SLT => op2_u256_fn!(state, self::bitwise::slt),
			Opcode::SGT => op2_u256_fn!(state, self::bitwise::sgt),
			Opcode::EQ => op2_u256_bool_ref!(state, eq),
			Opcode::ISZERO => op1_u256_fn!(state, self::bitwise::iszero),
			Opcode::AND => op2_u256!(state, bitand),
			Opcode::OR => op2_u256!(state, bitor),
			Opcode::XOR => op2_u256!(state, bitxor),
			Opcode::NOT => op1_u256_fn!(state, self::bitwise::not),
			Opcode::BYTE => op2_u256_fn!(state, self::bitwise::byte),
			Opcode::SHL => op2_u256_fn!(state, self::bitwise::shl),
			Opcode::SHR => op2_u256_fn!(state, self::bitwise::shr),
			Opcode::SAR => op2_u256_fn!(state, self::bitwise::sar),
			Opcode::POP => self::misc::pop(state),
			Opcode::PC => self::misc::pc(state, pc),
			Opcode::MSIZE => self::misc::msize(state),
			Opcode::PUSH1 => self::misc::push(state, 1, pc),
			Opcode::PUSH2 => self::misc::push(state, 2, pc),
			Opcode::PUSH3 => self::misc::push(state, 3, pc),
			Opcode::PUSH4 => self::misc::push(state, 4, pc),
			Opcode::PUSH5 => self::misc::push(state, 5, pc),
			Opcode::PUSH6 => self::misc::push(state, 6, pc),
			Opcode::PUSH7 => self::misc::push(state, 7, pc),
			Opcode::PUSH8 => self::misc::push(state, 8, pc),
			Opcode::PUSH9 => self::misc::push(state, 9, pc),
			Opcode::PUSH10 => self::misc::push(state, 10, pc),
			Opcode::PUSH11 => self::misc::push(state, 11, pc),
			Opcode::PUSH12 => self::misc::push(state, 12, pc),
			Opcode::PUSH13 => self::misc::push(state, 13, pc),
			Opcode::PUSH14 => self::misc::push(state, 14, pc),
			Opcode::PUSH15 => self::misc::push(state, 15, pc),
			Opcode::PUSH16 => self::misc::push(state, 16, pc),
			Opcode::PUSH17 => self::misc::push(state, 17, pc),
			Opcode::PUSH18 => self::misc::push(state, 18, pc),
			Opcode::PUSH19 => self::misc::push(state, 19, pc),
			Opcode::PUSH20 => self::misc::push(state, 20, pc),
			Opcode::PUSH21 => self::misc::push(state, 21, pc),
			Opcode::PUSH22 => self::misc::push(state, 22, pc),
			Opcode::PUSH23 => self::misc::push(state, 23, pc),
			Opcode::PUSH24 => self::misc::push(state, 24, pc),
			Opcode::PUSH25 => self::misc::push(state, 25, pc),
			Opcode::PUSH26 => self::misc::push(state, 26, pc),
			Opcode::PUSH27 => self::misc::push(state, 27, pc),
			Opcode::PUSH28 => self::misc::push(state, 28, pc),
			Opcode::PUSH29 => self::misc::push(state, 29, pc),
			Opcode::PUSH30 => self::misc::push(state, 30, pc),
			Opcode::PUSH31 => self::misc::push(state, 31, pc),
			Opcode::PUSH32 => self::misc::push(state, 32, pc),
			Opcode::MOD => op2_u256_fn!(state, self::arithmetic::rem),
			Opcode::SMOD => op2_u256_fn!(state, self::arithmetic::srem),
			Opcode::CODESIZE => self::misc::codesize(state),
			Opcode::CALLDATALOAD => self::misc::calldataload(state),
			Opcode::CALLDATASIZE => self::misc::calldatasize(state),
			Opcode::ADDMOD => op3_u256_fn!(state, self::arithmetic::addmod),
			Opcode::MULMOD => op3_u256_fn!(state, self::arithmetic::mulmod),

			// Instructions with potential exits.
			Opcode::MLOAD => self::misc::mload(state),
			Opcode::MSTORE => self::misc::mstore(state),
			Opcode::MSTORE8 => self::misc::mstore8(state),
			Opcode::CODECOPY => self::misc::codecopy(state),
			Opcode::CALLDATACOPY => self::misc::calldatacopy(state),
			Opcode::DUP1 => self::misc::dup(state, 1),
			Opcode::DUP2 => self::misc::dup(state, 2),
			Opcode::DUP3 => self::misc::dup(state, 3),
			Opcode::DUP4 => self::misc::dup(state, 4),
			Opcode::DUP5 => self::misc::dup(state, 5),
			Opcode::DUP6 => self::misc::dup(state, 6),
			Opcode::DUP7 => self::misc::dup(state, 7),
			Opcode::DUP8 => self::misc::dup(state, 8),
			Opcode::DUP9 => self::misc::dup(state, 9),
			Opcode::DUP10 => self::misc::dup(state, 10),
			Opcode::DUP11 => self::misc::dup(state, 11),
			Opcode::DUP12 => self::misc::dup(state, 12),
			Opcode::DUP13 => self::misc::dup(state, 13),
			Opcode::DUP14 => self::misc::dup(state, 14),
			Opcode::DUP15 => self::misc::dup(state, 15),
			Opcode::DUP16 => self::misc::dup(state, 16),
			Opcode::SWAP1 => self::misc::swap(state, 1),
			Opcode::SWAP2 => self::misc::swap(state, 2),
			Opcode::SWAP3 => self::misc::swap(state, 3),
			Opcode::SWAP4 => self::misc::swap(state, 4),
			Opcode::SWAP5 => self::misc::swap(state, 5),
			Opcode::SWAP6 => self::misc::swap(state, 6),
			Opcode::SWAP7 => self::misc::swap(state, 7),
			Opcode::SWAP8 => self::misc::swap(state, 8),
			Opcode::SWAP9 => self::misc::swap(state, 9),
			Opcode::SWAP10 => self::misc::swap(state, 10),
			Opcode::SWAP11 => self::misc::swap(state, 11),
			Opcode::SWAP12 => self::misc::swap(state, 12),
			Opcode::SWAP13 => self::misc::swap(state, 13),
			Opcode::SWAP14 => self::misc::swap(state, 14),
			Opcode::SWAP15 => self::misc::swap(state, 15),
			Opcode::SWAP16 => self::misc::swap(state, 16),

			// Control flow instructions.
			Opcode::RETURN => self::misc::ret(state),
			Opcode::REVERT => self::misc::revert(state),
			Opcode::INVALID => Control::Exit(ExitError::DesignatedInvalid.into()),
			Opcode::STOP => Control::Exit(ExitSucceed::Stopped.into()),
			Opcode::JUMPDEST => Control::Continue(1),
			Opcode::JUMP => self::misc::jump(state),
			Opcode::JUMPI => self::misc::jumpi(state),

			// External opcodes.
			Opcode(code) => {
				// Skip external instruction.
				state.position = Ok(pc + 1);
				Control::Trap(Opcode(code))
			}
		};
		#[cfg(feature = "tracing")]
		{
			use crate::Capture;
			let result = match &control {
				Control::Continue(_) | Control::Jump(_) => Ok(()),
				Control::Trap(t) => Err(Capture::Trap(t)),
				Control::Exit(e) => Err(Capture::Exit(e)),
			};
			handler.after_bytecode(&result, state);
		}
		pc = match control {
			Control::Continue(bytes) => pc + bytes,
			Control::Jump(pos) => pos,
			_ => return control,
		}
	}
}

#[inline]
// #[cfg(not(feature = "match-interpreter"))]
#[allow(dead_code)]
fn eval_table<H: InterpreterHandler>(
	state: &mut Machine,
	position: usize,
	handler: &mut H,
	address: &H160,
) -> Control {
	static TABLE: [fn(state: &mut Machine, opcode: Opcode, position: usize) -> Control; 256] = {
		fn eval_external(state: &mut Machine, opcode: Opcode, position: usize) -> Control {
			state.position = Ok(position + 1);
			Control::Trap(opcode)
		}
		let mut table = [eval_external as _; 256];
		// Ugly due to https://rust-lang.github.io/rfcs/1558-closure-to-fn-coercion.html
		// not being there.
		macro_rules! table_elem {
			($operation:ident, $definition:expr) => {
				#[allow(non_snake_case)]
				fn $operation(_state: &mut Machine, _opcode: Opcode, _position: usize) -> Control {
					$definition
				}
				table[Opcode::$operation.as_usize()] = $operation as _;
			};
			($operation:ident, $state:ident, $definition:expr) => {
				#[allow(non_snake_case)]
				fn $operation($state: &mut Machine, _opcode: Opcode, _position: usize) -> Control {
					$definition
				}
				table[Opcode::$operation.as_usize()] = $operation as _;
			};
			($operation:ident, $state:ident, $pc:ident, $definition:expr) => {
				#[allow(non_snake_case)]
				fn $operation($state: &mut Machine, _opcode: Opcode, $pc: usize) -> Control {
					$definition
				}
				table[Opcode::$operation.as_usize()] = $operation as _;
			};
		}
		table_elem!(ADD, state, op2_u256_tuple!(state, overflowing_add));
		table_elem!(MUL, state, op2_u256_tuple!(state, overflowing_mul));
		table_elem!(SUB, state, op2_u256_tuple!(state, overflowing_sub));
		table_elem!(DIV, state, op2_u256_fn!(state, self::arithmetic::div));
		table_elem!(SDIV, state, op2_u256_fn!(state, self::arithmetic::sdiv));
		table_elem!(EXP, state, op2_u256_fn!(state, self::arithmetic::exp));
		table_elem!(
			SIGNEXTEND,
			state,
			op2_u256_fn!(state, self::arithmetic::signextend)
		);
		table_elem!(LT, state, op2_u256_bool_ref!(state, lt));
		table_elem!(GT, state, op2_u256_bool_ref!(state, gt));
		table_elem!(SLT, state, op2_u256_fn!(state, self::bitwise::slt));
		table_elem!(SGT, state, op2_u256_fn!(state, self::bitwise::sgt));
		table_elem!(EQ, state, op2_u256_bool_ref!(state, eq));
		table_elem!(ISZERO, state, op1_u256_fn!(state, self::bitwise::iszero));
		table_elem!(AND, state, op2_u256!(state, bitand));
		table_elem!(OR, state, op2_u256!(state, bitor));
		table_elem!(XOR, state, op2_u256!(state, bitxor));
		table_elem!(NOT, state, op1_u256_fn!(state, self::bitwise::not));
		table_elem!(BYTE, state, op2_u256_fn!(state, self::bitwise::byte));
		table_elem!(SHL, state, op2_u256_fn!(state, self::bitwise::shl));
		table_elem!(SHR, state, op2_u256_fn!(state, self::bitwise::shr));
		table_elem!(SAR, state, op2_u256_fn!(state, self::bitwise::sar));
		table_elem!(POP, state, self::misc::pop(state));
		table_elem!(PC, state, position, self::misc::pc(state, position));
		table_elem!(MSIZE, state, self::misc::msize(state));
		table_elem!(PUSH1, state, position, self::misc::push(state, 1, position));
		table_elem!(PUSH2, state, position, self::misc::push(state, 2, position));
		table_elem!(PUSH3, state, position, self::misc::push(state, 3, position));
		table_elem!(PUSH4, state, position, self::misc::push(state, 4, position));
		table_elem!(PUSH5, state, position, self::misc::push(state, 5, position));
		table_elem!(PUSH6, state, position, self::misc::push(state, 6, position));
		table_elem!(PUSH7, state, position, self::misc::push(state, 7, position));
		table_elem!(PUSH8, state, position, self::misc::push(state, 8, position));
		table_elem!(PUSH9, state, position, self::misc::push(state, 9, position));
		table_elem!(
			PUSH10,
			state,
			position,
			self::misc::push(state, 10, position)
		);
		table_elem!(
			PUSH11,
			state,
			position,
			self::misc::push(state, 11, position)
		);
		table_elem!(
			PUSH12,
			state,
			position,
			self::misc::push(state, 12, position)
		);
		table_elem!(
			PUSH13,
			state,
			position,
			self::misc::push(state, 13, position)
		);
		table_elem!(
			PUSH14,
			state,
			position,
			self::misc::push(state, 14, position)
		);
		table_elem!(
			PUSH15,
			state,
			position,
			self::misc::push(state, 15, position)
		);
		table_elem!(
			PUSH16,
			state,
			position,
			self::misc::push(state, 16, position)
		);
		table_elem!(
			PUSH17,
			state,
			position,
			self::misc::push(state, 17, position)
		);
		table_elem!(
			PUSH18,
			state,
			position,
			self::misc::push(state, 18, position)
		);
		table_elem!(
			PUSH19,
			state,
			position,
			self::misc::push(state, 19, position)
		);
		table_elem!(
			PUSH20,
			state,
			position,
			self::misc::push(state, 20, position)
		);
		table_elem!(
			PUSH21,
			state,
			position,
			self::misc::push(state, 21, position)
		);
		table_elem!(
			PUSH22,
			state,
			position,
			self::misc::push(state, 22, position)
		);
		table_elem!(
			PUSH23,
			state,
			position,
			self::misc::push(state, 23, position)
		);
		table_elem!(
			PUSH24,
			state,
			position,
			self::misc::push(state, 24, position)
		);
		table_elem!(
			PUSH25,
			state,
			position,
			self::misc::push(state, 25, position)
		);
		table_elem!(
			PUSH26,
			state,
			position,
			self::misc::push(state, 26, position)
		);
		table_elem!(
			PUSH27,
			state,
			position,
			self::misc::push(state, 27, position)
		);
		table_elem!(
			PUSH28,
			state,
			position,
			self::misc::push(state, 28, position)
		);
		table_elem!(
			PUSH29,
			state,
			position,
			self::misc::push(state, 29, position)
		);
		table_elem!(
			PUSH30,
			state,
			position,
			self::misc::push(state, 30, position)
		);
		table_elem!(
			PUSH31,
			state,
			position,
			self::misc::push(state, 31, position)
		);
		table_elem!(
			PUSH32,
			state,
			position,
			self::misc::push(state, 32, position)
		);
		table_elem!(MOD, state, op2_u256_fn!(state, self::arithmetic::rem));
		table_elem!(SMOD, state, op2_u256_fn!(state, self::arithmetic::srem));
		table_elem!(CODESIZE, state, self::misc::codesize(state));
		table_elem!(CALLDATALOAD, state, self::misc::calldataload(state));
		table_elem!(CALLDATASIZE, state, self::misc::calldatasize(state));
		table_elem!(ADDMOD, state, op3_u256_fn!(state, self::arithmetic::addmod));
		table_elem!(MULMOD, state, op3_u256_fn!(state, self::arithmetic::mulmod));
		table_elem!(MLOAD, state, self::misc::mload(state));
		table_elem!(MSTORE, state, self::misc::mstore(state));
		table_elem!(MSTORE8, state, self::misc::mstore8(state));
		table_elem!(CODECOPY, state, self::misc::codecopy(state));
		table_elem!(CALLDATACOPY, state, self::misc::calldatacopy(state));
		table_elem!(DUP1, state, self::misc::dup(state, 1));
		table_elem!(DUP2, state, self::misc::dup(state, 2));
		table_elem!(DUP3, state, self::misc::dup(state, 3));
		table_elem!(DUP4, state, self::misc::dup(state, 4));
		table_elem!(DUP5, state, self::misc::dup(state, 5));
		table_elem!(DUP6, state, self::misc::dup(state, 6));
		table_elem!(DUP7, state, self::misc::dup(state, 7));
		table_elem!(DUP8, state, self::misc::dup(state, 8));
		table_elem!(DUP9, state, self::misc::dup(state, 9));
		table_elem!(DUP10, state, self::misc::dup(state, 10));
		table_elem!(DUP11, state, self::misc::dup(state, 11));
		table_elem!(DUP12, state, self::misc::dup(state, 12));
		table_elem!(DUP13, state, self::misc::dup(state, 13));
		table_elem!(DUP14, state, self::misc::dup(state, 14));
		table_elem!(DUP15, state, self::misc::dup(state, 15));
		table_elem!(DUP16, state, self::misc::dup(state, 16));
		table_elem!(SWAP1, state, self::misc::swap(state, 1));
		table_elem!(SWAP2, state, self::misc::swap(state, 2));
		table_elem!(SWAP3, state, self::misc::swap(state, 3));
		table_elem!(SWAP4, state, self::misc::swap(state, 4));
		table_elem!(SWAP5, state, self::misc::swap(state, 5));
		table_elem!(SWAP6, state, self::misc::swap(state, 6));
		table_elem!(SWAP7, state, self::misc::swap(state, 7));
		table_elem!(SWAP8, state, self::misc::swap(state, 8));
		table_elem!(SWAP9, state, self::misc::swap(state, 9));
		table_elem!(SWAP10, state, self::misc::swap(state, 10));
		table_elem!(SWAP11, state, self::misc::swap(state, 11));
		table_elem!(SWAP12, state, self::misc::swap(state, 12));
		table_elem!(SWAP13, state, self::misc::swap(state, 13));
		table_elem!(SWAP14, state, self::misc::swap(state, 14));
		table_elem!(SWAP15, state, self::misc::swap(state, 15));
		table_elem!(SWAP16, state, self::misc::swap(state, 16));
		table_elem!(RETURN, state, self::misc::ret(state));
		table_elem!(REVERT, state, self::misc::revert(state));
		table_elem!(INVALID, Control::Exit(ExitError::DesignatedInvalid.into()));
		table_elem!(STOP, Control::Exit(ExitSucceed::Stopped.into()));
		table_elem!(JUMPDEST, Control::Continue(1));
		table_elem!(JUMP, state, self::misc::jump(state));
		table_elem!(JUMPI, state, self::misc::jumpi(state));
		table
	};
	let mut pc = position;
	loop {
		// TODO: we need to optimize fetch loop by extracting raw slice
		// with instructions.
		let op = match state.code.get(pc) {
			Some(v) => Opcode(*v),
			None => {
				state.position = Err(ExitSucceed::Stopped.into());
				return Control::Exit(ExitSucceed::Stopped.into());
			}
		};
		match handler.before_bytecode(op, pc, state, address) {
			Ok(()) => (),
			Err(e) => {
				state.exit(e.clone().into());
				return Control::Exit(ExitReason::Error(e));
			}
		};
		let control = TABLE[op.as_usize()](state, op, pc);

		#[cfg(feature = "tracing")]
		{
			use crate::Capture;
			let result = match &control {
				Control::Continue(_) | Control::Jump(_) => Ok(()),
				Control::Trap(t) => Err(Capture::Trap(*t)),
				Control::Exit(e) => Err(Capture::Exit(e.clone())),
			};
			handler.after_bytecode(&result, state);
		}
		pc = match control {
			Control::Continue(bytes) => pc + bytes,
			Control::Jump(pos) => pos,
			_ => return control,
		}
	}
}
