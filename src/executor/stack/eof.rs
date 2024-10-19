#![allow(dead_code)]

use crate::prelude::Vec;
use evm_core::EofDecodeError;

/// `EOFv1` magic number
pub const EOF_MAGIC: &[u8; 2] = &[0xEF, 0x00];

const EOF_VERSION: u8 = 0x01;
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
	pub sum_container_sizes: u16,
}

impl EofHeader {
	/// Decode EOF header from raw bytes.
	pub fn decode(input: &[u8]) -> Result<Self, EofDecodeError> {
		// EOF header first input validation for 7 bytes
		if input.len() < 7 {
			return Err(EofDecodeError::MissingInput);
		}

		let mut header = Self::default();
		// EOF magic [0, 1]: 2 bytes 0xEF00 prefix
		if !input.starts_with(EOF_MAGIC) {
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

		// types_size [4, 5]: 2 bytes 0x0004-0xFFFF - 16-bit unsigned big-endian
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
		let (index, code_sizes, sum) = Self::header_sections_code_size(input, 7)?;
		// more than 1024 code sections are not allowed
		if code_sizes.len() > 0x0400 {
			return Err(EofDecodeError::TooManyCodeSections);
		}
		if code_sizes.len() != usize::from(header.types_size / 4) {
			return Err(EofDecodeError::MismatchCodeAndTypesSize);
		}
		header.code_sizes = code_sizes;
		header.sum_code_sizes = sum;

		// Check index for next element
		if input.len() <= index + 1 {
			return Err(EofDecodeError::MissingInput);
		}

		// `kind_container_or_data` - get from index [7 + code_sizes.len() * 2 + 1]
		// Return last accessed index
		let index = match input[index + 1] {
			KIND_CONTAINER => {
				// `container_sections_sizes`
				let (index, sizes, sum) = Self::header_sections_code_size(input, index + 2)?;
				// the number of container sections may not exceed 256
				if sizes.len() > 0x0100 {
					return Err(EofDecodeError::InvalidNumberContainerSections);
				}
				header.container_sizes = sizes;
				header.sum_container_sizes = sum;

				// Check index for next element
				if input.len() <= index + 1 {
					return Err(EofDecodeError::MissingInput);
				}
				// `kind_data`
				if input[index + 1] != KIND_DATA {
					return Err(EofDecodeError::InvalidDataKind);
				}
				index + 1
			}
			KIND_DATA => index + 1,
			_ => return Err(EofDecodeError::InvalidKindAfterCode),
		};

		// Check index for next elements
		if input.len() <= index + 3 {
			return Err(EofDecodeError::MissingInput);
		}

		// `data_size` [index+1, index+2]: 2 bytes 0x0000-0xFFFF 16-bit - unsigned big-endian
		// integer denoting the length of the data section content (for not yet deployed
		// containers this can be more than the actual content, see Data Section Lifecycle)
		let data_size = get_u16(input, index + 1);
		header.data_size = data_size;

		// `terminator` [index+3]: 1 byte 0x00 marks the end of the EofHeader
		if input[index + 3] != KIND_TERMINAL {
			return Err(EofDecodeError::InvalidTerminalByte);
		}

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
		// Pre-verify input length for next 2 elements
		if input.len() <= index + 1 {
			return Err(EofDecodeError::MissingInput);
		}
		let mut index = index;
		// `num_sections` [index, index+1]: 2 bytes 0x0001-0xFFFF - 16-bit unsigned big-endian integer denoting
		// the number of the sections
		let num_sections = get_u16(input, index);
		index += 1;
		if num_sections == 0 {
			return Err(EofDecodeError::SizesNotFound);
		}
		let num_sections = usize::from(num_sections);
		let byte_size = num_sections * 2;
		// Calculate input length including starting index
		if input.len() <= index + byte_size {
			return Err(EofDecodeError::ShortInputForSizes);
		}
		let mut sizes = Vec::with_capacity(num_sections);
		let mut sum = 0;
		// Fetch sections
		for i in 0..num_sections {
			// `code_size` 2 bytes 0x0001-0xFFFF - 16-bit unsigned big-endian integer
			// denoting the length of the section content. Calculated by index from input.
			let code_size = get_u16(input, index + 1 + i * 2);
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

#[cfg(test)]
mod tests {
	use super::*;
	use evm_core::EofDecodeError;
	use std::iter::once;

	/// Helper function to create a valid `EofHeader` input - first part of header.
	fn create_first_part_header() -> Vec<u8> {
		let types_size = 16_u16.to_be_bytes().to_vec();
		let input: Vec<u8> = EOF_MAGIC
			.iter()
			.copied()
			.chain(once(EOF_VERSION))
			.chain(once(KIND_TYPES))
			.chain(types_size)
			.chain(once(KIND_CODE))
			.collect();
		assert_eq!(input.len(), 7);
		input
	}

	fn create_with_code_sections_sizes(num_sections: u16, sections: Vec<u16>) -> Vec<u8> {
		// Bytes: [First header part] + [num_sections] + [sections]
		create_first_part_header()
			.into_iter()
			.chain(num_sections.to_be_bytes())
			.chain(
				sections
					.into_iter()
					.flat_map(|s| s.to_be_bytes().into_iter()),
			)
			.collect()
	}

	fn create_valid_input(
		num_sections: u16,
		sections: Vec<u16>,
		num_container: u16,
		container_sizes: Vec<u16>,
		data_size: u16,
	) -> Vec<u8> {
		create_with_code_sections_sizes(num_sections, sections)
			.into_iter()
			// Kind container
			.chain(once(KIND_CONTAINER))
			// Container sections count
			.chain(num_container.to_be_bytes())
			// Container sections
			.chain(container_sizes.into_iter().flat_map(u16::to_be_bytes))
			// Data size
			.chain(data_size.to_be_bytes())
			// Terminator
			.chain(once(KIND_TERMINAL))
			.collect()
	}

	/// Test decoding with input too short.
	#[test]
	fn test_decode_input_too_short() {
		let types_size = 16_u16.to_be_bytes().to_vec();
		let mut input = vec![0xEF, 0x00, 0x01, KIND_TYPES];
		input.extend(types_size);
		assert_eq!(input.len(), 6);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	/// Test decoding with invalid EOF magic number.
	#[test]
	fn test_decode_invalid_eof_magic() {
		let mut input = create_first_part_header();
		input[1] = 0x01; // Invalid magic number
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidEOFMagicNumber));
	}

	/// Test decoding with invalid version byte.
	#[test]
	fn test_decode_invalid_version() {
		let mut input = create_first_part_header();
		input[2] = 0x00; // Invalid version
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidEOFVersion));
	}

	/// Test decoding with invalid `kind_types` byte.
	#[test]
	fn test_decode_invalid_kind_types() {
		let mut input = create_first_part_header();
		input[3] = 0; // Invalid kind_types
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidNumberForTypesKind));
	}

	/// Test decoding with `types_size` not a multiple of 4.
	#[test]
	fn test_decode_invalid_types_size() {
		let mut input = create_first_part_header();
		let types_size = 15_u16.to_be_bytes().to_vec();
		input[4] = types_size[0];
		input[5] = types_size[1];
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidTypesSectionSize));
	}

	/// Test decoding with invalid `kind_code` byte.
	#[test]
	fn test_decode_invalid_kind_code() {
		let mut input = create_first_part_header();
		input[6] = 0; // Invalid kind_code
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidNumberForCodeKind));
	}

	/// Test decoding with empty `code_sections_sizes`
	#[test]
	fn test_empty_code_sections_sizes() {
		let input = create_first_part_header();
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	/// Test decoding with zero container sections.
	#[test]
	fn test_decode_zero_container_sections() {
		let input = create_with_code_sections_sizes(0, vec![]);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::SizesNotFound));
	}

	/// Test decoding with short input for code sizes.
	#[test]
	fn test_decode_short_input_for_code_sizes() {
		let input = create_with_code_sections_sizes(1, vec![]);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::ShortInputForSizes));
	}

	/// Test decoding with zero code size.
	#[test]
	fn test_decode_zero_code_size() {
		let input = create_with_code_sections_sizes(1, vec![0]);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::ZeroCodeSize));
	}

	/// Test decoding with too many code sections.
	#[test]
	fn test_decode_too_many_code_sections() {
		let code_size = vec![1_u16; 1025]; // 1025 sections
		let input = create_with_code_sections_sizes(1025, code_size);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::TooManyCodeSections));
	}

	/// Test decoding with mismatched code and types size.
	#[test]
	fn test_decode_mismatch_code_types_size() {
		let input = create_with_code_sections_sizes(3, vec![1, 2, 3]);
		let result = EofHeader::decode(&input);
		// type_size = 16/4, code_sizes = 3
		assert_eq!(result, Err(EofDecodeError::MismatchCodeAndTypesSize));
	}

	#[test]
	fn test_decode_missing_input_after_code_types_size() {
		let input = create_with_code_sections_sizes(4, vec![1, 2, 3, 4]);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	/// Test decoding with invalid kind after code (neither container nor data).
	#[test]
	fn test_decode_invalid_kind_after_code() {
		let mut input = create_with_code_sections_sizes(4, vec![1, 2, 3, 4]);
		input.push(0x05); // Invalid kind
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidKindAfterCode));
	}

	/// Test decoding with valid kind data and missing input.
	#[test]
	fn test_decode_missing_input_after_kind_data() {
		let mut input = create_with_code_sections_sizes(4, vec![1, 2, 3, 4]);
		input.push(KIND_DATA);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	/// Test decoding with invalid terminator byte.
	#[test]
	fn test_decode_invalid_terminator() {
		let mut input = create_with_code_sections_sizes(4, vec![1, 2, 3, 4]);
		input.push(KIND_DATA);
		let data_size = 15_u16.to_be_bytes().to_vec();
		input.extend(data_size);
		input.push(KIND_TERMINAL + 1); // Invalid terminator
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidTerminalByte));
	}

	/// Test successful decoding without container section.
	#[test]
	fn test_decode_invalid_number_container_sections() {
		let input = create_valid_input(4, vec![1, 2, 3, 4], 257, vec![1_u16; 257], 1);
		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidNumberContainerSections));
	}
}
