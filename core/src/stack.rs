use crate::utils::USIZE_MAX;
use crate::ExitError;
use heapless::Vec as ConstVec;
use primitive_types::{H256, U256};

/// Fixed Stack limit.
pub const STACK_LIMIT: usize = 1024;

/// EVM stack.
#[derive(Clone, Debug)]
pub struct Stack {
	data: ConstVec<U256, STACK_LIMIT>,
}

impl Stack {
	/// Create a new stack with given limit.
	#[must_use]
	pub const fn new(_limit: usize) -> Self {
		Self {
			data: ConstVec::new(),
		}
	}

	/// Stack length.
	#[inline]
	#[must_use]
	pub fn len(&self) -> usize {
		self.data.len()
	}

	/// Whether the stack is empty.
	#[inline]
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.data.is_empty()
	}

	#[must_use]
	pub const fn data(&self) -> &ConstVec<U256, STACK_LIMIT> {
		&self.data
	}

	/// Pop a value from the stack. If the stack is already empty, returns the
	/// `StackUnderflow` error.
	///
	/// # Errors
	/// Return `ExitError::StackUnderflow`
	#[inline]
	pub fn pop(&mut self) -> Result<U256, ExitError> {
		self.data.pop().ok_or(ExitError::StackUnderflow)
	}

	/// Pop `H256` value from the stack.
	///
	/// # Errors
	/// Return `ExitError::StackUnderflow`
	#[inline]
	pub fn pop_h256(&mut self) -> Result<H256, ExitError> {
		self.pop().map(|it| {
			let mut res = H256([0; 32]);
			it.to_big_endian(&mut res.0);
			res
		})
	}

	/// Push a new value into the stack. If it will exceed the stack limit,
	/// returns `StackOverflow` error and leaves the stack unchanged.
	///
	/// # Errors
	/// Return `ExitError`
	#[inline]
	pub fn push(&mut self, value: U256) -> Result<(), ExitError> {
		if self.data.len() + 1 > STACK_LIMIT {
			return Err(ExitError::StackOverflow);
		}
		self.data.push(value).map_err(|_| ExitError::StackOverflow)
	}

	/// Peek a value at given index for the stack, where the top of
	/// the stack is at index `0`. If the index is too large,
	/// `StackError::Underflow` is returned.
	///
	/// # Errors
	/// Return `ExitError::StackUnderflow`
	#[inline]
	pub fn peek(&self, no_from_top: usize) -> Result<U256, ExitError> {
		if self.data.len() > no_from_top {
			Ok(self.data[self.data.len() - no_from_top - 1])
		} else {
			Err(ExitError::StackUnderflow)
		}
	}

	#[inline]
	/// Peek a value at given index for the stack, where the top of
	/// the stack is at index `0`. If the index is too large,
	/// `StackError::Underflow` is returned.
	///
	/// # Return
	/// Returns the value as `H256` from the index.
	///
	/// # Errors
	/// Return `ExitError::StackUnderflow`
	pub fn peek_h256(&self, no_from_top: usize) -> Result<H256, ExitError> {
		self.peek(no_from_top).map(|it| {
			let mut res = H256([0; 32]);
			it.to_big_endian(&mut res.0);
			res
		})
	}

	/// Peek a value at given index for the stack as usize.
	///
	/// If the value is larger than `usize::MAX`, `OutOfGas` error is returned.
	///
	///  # Return
	/// Returns the value as `usize` from the index.
	///
	/// # Errors
	/// Return `ExitError::OutOfGas` or `ExitError::StackUnderflow`
	#[inline]
	pub fn peek_usize(&self, no_from_top: usize) -> Result<usize, ExitError> {
		let u = self.peek(no_from_top)?;
		if u > USIZE_MAX {
			return Err(ExitError::OutOfGas);
		}
		Ok(u.as_usize())
	}

	/// Set a value at given index for the stack, where the top of the
	/// stack is at index `0`. If the index is too large,
	/// `StackError::Underflow` is returned.
	///
	/// # Errors
	/// Return `ExitError::StackUnderflow`
	#[inline]
	pub fn set(&mut self, no_from_top: usize, val: U256) -> Result<(), ExitError> {
		let len = self.data.len();
		if len > no_from_top {
			self.data[len - no_from_top - 1] = val;
			Ok(())
		} else {
			Err(ExitError::StackUnderflow)
		}
	}
}
