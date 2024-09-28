use crate::utils::USIZE_MAX;
use crate::{ExitError, Vec};
use primitive_types::{H256, U256};

/// Fixed Stack limit.
pub const STACK_LIMIT: usize = 1024;

/// EVM stack.
#[derive(Clone, Debug)]
pub struct Stack {
	data: SegmentedStack<10>,
}

impl Stack {
	/// Create a new stack with given limit.
	#[must_use]
	pub const fn new(limit: usize) -> Self {
		Self {
			data: SegmentedStack::new(limit),
		}
	}

	/// Stack length.
	#[inline]
	#[must_use]
	pub const fn len(&self) -> usize {
		self.data.get_length()
	}

	/// Whether the stack is empty.
	#[inline]
	#[must_use]
	pub const fn is_empty(&self) -> bool {
		self.data.get_length() == 0
	}

	#[must_use]
	pub const fn get_limit(&self) -> usize {
		self.data.get_limit()
	}

	#[must_use]
	pub fn data(&self) -> Vec<U256> {
		self.data.get_data()
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
		if self.len() + 1 > STACK_LIMIT {
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
		self.data.peek(no_from_top).ok_or(ExitError::OutOfGas)
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
		self.data.set(no_from_top, val)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentedStack<const N: usize> {
	index: usize,
	limit: usize,
	segment_1: Option<[U256; N]>,
	segment_2: Option<[U256; N]>,
	segment_3: Option<[U256; N]>,
	segment_4: Option<[U256; N]>,
	segment_5: Option<[U256; N]>,
	segment_6: Option<[U256; N]>,
	segment_7: Option<[U256; N]>,
	segment_8: Option<[U256; N]>,
	segment_9: Option<[U256; N]>,
	segment_10: Option<[U256; N]>,
	segment_11: Option<[U256; N]>,
	segment_12: Option<[U256; N]>,
	segment_13: Option<[U256; N]>,
	segment_14: Option<[U256; N]>,
	segment_15: Option<[U256; N]>,
	segment_16: Option<[U256; N]>,
	segment_17: Option<[U256; N]>,
	segment_18: Option<[U256; N]>,
	segment_19: Option<[U256; N]>,
	segment_20: Option<[U256; N]>,
}

impl<const N: usize> SegmentedStack<N> {
	#[must_use]
	pub const fn new(limit: usize) -> Self {
		Self {
			index: 0,
			limit,
			segment_1: None,
			segment_2: None,
			segment_3: None,
			segment_4: None,
			segment_5: None,
			segment_6: None,
			segment_7: None,
			segment_8: None,
			segment_9: None,
			segment_10: None,
			segment_11: None,
			segment_12: None,
			segment_13: None,
			segment_14: None,
			segment_15: None,
			segment_16: None,
			segment_17: None,
			segment_18: None,
			segment_19: None,
			segment_20: None,
		}
	}

	#[inline]
	fn push(&mut self, value: U256) -> Result<(), ExitError> {
		self.set_at_index(self.index, value)?;
		self.index += 1;
		Ok(())
	}

	#[inline]
	fn set_at_index(&mut self, target_index: usize, value: U256) -> Result<(), ExitError> {
		match target_index {
			i if i < N => self.segment_1.get_or_insert_with(|| [U256::zero(); N])[i] = value,
			i if i < N * 2 => {
				self.segment_2.get_or_insert_with(|| [U256::zero(); N])[i - N] = value;
			}
			i if i < N * 3 => {
				self.segment_3.get_or_insert_with(|| [U256::zero(); N])[i - N * 2] = value;
			}
			_ => return Err(ExitError::StackOverflow),
		}
		Ok(())
	}

	#[inline]
	fn get_on_index(&self, target_index: usize) -> Option<U256> {
		match target_index {
			i if i < N => self.segment_1.as_ref().map(|segment| segment[i]),
			i if i < N * 2 => self.segment_2.as_ref().map(|segment| segment[i - N]),
			i if i < N * 3 => self.segment_3.as_ref().map(|segment| segment[i - N * 2]),
			_ => None,
		}
	}

	#[inline]
	fn pop(&mut self) -> Option<U256> {
		if self.index == 0 {
			return None;
		}
		self.index -= 1;
		self.get_on_index(self.index)
	}

	fn set(&mut self, no_from_top: usize, value: U256) -> Result<(), ExitError> {
		if no_from_top >= self.index {
			return Err(ExitError::StackUnderflow);
		}
		let target_index = self.index - no_from_top - 1;
		self.set_at_index(target_index, value)?;
		Ok(())
	}

	fn peek(&self, no_from_top: usize) -> Option<U256> {
		if no_from_top >= self.index {
			return None;
		}
		let target_index = self.index - no_from_top - 1;
		self.get_on_index(target_index)
	}

	fn get_data(&self) -> Vec<U256> {
		let mut data = Vec::new();
		let segments = [&self.segment_1, &self.segment_2, &self.segment_3];
		for (i, segment) in segments.iter().enumerate() {
			if let Some(ref segment) = segment {
				let len = (self.index - i * N).min(N);
				if len > 0 {
					data.extend_from_slice(&segment[0..len]);
				}
			}
		}

		data
	}

	#[inline]
	const fn get_length(&self) -> usize {
		self.index
	}

	#[inline]
	const fn get_limit(&self) -> usize {
		self.limit
	}
}
