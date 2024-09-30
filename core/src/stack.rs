use crate::utils::USIZE_MAX;
use crate::{ExitError, Vec};
use primitive_types::{H256, U256};

/// Fixed Stack limit.
pub const STACK_LIMIT: usize = 1024;

/// EVM stack.
#[derive(Clone, Debug)]
pub struct Stack {
	data: SegmentedStack<2>,
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
		self.data.peek(no_from_top).ok_or(ExitError::StackUnderflow)
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
	segment_21: Option<[U256; N]>,
	segment_22: Option<[U256; N]>,
	segment_23: Option<[U256; N]>,
	segment_24: Option<[U256; N]>,
	segment_25: Option<[U256; N]>,
	segment_26: Option<[U256; N]>,
	segment_27: Option<[U256; N]>,
	segment_28: Option<[U256; N]>,
	segment_29: Option<[U256; N]>,
	segment_30: Option<[U256; N]>,
	segment_31: Option<[U256; N]>,
	segment_32: Option<[U256; N]>,
	segment_33: Option<[U256; N]>,
	segment_34: Option<[U256; N]>,
	segment_35: Option<[U256; N]>,
	segment_36: Option<[U256; N]>,
	segment_37: Option<[U256; N]>,
	segment_38: Option<[U256; N]>,
	segment_39: Option<[U256; N]>,
	segment_40: Option<[U256; N]>,
	segment_41: Option<[U256; N]>,
	segment_42: Option<[U256; N]>,
	segment_43: Option<[U256; N]>,
	segment_44: Option<[U256; N]>,
	segment_45: Option<[U256; N]>,
	segment_46: Option<[U256; N]>,
	segment_47: Option<[U256; N]>,
	segment_48: Option<[U256; N]>,
	segment_49: Option<[U256; N]>,
	segment_50: Option<[U256; N]>,
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
			segment_21: None,
			segment_22: None,
			segment_23: None,
			segment_24: None,
			segment_25: None,
			segment_26: None,
			segment_27: None,
			segment_28: None,
			segment_29: None,
			segment_30: None,
			segment_31: None,
			segment_32: None,
			segment_33: None,
			segment_34: None,
			segment_35: None,
			segment_36: None,
			segment_37: None,
			segment_38: None,
			segment_39: None,
			segment_40: None,
			segment_41: None,
			segment_42: None,
			segment_43: None,
			segment_44: None,
			segment_45: None,
			segment_46: None,
			segment_47: None,
			segment_48: None,
			segment_49: None,
			segment_50: None,
		}
	}

	#[inline]
	fn push(&mut self, value: U256) -> Result<(), ExitError> {
		self.set_at_index(self.index, value)?;
		self.index += 1;
		Ok(())
	}

	#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
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
			i if i < N * 4 => {
				self.segment_4.get_or_insert_with(|| [U256::zero(); N])[i - N * 3] = value;
			}
			i if i < N * 5 => {
				self.segment_5.get_or_insert_with(|| [U256::zero(); N])[i - N * 4] = value;
			}
			i if i < N * 6 => {
				self.segment_6.get_or_insert_with(|| [U256::zero(); N])[i - N * 5] = value;
			}
			i if i < N * 7 => {
				self.segment_7.get_or_insert_with(|| [U256::zero(); N])[i - N * 6] = value;
			}
			i if i < N * 8 => {
				self.segment_8.get_or_insert_with(|| [U256::zero(); N])[i - N * 7] = value;
			}
			i if i < N * 9 => {
				self.segment_9.get_or_insert_with(|| [U256::zero(); N])[i - N * 8] = value;
			}
			i if i < N * 10 => {
				self.segment_10.get_or_insert_with(|| [U256::zero(); N])[i - N * 9] = value;
			}
			i if i < N * 11 => {
				self.segment_11.get_or_insert_with(|| [U256::zero(); N])[i - N * 10] = value;
			}
			i if i < N * 12 => {
				self.segment_12.get_or_insert_with(|| [U256::zero(); N])[i - N * 11] = value;
			}
			i if i < N * 13 => {
				self.segment_13.get_or_insert_with(|| [U256::zero(); N])[i - N * 12] = value;
			}
			i if i < N * 14 => {
				self.segment_14.get_or_insert_with(|| [U256::zero(); N])[i - N * 13] = value;
			}
			i if i < N * 15 => {
				self.segment_15.get_or_insert_with(|| [U256::zero(); N])[i - N * 14] = value;
			}
			i if i < N * 16 => {
				self.segment_16.get_or_insert_with(|| [U256::zero(); N])[i - N * 15] = value;
			}
			i if i < N * 17 => {
				self.segment_17.get_or_insert_with(|| [U256::zero(); N])[i - N * 16] = value;
			}
			i if i < N * 18 => {
				self.segment_18.get_or_insert_with(|| [U256::zero(); N])[i - N * 17] = value;
			}
			i if i < N * 19 => {
				self.segment_19.get_or_insert_with(|| [U256::zero(); N])[i - N * 18] = value;
			}
			i if i < N * 20 => {
				self.segment_20.get_or_insert_with(|| [U256::zero(); N])[i - N * 19] = value;
			}
			i if i < N * 21 => {
				self.segment_21.get_or_insert_with(|| [U256::zero(); N])[i - N * 20] = value;
			}
			i if i < N * 22 => {
				self.segment_22.get_or_insert_with(|| [U256::zero(); N])[i - N * 21] = value;
			}
			i if i < N * 23 => {
				self.segment_23.get_or_insert_with(|| [U256::zero(); N])[i - N * 22] = value;
			}
			i if i < N * 24 => {
				self.segment_24.get_or_insert_with(|| [U256::zero(); N])[i - N * 23] = value;
			}
			i if i < N * 25 => {
				self.segment_25.get_or_insert_with(|| [U256::zero(); N])[i - N * 24] = value;
			}
			i if i < N * 26 => {
				self.segment_26.get_or_insert_with(|| [U256::zero(); N])[i - N * 25] = value;
			}
			i if i < N * 27 => {
				self.segment_27.get_or_insert_with(|| [U256::zero(); N])[i - N * 26] = value;
			}
			i if i < N * 28 => {
				self.segment_28.get_or_insert_with(|| [U256::zero(); N])[i - N * 27] = value;
			}
			i if i < N * 29 => {
				self.segment_29.get_or_insert_with(|| [U256::zero(); N])[i - N * 28] = value;
			}
			i if i < N * 30 => {
				self.segment_30.get_or_insert_with(|| [U256::zero(); N])[i - N * 29] = value;
			}
			i if i < N * 31 => {
				self.segment_31.get_or_insert_with(|| [U256::zero(); N])[i - N * 30] = value;
			}
			i if i < N * 32 => {
				self.segment_32.get_or_insert_with(|| [U256::zero(); N])[i - N * 31] = value;
			}
			i if i < N * 33 => {
				self.segment_33.get_or_insert_with(|| [U256::zero(); N])[i - N * 32] = value;
			}
			i if i < N * 34 => {
				self.segment_34.get_or_insert_with(|| [U256::zero(); N])[i - N * 33] = value;
			}
			i if i < N * 35 => {
				self.segment_35.get_or_insert_with(|| [U256::zero(); N])[i - N * 34] = value;
			}
			i if i < N * 36 => {
				self.segment_36.get_or_insert_with(|| [U256::zero(); N])[i - N * 35] = value;
			}
			i if i < N * 37 => {
				self.segment_37.get_or_insert_with(|| [U256::zero(); N])[i - N * 36] = value;
			}
			i if i < N * 38 => {
				self.segment_38.get_or_insert_with(|| [U256::zero(); N])[i - N * 37] = value;
			}
			i if i < N * 39 => {
				self.segment_39.get_or_insert_with(|| [U256::zero(); N])[i - N * 38] = value;
			}
			i if i < N * 40 => {
				self.segment_40.get_or_insert_with(|| [U256::zero(); N])[i - N * 39] = value;
			}
			i if i < N * 41 => {
				self.segment_41.get_or_insert_with(|| [U256::zero(); N])[i - N * 40] = value;
			}
			i if i < N * 42 => {
				self.segment_42.get_or_insert_with(|| [U256::zero(); N])[i - N * 41] = value;
			}
			i if i < N * 43 => {
				self.segment_43.get_or_insert_with(|| [U256::zero(); N])[i - N * 42] = value;
			}
			i if i < N * 44 => {
				self.segment_44.get_or_insert_with(|| [U256::zero(); N])[i - N * 43] = value;
			}
			i if i < N * 45 => {
				self.segment_45.get_or_insert_with(|| [U256::zero(); N])[i - N * 44] = value;
			}
			i if i < N * 46 => {
				self.segment_46.get_or_insert_with(|| [U256::zero(); N])[i - N * 45] = value;
			}
			i if i < N * 47 => {
				self.segment_47.get_or_insert_with(|| [U256::zero(); N])[i - N * 46] = value;
			}
			i if i < N * 48 => {
				self.segment_48.get_or_insert_with(|| [U256::zero(); N])[i - N * 47] = value;
			}
			i if i < N * 49 => {
				self.segment_49.get_or_insert_with(|| [U256::zero(); N])[i - N * 48] = value;
			}
			i if i < N * 50 => {
				self.segment_50.get_or_insert_with(|| [U256::zero(); N])[i - N * 49] = value;
			}
			_ => return Err(ExitError::StackOverflow),
		}
		Ok(())
	}

	#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
	#[inline]
	fn get_on_index(&self, target_index: usize) -> Option<U256> {
		match target_index {
			i if i < N => self.segment_1.as_ref().map(|segment| segment[i]),
			i if i < N * 2 => self.segment_2.as_ref().map(|segment| segment[i - N]),
			i if i < N * 3 => self.segment_3.as_ref().map(|segment| segment[i - N * 2]),
			i if i < N * 4 => self.segment_4.as_ref().map(|segment| segment[i - N * 3]),
			i if i < N * 5 => self.segment_5.as_ref().map(|segment| segment[i - N * 4]),
			i if i < N * 6 => self.segment_6.as_ref().map(|segment| segment[i - N * 5]),
			i if i < N * 7 => self.segment_7.as_ref().map(|segment| segment[i - N * 6]),
			i if i < N * 8 => self.segment_8.as_ref().map(|segment| segment[i - N * 7]),
			i if i < N * 9 => self.segment_9.as_ref().map(|segment| segment[i - N * 8]),
			i if i < N * 10 => self.segment_10.as_ref().map(|segment| segment[i - N * 9]),
			i if i < N * 11 => self.segment_11.as_ref().map(|segment| segment[i - N * 10]),
			i if i < N * 12 => self.segment_12.as_ref().map(|segment| segment[i - N * 11]),
			i if i < N * 13 => self.segment_13.as_ref().map(|segment| segment[i - N * 12]),
			i if i < N * 14 => self.segment_14.as_ref().map(|segment| segment[i - N * 13]),
			i if i < N * 15 => self.segment_15.as_ref().map(|segment| segment[i - N * 14]),
			i if i < N * 16 => self.segment_16.as_ref().map(|segment| segment[i - N * 15]),
			i if i < N * 17 => self.segment_17.as_ref().map(|segment| segment[i - N * 16]),
			i if i < N * 18 => self.segment_18.as_ref().map(|segment| segment[i - N * 17]),
			i if i < N * 19 => self.segment_19.as_ref().map(|segment| segment[i - N * 18]),
			i if i < N * 20 => self.segment_20.as_ref().map(|segment| segment[i - N * 19]),
			i if i < N * 21 => self.segment_21.as_ref().map(|segment| segment[i - N * 20]),
			i if i < N * 22 => self.segment_22.as_ref().map(|segment| segment[i - N * 21]),
			i if i < N * 23 => self.segment_23.as_ref().map(|segment| segment[i - N * 22]),
			i if i < N * 24 => self.segment_24.as_ref().map(|segment| segment[i - N * 23]),
			i if i < N * 25 => self.segment_25.as_ref().map(|segment| segment[i - N * 24]),
			i if i < N * 26 => self.segment_26.as_ref().map(|segment| segment[i - N * 25]),
			i if i < N * 27 => self.segment_27.as_ref().map(|segment| segment[i - N * 26]),
			i if i < N * 28 => self.segment_28.as_ref().map(|segment| segment[i - N * 27]),
			i if i < N * 29 => self.segment_29.as_ref().map(|segment| segment[i - N * 28]),
			i if i < N * 30 => self.segment_30.as_ref().map(|segment| segment[i - N * 29]),
			i if i < N * 31 => self.segment_31.as_ref().map(|segment| segment[i - N * 30]),
			i if i < N * 32 => self.segment_32.as_ref().map(|segment| segment[i - N * 31]),
			i if i < N * 33 => self.segment_33.as_ref().map(|segment| segment[i - N * 32]),
			i if i < N * 34 => self.segment_34.as_ref().map(|segment| segment[i - N * 33]),
			i if i < N * 35 => self.segment_35.as_ref().map(|segment| segment[i - N * 34]),
			i if i < N * 36 => self.segment_36.as_ref().map(|segment| segment[i - N * 35]),
			i if i < N * 37 => self.segment_37.as_ref().map(|segment| segment[i - N * 36]),
			i if i < N * 38 => self.segment_38.as_ref().map(|segment| segment[i - N * 37]),
			i if i < N * 39 => self.segment_39.as_ref().map(|segment| segment[i - N * 38]),
			i if i < N * 40 => self.segment_40.as_ref().map(|segment| segment[i - N * 39]),
			i if i < N * 41 => self.segment_41.as_ref().map(|segment| segment[i - N * 40]),
			i if i < N * 42 => self.segment_42.as_ref().map(|segment| segment[i - N * 41]),
			i if i < N * 43 => self.segment_43.as_ref().map(|segment| segment[i - N * 42]),
			i if i < N * 44 => self.segment_44.as_ref().map(|segment| segment[i - N * 43]),
			i if i < N * 45 => self.segment_45.as_ref().map(|segment| segment[i - N * 44]),
			i if i < N * 46 => self.segment_46.as_ref().map(|segment| segment[i - N * 45]),
			i if i < N * 47 => self.segment_47.as_ref().map(|segment| segment[i - N * 46]),
			i if i < N * 48 => self.segment_48.as_ref().map(|segment| segment[i - N * 47]),
			i if i < N * 49 => self.segment_49.as_ref().map(|segment| segment[i - N * 48]),
			i if i < N * 50 => self.segment_50.as_ref().map(|segment| segment[i - N * 49]),
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
		let segments = [
			&self.segment_1,
			&self.segment_2,
			&self.segment_3,
			&self.segment_4,
			&self.segment_5,
			&self.segment_6,
			&self.segment_7,
			&self.segment_8,
			&self.segment_9,
			&self.segment_10,
			&self.segment_11,
			&self.segment_12,
			&self.segment_13,
			&self.segment_14,
			&self.segment_15,
			&self.segment_16,
			&self.segment_17,
			&self.segment_18,
			&self.segment_19,
			&self.segment_20,
			&self.segment_21,
			&self.segment_22,
			&self.segment_23,
			&self.segment_24,
			&self.segment_25,
			&self.segment_26,
			&self.segment_27,
			&self.segment_28,
			&self.segment_29,
			&self.segment_30,
			&self.segment_31,
			&self.segment_32,
			&self.segment_33,
			&self.segment_34,
			&self.segment_35,
			&self.segment_36,
			&self.segment_37,
			&self.segment_38,
			&self.segment_39,
			&self.segment_40,
			&self.segment_41,
			&self.segment_42,
			&self.segment_43,
			&self.segment_44,
			&self.segment_45,
			&self.segment_46,
			&self.segment_47,
			&self.segment_48,
			&self.segment_49,
			&self.segment_50,
		];
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
