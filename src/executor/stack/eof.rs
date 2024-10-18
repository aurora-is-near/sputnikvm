#![allow(dead_code)]

use crate::prelude::Vec;
use evm_core::EofDecodeError;

/// `EOFv1` magic number
pub const EOF_MAGIC: &[u8; 2] = &[0xEF, 0x00];

const KIND_TERMINAL: u8 = 0;
const KIND_TYPES: u8 = 1;
const KIND_CODE: u8 = 2;
const KIND_CONTAINER: u8 = 3;
const KIND_DATA: u8 = 4;

/// Get `u16` value from slice by index range: `[index, index+1]`.
/// NOTE: Index range should be valid.
#[inline]
const fn get_u16(input: &[u8], index: usize) -> u16 {
	u16::from_be_bytes([input[index], input[index + 1]])
}

/// EOF Header containing
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EofHeader {
	/// Size of EOF types section.
	/// types section includes num of input and outputs and max stack size.
	pub types_size: u16,
	/// Sizes of EOF code section. Code size can't be zero.
	pub code_sizes: Vec<u16>,
	/// EOF Container size. Container size can be zero.
	pub container_sizes: Vec<u16>,
	/// EOF data size.
	pub data_size: u16,
	/// Sum of code sizes
	pub sum_code_sizes: u16,
	/// Sum of container sizes
	pub sum_container_sizes: usize,
}

impl EofHeader {
	/// Decode EOF header from raw bytes.
	pub fn decode(input: &[u8]) -> Result<Self, EofDecodeError> {
		// EOF header is at least 8 bytes.
		if input.len() < 8 {
			return Err(EofDecodeError::MissingInput);
		}

		let mut header = Self::default();
		// EOF magic [0..1]: 2 bytes 0xEF00 prefix
		if input.starts_with(EOF_MAGIC) {
			return Err(EofDecodeError::InvalidEOFMagicNumber);
		}

		// Version [2]:	1 byte 0x01 EOF version
		if input[2] != 0x01 {
			return Err(EofDecodeError::InvalidEOFVersion);
		}

		// kind_types [3]: 1 byte 0x01 kind marker for types size section
		if input[3] != KIND_TYPES {
			return Err(EofDecodeError::InvalidNumberForTypesKind);
		}

		// types_size [4..5]: 2 bytes 0x0004-0xFFFF - 16-bit unsigned big-endian
		// integer denoting the length of the type section content
		header.types_size = get_u16(input, 4);
		if header.types_size % 4 != 0 {
			return Err(EofDecodeError::InvalidTypesSectionSize);
		}

		// kind_code [6]: 1 byte 0x02 kind marker for code size section
		if input[6] != KIND_CODE {
			return Err(EofDecodeError::InvalidNumberForCodeKind);
		}

		// `code_sections_sizes` - get from index [7]
		let (_index, code_sizes, sum) = Self::header_sections_code_size(input, 7)?;

		// more than 1024 code sections are not allowed
		if code_sizes.len() > 0x0400 {
			return Err(EofDecodeError::TooManyCodeSections);
		}

		if code_sizes.is_empty() {
			return Err(EofDecodeError::InvalidNumberCodeSections);
		}

		if code_sizes.len() != usize::from(header.types_size / 4) {
			return Err(EofDecodeError::MismatchCodeAndTypesSize);
		}

		header.code_sizes = code_sizes;
		header.sum_code_sizes = sum;

		Ok(header)
	}

	/// Get EOF header sections `code_size`, and sum of `code_size`.
	/// Returns:
	/// - last index of input read.
	/// - `Vec<u16>` of code sizes.
	/// - `u16` sum of code sizes.
	#[inline]
	fn header_sections_code_size(
		input: &[u8],
		index: usize,
	) -> Result<(usize, Vec<u16>, u16), EofDecodeError> {
		// Pre-verify input length for index
		if input.len() < index + 2 {
			return Err(EofDecodeError::MissingInput);
		}
		let mut index = index;
		// `num_sections` 2 bytes 0x0001-0xFFFF - 16-bit unsigned big-endian integer denoting
		// the number of the sections
		let num_sections = get_u16(input, index);
		index += 2;
		if num_sections == 0 {
			return Err(EofDecodeError::SizesNotFound);
		}
		let num_sections = usize::from(num_sections);
		let byte_size = num_sections * 2;
		// Calculate input length including starting index
		if input.len() < index + byte_size {
			return Err(EofDecodeError::ShortInputForSizes);
		}
		let mut sizes = Vec::with_capacity(num_sections);
		let mut sum = 0;
		// Fetch sections
		for i in 0..num_sections {
			// `code_size` 2 bytes 0x0001-0xFFFF - 16-bit unsigned big-endian integer
			// denoting the length of the section content. Calculated by index from input.
			let code_size = get_u16(input, index + i * 2);
			if code_size == 0 {
				return Err(EofDecodeError::ZeroCodeSize);
			}
			// `u16::MAX` = 65535, we do not expect overflow here. Code size can't be `> 1024`.
			sum += code_size;
			sizes.push(code_size);
		}

		Ok((index + byte_size, sizes, sum))
	}
}

/// EOF container body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EofBody {
	pub types_section: Vec<TypesSection>,
	pub code_section: Vec<u8>,
	pub container_section: Vec<u8>,
	pub data_section: Vec<u8>,
	pub is_data_filled: bool,
}

/// Types section that contains stack information for matching code section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Copy)]
pub struct TypesSection {
	/// inputs - 1 byte - `0x00-0x7F`: number of stack elements the code section consumes
	pub inputs: u8,
	/// outputs - 1 byte - `0x00-0x80`: number of stack elements the code section returns
	/// or `0x80` for non-returning functions
	pub outputs: u8,
	/// `max_stack_height` - 2 bytes - `0x0000-0x03FF`: maximum number of elements ever
	/// placed onto the stack by the code section
	pub max_stack_size: u16,
}
