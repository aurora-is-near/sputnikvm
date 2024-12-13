//! # SWAPN, DUPN and EXCHANGE instructions
//! EIP-663 Introduce additional instructions for manipulating the
//! stack which allow accessing the stack at higher depths.
//!
//! [EIP-663](https://eips.ethereum.org/EIPS/eip-663)

use crate::eval::Control;
use crate::{ExitError, Handler, Runtime};

pub fn dupn<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	require_eof!(runtime);
	// Immediate value after the instruction.
	let raw_offset = try_or_fail!(runtime.machine.get_code_and_inc_pc(1));
	let imm = usize::from(raw_offset[0]);
	let value = match runtime.machine.stack().peek(imm + 1) {
		Ok(value) => value,
		Err(e) => return Control::Exit(e.into()),
	};

	push_u256!(runtime, value);
	Control::Continue
}

pub fn swapn<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	require_eof!(runtime);
	let value1 = match runtime.machine.stack().peek(0) {
		Ok(value) => value,
		Err(e) => return Control::Exit(e.into()),
	};

	// Immediate value after the instruction.
	let imm = try_or_fail!(runtime.machine.get_code_and_inc_pc(1));
	let n = usize::from(imm[0]) + 1;

	let value2 = match runtime.machine.stack().peek(n) {
		Ok(value) => value,
		Err(e) => return Control::Exit(e.into()),
	};

	match runtime.machine.stack_mut().set(0, value2) {
		Ok(()) => (),
		Err(e) => return Control::Exit(e.into()),
	}
	match runtime.machine.stack_mut().set(n, value1) {
		Ok(()) => (),
		Err(e) => return Control::Exit(e.into()),
	}

	Control::Continue
}

pub fn exchange<H: Handler>(runtime: &mut Runtime) -> Control<H> {
	require_eof!(runtime);
	// Immediate value after the instruction.
	let raw_imm = try_or_fail!(runtime.machine.get_code_and_inc_pc(1));
	let imm = usize::from(raw_imm[0]);
	let n = (imm >> 4) + 1;
	let m = (imm & 0x0F) + 1;

	let value1 = match runtime.machine.stack().peek(n) {
		Ok(value) => value,
		Err(e) => return Control::Exit(e.into()),
	};
	let value2 = match runtime.machine.stack().peek(n + m) {
		Ok(value) => value,
		Err(e) => return Control::Exit(e.into()),
	};

	match runtime.machine.stack_mut().set(n, value2) {
		Ok(()) => (),
		Err(e) => return Control::Exit(e.into()),
	}
	match runtime.machine.stack_mut().set(n + m, value1) {
		Ok(()) => (),
		Err(e) => return Control::Exit(e.into()),
	}

	Control::Continue
}

#[cfg(test)]
mod tests {
	use crate::eval::eof::mock::{create_eof, init_runtime, MockHandler};
	use crate::eval::eof::stack::{dupn, exchange, swapn};
	use crate::eval::Control;
	use evm_core::{ExitError, ExitReason};
	use primitive_types::U256;

	#[test]
	fn test_dupn_not_eof() {
		let mut runtime = init_runtime(vec![], None);

		let control = dupn::<MockHandler>(&mut runtime);
		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::EOFOpcodeDisabledInLegacy))
		));
	}

	#[test]
	fn test_dupn_success() {
		let code = vec![0x05, 0xAC, 0xCF, 0xFE];
		let expected_data = U256::from(0xAD);

		let mut runtime = init_runtime(code, Some(create_eof(vec![])));
		for _ in 0..9 {
			runtime.machine.stack_mut().push(U256::from(1)).unwrap();
		}
		let len = runtime.machine.stack().len();
		runtime
			.machine
			.stack_mut()
			.set(len - 5, expected_data)
			.unwrap();

		let control = dupn::<MockHandler>(&mut runtime);
		let res = runtime.machine.stack().peek(5).unwrap();

		assert!(matches!(control, Control::Continue));
		assert_eq!(res, expected_data);
	}

	#[test]
	fn test_dupn_stack_underflow() {
		let code = vec![0x0B, 0xAC, 0xCF, 0xFE];

		let mut runtime = init_runtime(code, Some(create_eof(vec![])));
		for _ in 0..9 {
			runtime.machine.stack_mut().push(U256::from(1)).unwrap();
		}

		let control = dupn::<MockHandler>(&mut runtime);

		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::StackUnderflow))
		));
	}

	#[test]
	fn test_swapn_not_eof() {
		let mut runtime = init_runtime(vec![], None);

		let control = swapn::<MockHandler>(&mut runtime);
		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::EOFOpcodeDisabledInLegacy))
		));
	}

	#[test]
	fn test_swapn_success() {
		let code = vec![0x05, 0xAC, 0xCF, 0xFE];
		let expected_data1 = U256::from(0xAD);
		let expected_data2 = U256::from(0xE3);

		let mut runtime = init_runtime(code, Some(create_eof(vec![])));
		for _ in 0..9 {
			runtime.machine.stack_mut().push(U256::from(1)).unwrap();
		}

		runtime.machine.stack_mut().set(0, expected_data1).unwrap();
		runtime.machine.stack_mut().set(6, expected_data2).unwrap();

		let control = swapn::<MockHandler>(&mut runtime);
		let res1 = runtime.machine.stack().peek(0).unwrap();
		let res2 = runtime.machine.stack().peek(6).unwrap();

		assert!(matches!(control, Control::Continue));
		assert_eq!(res1, expected_data2);
		assert_eq!(res2, expected_data1);
	}

	#[test]
	fn test_swapn_stack_underflow() {
		let code = vec![0x0B, 0xAC, 0xCF, 0xFE];
		let expected_data1 = U256::from(0xAD);
		let expected_data2 = U256::from(0xE3);

		let mut runtime = init_runtime(code, Some(create_eof(vec![])));
		for _ in 0..9 {
			runtime.machine.stack_mut().push(U256::from(1)).unwrap();
		}

		runtime.machine.stack_mut().set(0, expected_data1).unwrap();
		runtime.machine.stack_mut().set(6, expected_data2).unwrap();

		let control = swapn::<MockHandler>(&mut runtime);
		let res1 = runtime.machine.stack().peek(0).unwrap();
		let res2 = runtime.machine.stack().peek(6).unwrap();

		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::StackUnderflow))
		));
		assert_eq!(res1, expected_data1);
		assert_eq!(res2, expected_data2);
	}

	#[test]
	fn test_exchange_not_eof() {
		let mut runtime = init_runtime(vec![], None);

		let control = exchange::<MockHandler>(&mut runtime);
		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::EOFOpcodeDisabledInLegacy))
		));
	}

	#[test]
	fn test_exchange_success() {
		let code = vec![0x05, 0xAC, 0xCF, 0xFE];
		let expected_data1 = U256::from(0xAD);
		let expected_data2 = U256::from(0xE3);

		let mut runtime = init_runtime(code, Some(create_eof(vec![])));
		for _ in 0..9 {
			runtime.machine.stack_mut().push(U256::from(1)).unwrap();
		}

		runtime.machine.stack_mut().set(1, expected_data1).unwrap();
		runtime.machine.stack_mut().set(6, expected_data2).unwrap();

		let control = exchange::<MockHandler>(&mut runtime);
		let res1 = runtime.machine.stack().peek(6).unwrap();
		let res2 = runtime.machine.stack().peek(7).unwrap();

		assert!(matches!(control, Control::Continue));
		assert_eq!(res1, expected_data2);
		assert_eq!(res2, expected_data1);
	}

	#[test]
	fn test_exchange_stack_underflow() {
		let code = vec![0x0B, 0xAC, 0xCF, 0xFE];
		let expected_data1 = U256::from(0xAD);
		let expected_data2 = U256::from(0xE3);

		let mut runtime = init_runtime(code, Some(create_eof(vec![])));
		for _ in 0..9 {
			runtime.machine.stack_mut().push(U256::from(1)).unwrap();
		}

		runtime.machine.stack_mut().set(1, expected_data1).unwrap();
		runtime.machine.stack_mut().set(6, expected_data2).unwrap();

		let control = exchange::<MockHandler>(&mut runtime);
		let res1 = runtime.machine.stack().peek(1).unwrap();
		let res2 = runtime.machine.stack().peek(6).unwrap();

		assert!(matches!(
			control,
			Control::Exit(ExitReason::Error(ExitError::StackUnderflow))
		));
		assert_eq!(res1, expected_data1);
		assert_eq!(res2, expected_data2);
	}
}
