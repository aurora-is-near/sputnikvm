//! # EOF - EVM Object Format v1
//!
//! - [EIP-3540](https://eips.ethereum.org/EIPS/eip-3540)
//! - [EIP-4750 Specification: Type Section](https://eips.ethereum.org/EIPS/eip-4750#type-section)
#![allow(clippy::module_name_repetitions)]

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
#[must_use]
pub const fn get_u16(input: &[u8], index: usize) -> u16 {
	u16::from_be_bytes([input[index], input[index + 1]])
}

/// EVM Object Format (EOF) container.
///
/// It consists of a header, body.
/// <https://eips.ethereum.org/EIPS/eip-3540#container>
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Eof {
	pub header: EofHeader,
	pub body: EofBody,
}

impl Eof {
	/// Decode EOF from raw bytes.
	///
	/// ## Errors
	/// Returns EOF decode error
	pub fn decode(input: &[u8]) -> Result<Self, EofDecodeError> {
		let header = EofHeader::decode(input)?;
		let body = EofBody::decode(input, &header)?;
		Ok(Self { header, body })
	}

	/// Decode EOF that have additional surplus bytes.
	///
	/// ## Errors
	/// Returns EOF decode error with surplus
	pub fn decode_surplus(input: &[u8]) -> Result<(Self, Vec<u8>), EofDecodeError> {
		let header = EofHeader::decode(input)?;
		let eof_size = header.body_size() + header.size();
		if input.len() < eof_size {
			return Err(EofDecodeError::MissingInput);
		}
		let (input, surplus_data) = input.split_at(eof_size);
		let body = EofBody::decode(input, &header)?;
		Ok((Self { header, body }, surplus_data.to_vec()))
	}

	/// Returns a slice of the raw bytes.
	/// If offset is greater than the length of the raw bytes, an empty slice is returned.
	/// If len is greater than the length of the raw bytes, the slice is truncated to the length of the raw bytes.
	#[must_use]
	pub fn data_slice(&self, offset: usize, len: usize) -> &[u8] {
		self.body
			.data_section
			.get(offset..)
			.and_then(|bytes| bytes.get(..core::cmp::min(len, bytes.len())))
			.unwrap_or_default()
	}
}

/// EOF Header containing
/// <https://eips.ethereum.org/EIPS/eip-3540#header>
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
	/// Stored header size in bytes. Not part of EOF Header.
	pub(crate) header_size: usize,
}

impl EofHeader {
	/// Decode EOF header from raw bytes.
	///
	/// ## Errors
	/// Returns EOF header decode error
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
		if input[2] != EOF_VERSION {
			return Err(EofDecodeError::InvalidEOFVersion);
		}

		// kind_types [3]: 1 byte 0x01 kind marker for types size section
		if input[3] != KIND_TYPES {
			return Err(EofDecodeError::InvalidNumberForTypesKind);
		}

		// types_size [4, 5]: 2 bytes 0x0004-0x1000 - 16-bit unsigned big-endian
		// integer denoting the length of the type section content.
		// Validation: the number of code sections must be equal to types_size / 4
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
		// Validation: the number of code sections must be equal to `types_size / 4` = 0x0400 (1024)
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

		// Set header size, it's the number after last index of input read.
		header.header_size = index + 4;

		Ok(header)
	}

	/// Size of EOF header in bytes.
	#[must_use]
	pub const fn size(&self) -> usize {
		self.header_size
	}

	/// Returns body size. It is sum of code sizes, container sizes and data size.
	#[must_use]
	pub fn body_size(&self) -> usize {
		usize::from(
			self.types_size + self.sum_code_sizes + self.sum_container_sizes + self.data_size,
		)
	}

	/// Returns number of types.
	#[must_use]
	pub fn types_count(&self) -> usize {
		usize::from(self.types_size / 4)
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
/// <https://eips.ethereum.org/EIPS/eip-3540#body>
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EofBody {
	pub types_section: Vec<TypesSection>,
	pub code_section: Vec<u8>,
	pub container_section: Vec<u8>,
	pub data_section: Vec<u8>,
	pub is_data_filled: bool,
}

impl EofBody {
	/// Decode EOF container body from the given buffer and header.
	///
	/// ## Errors
	/// Returns EOF decode body error
	pub fn decode(input: &[u8], header: &EofHeader) -> Result<Self, EofDecodeError> {
		let header_len = header.size();
		let partial_body_len =
			usize::from(header.types_size + header.sum_code_sizes + header.sum_container_sizes);
		let full_body_len = partial_body_len + usize::from(header.data_size);

		if input.len() < header_len + partial_body_len {
			return Err(EofDecodeError::MissingBodyWithoutData);
		}

		if input.len() > header_len + full_body_len {
			return Err(EofDecodeError::DanglingData);
		}

		// Start `types_index` for input data after header
		let mut index = header_len;
		let mut types_section = Vec::new();
		for _ in 0..header.types_count() {
			// We pass not accessed yet index
			let types_section_data = TypesSection::decode(index, input)?;
			// Next not accessed index after 4 bytes
			index += 4;
			types_section.push(types_section_data);
		}

		// Extract code section form input
		let mut code_section = Vec::new();
		for size in header.code_sizes.iter().map(|x| usize::from(*x)) {
			code_section.extend(&input[index..index + size]);
			// Next not accessed index after `size` bytes
			index += size;
		}

		// Extract container section
		let mut container_section = Vec::new();
		for size in header.container_sizes.iter().map(|x| usize::from(*x)) {
			container_section.extend(&input[index..index + size]);
			// Next not accessed index after `size` bytes
			index += size;
		}

		let data_section = input[index..].to_vec();
		let is_data_filled = data_section.len() == usize::from(header.data_size);

		Ok(Self {
			types_section,
			code_section,
			container_section,
			data_section,
			is_data_filled,
		})
	}

	/// Decode types section from input.
	#[allow(dead_code)]
	fn decode_types_section(_input: &[u8]) -> Result<Vec<TypesSection>, EofDecodeError> {
		todo!()
		/*
		let mut types_section = Vec::new();
		let mut index = 0;
		while index < input.len() {
			// inputs - 1 byte - `0x00-0x7F`: number of stack elements the code section consumes
			let inputs = input[index];
			index += 1;
			// outputs - 1 byte - `0x00-0x80`: number of stack elements the code section returns
			// or `0x80` for non-returning functions
			let outputs = input[index];
			index += 1;
			// `max_stack_height` - 2 bytes - `0x0000-0x03FF`: maximum number of elements ever
			// placed onto the stack by the code section
			let max_stack_size = get_u16(input, index);
			index += 2;
			types_section.push(TypesSection {
				inputs,
				outputs,
				max_stack_size,
			});
		}
		Ok(types_section)
		*/
	}
}

/// Types section that contains stack information for matching code section.
/// [EIP-4750 Specification: Type Section](https://eips.ethereum.org/EIPS/eip-4750#type-section)
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

impl TypesSection {
	/// Decode the section for the input from index.
	/// NOTE: Input data length should be already re-verified
	///
	/// ## Errors
	/// Returns EOF decode error
	#[inline]
	pub fn decode(index: usize, input: &[u8]) -> Result<Self, EofDecodeError> {
		let inputs = input[index];
		let outputs = input[index + 1];
		let max_stack_size = get_u16(input, index + 2);

		// Validate the section
		if inputs > 0x7f || outputs > 0x80 || max_stack_size > 0x03FF {
			return Err(EofDecodeError::InvalidTypesSection);
		}
		if u16::from(inputs) > max_stack_size {
			return Err(EofDecodeError::InvalidTypesSection);
		}
		Ok(Self {
			inputs,
			outputs,
			max_stack_size,
		})
	}
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
			// Sey kind data flag
			.chain(once(KIND_DATA))
			// Data size
			.chain(data_size.to_be_bytes())
			// Terminator
			.chain(once(KIND_TERMINAL))
			.collect()
	}

	fn create_header_and_body_input() -> (Vec<u8>, EofHeader) {
		let header = EofHeader {
			types_size: 8,
			code_sizes: vec![2, 4],
			container_sizes: vec![1, 3],
			data_size: 3,
			sum_code_sizes: 6,
			sum_container_sizes: 4,
			header_size: 24,
		};
		let input = vec![
			0xEF, 0x00, 0x01, // HEADER: meta information
			0x01, 0x00, 0x08, // Types size
			0x02, 0x00, 0x02, // Code size
			0x00, 0x02, 0x00, 0x04, // Code section
			0x03, 0x00, 0x02, // Container size
			0x00, 0x01, 0x00, 0x03, // Container section
			0x04, 0x00, 0x03, // Data size
			0x00, // Terminator
			0x1A, 0x0C, 0x01, 0xFD, // BODY: types section data [1]
			0x3E, 0x6D, 0x02, 0x9A, // types section data [2]
			0xA9, 0xE0, // Code size data [1]
			0xCF, 0x39, 0x8A, 0x3B, // Code size data [2]
			0xB8, // Container size data [1]
			0xE7, 0xB3, 0x7C, // Container size data [2]
			0x3B, 0x5F, 0xE3, // Data size data
		];
		let decoded_header = EofHeader::decode(&input);
		assert_eq!(Ok(header.clone()), decoded_header);
		(input, header)
	}

	#[test]
	fn test_decode_input_too_short() {
		let types_size = 16_u16.to_be_bytes().to_vec();
		let input: Vec<u8> = vec![0xEF, 0x00, 0x01, KIND_TYPES]
			.into_iter()
			.chain(types_size)
			.collect();
		assert_eq!(input.len(), 6);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	#[test]
	fn test_decode_invalid_eof_magic() {
		let mut input = create_first_part_header();
		// Set invalid magic number
		input[1] = 0x01;

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidEOFMagicNumber));
	}

	#[test]
	fn test_decode_invalid_version() {
		let mut input = create_first_part_header();
		// Set invalid version
		input[2] = 0x00;

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidEOFVersion));
	}

	#[test]
	fn test_decode_invalid_kind_types() {
		let mut input = create_first_part_header();
		// Set invalid kind_types
		input[3] = 0;

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidNumberForTypesKind));
	}

	#[test]
	fn test_decode_invalid_types_size() {
		let mut input = create_first_part_header();
		let types_size = 15_u16.to_be_bytes().to_vec();
		input[4] = types_size[0];
		input[5] = types_size[1];

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidTypesSectionSize));
	}

	#[test]
	fn test_decode_invalid_kind_code() {
		let mut input = create_first_part_header();
		// Set invalid kind_code
		input[6] = 0;

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidNumberForCodeKind));
	}

	#[test]
	fn test_empty_code_sections_sizes() {
		let input = create_first_part_header();

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	#[test]
	fn test_decode_zero_code_sections() {
		let input = create_with_code_sections_sizes(0, vec![]);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::SizesNotFound));
	}

	#[test]
	fn test_decode_short_input_for_code_sizes() {
		let input = create_with_code_sections_sizes(1, vec![]);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::ShortInputForSizes));
	}

	#[test]
	fn test_decode_zero_code_size() {
		let input = create_with_code_sections_sizes(1, vec![0]);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::ZeroCodeSize));
	}

	#[test]
	fn test_decode_too_many_code_sections() {
		let code_size = vec![1_u16; 1025]; // 1025 sections
		let input = create_with_code_sections_sizes(1025, code_size);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::TooManyCodeSections));
	}

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

	#[test]
	fn test_decode_invalid_kind_after_code() {
		let mut input = create_with_code_sections_sizes(4, vec![1, 2, 3, 4]);
		// Set invalid kind
		input.push(0x05);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidKindAfterCode));
	}

	#[test]
	fn test_decode_missing_input_after_kind_data() {
		let mut input = create_with_code_sections_sizes(4, vec![1, 2, 3, 4]);
		input.push(KIND_DATA);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	#[test]
	fn test_decode_invalid_terminator() {
		let data_size = 15_u16.to_be_bytes().to_vec();
		let input: Vec<u8> = create_with_code_sections_sizes(4, vec![1, 2, 3, 4])
			.iter()
			.copied()
			.chain(once(KIND_DATA))
			.chain(data_size)
			// Set invalid terminator
			.chain(once(KIND_TERMINAL + 1))
			.collect();

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidTerminalByte));
	}

	#[test]
	fn test_decode_invalid_number_container_sections() {
		let input = create_valid_input(4, vec![1, 2, 3, 4], 257, vec![1_u16; 257], 1);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidNumberContainerSections));
	}

	#[test]
	fn test_decode_invalid_container_sections_kind_data_empty() {
		let mut input = create_valid_input(4, vec![1, 2, 3, 4], 4, vec![1, 2, 3, 4], 1);
		// Remove input with KIND_DATA flag
		input.truncate(input.len() - 4);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	#[test]
	fn test_decode_invalid_container_sections_empty_data_size() {
		let mut input = create_valid_input(4, vec![1, 2, 3, 4], 4, vec![1, 2, 3, 4], 1);
		// Remove input with data_size value
		input.truncate(input.len() - 3);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	#[test]
	fn test_decode_invalid_container_sections_wrong_terminal() {
		let input = create_valid_input(4, vec![1, 2, 3, 4], 4, vec![1, 2, 3, 4], 1);
		// Change input KIND_TERMINAL value
		let input: Vec<u8> = input[..input.len() - 1]
			.iter()
			.copied()
			.chain(once(KIND_TERMINAL + 1))
			.collect();

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::InvalidTerminalByte));
	}

	#[test]
	fn test_empty_container_sections() {
		let mut input = create_valid_input(4, vec![2, 3, 4, 5], 0, vec![], 3);
		input.truncate(input.len() - 5);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::MissingInput));
	}

	#[test]
	fn test_empty_container_sections_sizes() {
		let mut input = create_valid_input(4, vec![2, 3, 4, 5], 0, vec![], 3);
		input.truncate(input.len() - 4);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::SizesNotFound));
	}

	#[test]
	fn test_decode_short_input_for_container_sizes() {
		let mut input = create_valid_input(4, vec![2, 3, 4, 5], 1, vec![], 3);
		input.truncate(input.len() - 4);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::ShortInputForSizes));
	}

	#[test]
	fn test_decode_zero_container_size() {
		let input = create_valid_input(4, vec![2, 3, 4, 5], 1, vec![0], 3);

		let result = EofHeader::decode(&input);
		assert_eq!(result, Err(EofDecodeError::ZeroCodeSize));
	}

	#[test]
	fn test_success_decode_with_container_sections() {
		let input = create_valid_input(4, vec![2, 3, 4, 5], 4, vec![1, 2, 3, 4], 3);
		let expected = EofHeader {
			types_size: 16,
			code_sizes: vec![2, 3, 4, 5],
			sum_code_sizes: 14,
			container_sizes: vec![1, 2, 3, 4],
			sum_container_sizes: 10,
			data_size: 3,
			header_size: 32,
		};

		let result = EofHeader::decode(&input).expect("Decode failed");
		assert_eq!(result, expected);
	}

	#[test]
	fn test_type_sections_decode_success() {
		let input = vec![0x01, 0x02, 0x00, 0x03];

		let result = TypesSection::decode(0, &input).expect("Decode should succeed");
		assert_eq!(
			result,
			TypesSection {
				inputs: 0x01,
				outputs: 0x02,
				max_stack_size: 0x0003,
			}
		);
	}

	#[test]
	fn test_type_sections_decode_invalid_inputs() {
		let input = vec![0x80, 0x02, 0x00, 0x03];

		let result = TypesSection::decode(0, &input);
		assert_eq!(result, Err(EofDecodeError::InvalidTypesSection));
	}

	#[test]
	fn test_type_sections_decode_invalid_outputs() {
		let input = vec![0x01, 0x81, 0x00, 0x03];

		let result = TypesSection::decode(0, &input);
		assert_eq!(result, Err(EofDecodeError::InvalidTypesSection));
	}

	#[test]
	fn test_type_sections_decode_invalid_max_stack_size() {
		let input = vec![0x01, 0x02, 0x04, 0x00];

		let result = TypesSection::decode(0, &input);
		assert_eq!(result, Err(EofDecodeError::InvalidTypesSection));
	}

	#[test]
	fn test_type_sections_decode_inputs_greater_than_max_stack_size() {
		let input = vec![0x05, 0x02, 0x00, 0x03];

		let result = TypesSection::decode(0, &input);
		assert_eq!(result, Err(EofDecodeError::InvalidTypesSection));
	}

	#[test]
	fn test_eof_body_decode_success() {
		let (input, header) = create_header_and_body_input();

		let body = EofBody::decode(&input, &header).expect("Decode EOF body");
		assert_eq!(
			body.types_section,
			vec![
				TypesSection {
					inputs: 0x1A,
					outputs: 0x0C,
					max_stack_size: 0x01FD
				},
				TypesSection {
					inputs: 0x3E,
					outputs: 0x6D,
					max_stack_size: 0x029A
				}
			]
		);
		assert_eq!(body.code_section, vec![0xA9, 0xE0, 0xCF, 0x39, 0x8A, 0x3B]);
		assert_eq!(body.container_section, vec![0xB8, 0xE7, 0xB3, 0x7C]);
		assert_eq!(body.data_section, vec![0x3B, 0x5F, 0xE3]);
		assert!(body.is_data_filled);
	}

	#[test]
	fn test_eof_body_decode_missing_body_without_data() {
		let (mut input, header) = create_header_and_body_input();
		input.truncate(header.size() + usize::from(header.sum_code_sizes));

		let body = EofBody::decode(&input, &header);
		assert!(matches!(body, Err(EofDecodeError::MissingBodyWithoutData)));
	}

	#[test]
	fn test_eof_body_decode_dangling_data() {
		let (mut input, header) = create_header_and_body_input();
		input.push(0xC3);

		let body = EofBody::decode(&input, &header);
		assert!(matches!(body, Err(EofDecodeError::DanglingData)));
	}

	#[test]
	fn test_eof_body_decode_invalid_types_section() {
		let (mut input, header) = create_header_and_body_input();
		input[header.header_size] = 0x9A;

		let body = EofBody::decode(&input, &header);
		assert!(matches!(body, Err(EofDecodeError::InvalidTypesSection)));
	}

	#[test]
	fn test_eof_decode_surplus_success() {
		let (input, header) = create_header_and_body_input();
		let input = input
			.into_iter()
			.chain(vec![0xA3, 0x3E, 0xB5])
			.collect::<Vec<u8>>();

		let (eof, surplus) = Eof::decode_surplus(&input).expect("Decode EOF with surplus");
		assert_eq!(eof.header, header);
		assert_eq!(surplus, vec![0xA3, 0x3E, 0xB5]);
	}

	#[test]
	fn test_eof_decode_surplus_missing_input() {
		let (mut input, _) = create_header_and_body_input();
		input.truncate(input.len() - 1);

		let eof = Eof::decode_surplus(&input);
		assert!(matches!(eof, Err(EofDecodeError::MissingInput)));
	}

	#[test]
	fn test_data_slice_within_bounds() {
		let eof_body = EofBody {
			data_section: vec![1, 2, 3, 4, 5],
			..Default::default()
		};
		let eof = Eof {
			body: eof_body,
			..Default::default()
		};
		assert_eq!(eof.data_slice(1, 3), &[2, 3, 4]);
	}

	#[test]
	fn test_data_slice_out_of_bounds_offset() {
		let eof_body = EofBody {
			data_section: vec![1, 2, 3, 4, 5],
			..Default::default()
		};
		let eof = Eof {
			body: eof_body,
			..Default::default()
		};
		assert_eq!(eof.data_slice(10, 3), &[]);
	}

	#[test]
	fn test_data_slice_out_of_bounds_length() {
		let eof_body = EofBody {
			data_section: vec![1, 2, 3, 4, 5],
			..Default::default()
		};
		let eof = Eof {
			body: eof_body,
			..Default::default()
		};
		assert_eq!(eof.data_slice(2, 10), &[3, 4, 5]);
	}

	#[test]
	fn test_data_slice_empty_data_section() {
		let eof_body = EofBody {
			data_section: vec![],
			..Default::default()
		};
		let eof = Eof {
			body: eof_body,
			..Default::default()
		};
		assert_eq!(eof.data_slice(0, 3), &[]);
	}
}
