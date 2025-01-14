use crate::prelude::*;
use crate::utils::USIZE_MAX;
use crate::{ExitError, ExitFatal};
use core::cmp::min;
use core::ops::{BitAnd, Not};
use primitive_types::{H256, U256};

/// A sequential memory. It uses Rust's `Vec` for internal
/// representation.
#[derive(Clone, Debug)]
pub struct Memory {
	/// Memory data
	data: Vec<u8>,
	/// Memory effective length, that changed after resize operations.
	effective_len: usize,
	/// Memory limit
	limit: usize,
}

impl Memory {
	/// Create a new memory with the given limit.
	#[must_use]
	pub const fn new(limit: usize) -> Self {
		Self {
			data: Vec::new(),
			effective_len: 0,
			limit,
		}
	}

	/// Memory limit.
	#[must_use]
	pub const fn limit(&self) -> usize {
		self.limit
	}

	/// Get the length of the current memory range.
	#[must_use]
	pub fn len(&self) -> usize {
		self.data.len()
	}

	/// Get the effective length.
	#[must_use]
	pub const fn effective_len(&self) -> usize {
		self.effective_len
	}

	/// Return true if current effective memory range is zero.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Return the full memory.
	#[must_use]
	pub const fn data(&self) -> &Vec<u8> {
		&self.data
	}

	/// Resize the memory, making it cover the memory region of `offset..offset + len`,
	/// with 32 bytes as the step. If the length is zero, this function does nothing.
	///
	/// # Errors
	/// Return `ExitError::InvalidRange` if `offset + len` is overflow.
	pub fn resize_offset(&mut self, offset: usize, len: usize) -> Result<(), ExitError> {
		if len == 0 {
			return Ok(());
		}

		offset
			.checked_add(len)
			.map_or(Err(ExitError::InvalidRange), |end| self.resize_end(end))
	}

	/// Resize the memory, making it cover to `end`, with 32 bytes as the step.
	///
	/// # Errors
	/// Return `ExitError::InvalidRange` if `end` value is overflow in `next_multiple_of_32` call.
	pub fn resize_end(&mut self, end: usize) -> Result<(), ExitError> {
		if end > self.effective_len {
			let new_end = next_multiple_of_32(end).ok_or(ExitError::InvalidRange)?;
			self.effective_len = new_end;
		}

		Ok(())
	}

	/// Get memory region at given offset.
	///
	/// ## Panics
	///
	/// Value of `size` is considered trusted. If they're too large,
	/// the program can run out of memory, or it can overflow.
	#[must_use]
	pub fn get(&self, mut offset: usize, size: usize) -> Vec<u8> {
		if offset > self.data.len() {
			offset = self.data.len();
		}

		let mut end = offset + size;
		if end > self.data.len() {
			end = self.data.len();
		}

		let mut ret = self.data[offset..end].to_vec();
		ret.resize(size, 0);
		ret
	}

	/// Get `H256` value from a specific offset in memory.
	#[must_use]
	pub fn get_h256(&self, offset: usize) -> H256 {
		let mut ret = [0; 32];

		#[allow(clippy::needless_range_loop)]
		for index in 0..32 {
			let position = offset + index;
			if position >= self.data.len() {
				break;
			}

			ret[index] = self.data[position];
		}

		H256(ret)
	}

	/// Set memory region at given offset. The offset and value is considered
	/// untrusted.
	///
	/// # Errors
	/// Return `ExitFatal::NotSupported` if `offset + target_size` is out of memory limit or overflow.
	pub fn set(
		&mut self,
		offset: usize,
		value: &[u8],
		target_size: Option<usize>,
	) -> Result<(), ExitFatal> {
		let target_size = target_size.unwrap_or(value.len());
		if target_size == 0 {
			return Ok(());
		}

		if offset
			.checked_add(target_size)
			.map_or(true, |pos| pos > self.limit)
		{
			return Err(ExitFatal::NotSupported);
		}

		if self.data.len() < offset + target_size {
			self.data.resize(offset + target_size, 0);
		}

		if target_size > value.len() {
			self.data[offset..((value.len()) + offset)].clone_from_slice(value);
			for index in (value.len())..target_size {
				self.data[offset + index] = 0;
			}
		} else {
			self.data[offset..(target_size + offset)].clone_from_slice(&value[..target_size]);
		}

		Ok(())
	}

	/// Copy memory region form `src` to `dst` with length.
	/// `copy_within` uses `memmove` to avoid `DoS` attacks.
	///
	/// # Errors
	/// Return `ExitFatal::Other`:
	/// - `OverflowOnCopy` if `offset + length` is overflow
	/// - `OutOfGasOnCopy` if `offst_length` out of memory limit
	pub fn copy(
		&mut self,
		src_offset: usize,
		dst_offset: usize,
		length: usize,
	) -> Result<(), ExitFatal> {
		// If length is zero - do nothing
		if length == 0 {
			return Ok(());
		}

		// Get maximum offset
		let offset = core::cmp::max(src_offset, dst_offset);
		let offset_length = offset
			.checked_add(length)
			.ok_or_else(|| ExitFatal::Other(Cow::from("OverflowOnCopy")))?;
		if offset_length > self.limit {
			return Err(ExitFatal::Other(Cow::from("OutOfGasOnCopy")));
		}

		// Resize data memory
		if self.data.len() < offset_length {
			self.data.resize(offset_length, 0);
		}

		self.data
			.copy_within(src_offset..src_offset + length, dst_offset);
		Ok(())
	}

	/// Copy `data` into the memory, of given `len`.
	///
	/// # Errors
	/// Return `ExitFatal::NotSupported` if `set()` call return out of memory limit.
	pub fn copy_large(
		&mut self,
		memory_offset: usize,
		data_offset: U256,
		len: usize,
		data: &[u8],
	) -> Result<(), ExitFatal> {
		// Needed to pass ethereum test defined in
		// https://github.com/ethereum/tests/commit/17f7e7a6c64bb878c1b6af9dc8371b46c133e46d
		// (regardless of other inputs, a zero-length copy is defined to be a no-op).
		// TODO: refactor `set` and `copy_large` (see
		// https://github.com/rust-blockchain/evm/pull/40#discussion_r677180794)
		if len == 0 {
			return Ok(());
		}

		#[allow(clippy::as_conversions)]
		let data = data_offset
			.checked_add(len.into())
			.map_or(&[] as &[u8], |end| {
				if end > USIZE_MAX {
					&[]
				} else {
					let data_offset = data_offset.as_usize();
					let end = end.as_usize();

					if data_offset > data.len() {
						&[]
					} else {
						&data[data_offset..min(end, data.len())]
					}
				}
			});

		self.set(memory_offset, data, Some(len))
	}
}

/// Rounds up `x` to the closest multiple of 32. If `x % 32 == 0` then `x` is returned.
#[inline]
fn next_multiple_of_32(x: usize) -> Option<usize> {
	let r = x.bitand(31).not().wrapping_add(1).bitand(31);
	x.checked_add(r)
}

#[cfg(test)]
mod tests {
	use super::next_multiple_of_32;

	#[test]
	fn test_next_multiple_of_32() {
		// next_multiple_of_32 returns x when it is a multiple of 32
		for i in 0..32 {
			let x = i * 32;
			assert_eq!(Some(x), next_multiple_of_32(x));
		}

		// next_multiple_of_32 rounds up to the nearest multiple of 32 when `x % 32 != 0`
		for x in 0..1024 {
			if x % 32 == 0 {
				continue;
			}
			let next_multiple = x + 32 - (x % 32);
			assert_eq!(Some(next_multiple), next_multiple_of_32(x));
		}

		// next_multiple_of_32 returns None when the next multiple of 32 is too big
		let last_multiple_of_32 = usize::MAX & !31;
		for i in 0..63 {
			let x = usize::MAX - i;
			if x > last_multiple_of_32 {
				assert_eq!(None, next_multiple_of_32(x));
			} else {
				assert_eq!(Some(last_multiple_of_32), next_multiple_of_32(x));
			}
		}
	}
}
