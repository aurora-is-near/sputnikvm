use crate::prelude::*;
use crate::Opcode;

/// Trap which indicates that an `ExternalOpcode` has to be handled.
pub type Trap = Opcode;

/// Capture represents the result of execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capture<E, T> {
	/// The machine has exited. It cannot be executed again.
	Exit(E),
	/// The machine has trapped. It is waiting for external information, and can
	/// be executed again.
	Trap(T),
}

/// Exit reason.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExitReason {
	/// Machine has succeeded.
	Succeed(ExitSucceed),
	/// Machine returns a normal EVM error.
	Error(ExitError),
	/// Machine encountered an explicit revert.
	Revert(ExitRevert),
	/// Machine encountered an error that is not supposed to be normal EVM
	/// errors, such as requiring too much memory to execute.
	Fatal(ExitFatal),
}

impl ExitReason {
	/// Whether the exit is succeeded.
	#[must_use]
	pub const fn is_succeed(&self) -> bool {
		matches!(self, Self::Succeed(_))
	}

	/// Whether the exit is error.
	#[must_use]
	pub const fn is_error(&self) -> bool {
		matches!(self, Self::Error(_))
	}

	/// Whether the exit is revert.
	#[must_use]
	pub const fn is_revert(&self) -> bool {
		matches!(self, Self::Revert(_))
	}

	/// Whether the exit is fatal.
	#[must_use]
	pub const fn is_fatal(&self) -> bool {
		matches!(self, Self::Fatal(_))
	}
}

/// Exit succeed reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExitSucceed {
	/// Machine encountered an explicit stop.
	Stopped,
	/// Machine encountered an explicit return.
	Returned,
	/// Machine encountered an explicit suicide.
	Suicided,
}

impl From<ExitSucceed> for ExitReason {
	fn from(s: ExitSucceed) -> Self {
		Self::Succeed(s)
	}
}

/// Exit revert reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExitRevert {
	/// Machine encountered an explicit revert.
	Reverted,
}

impl From<ExitRevert> for ExitReason {
	fn from(s: ExitRevert) -> Self {
		Self::Revert(s)
	}
}

/// Exit error reason.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExitError {
	/// Trying to pop from an empty stack.
	#[cfg_attr(feature = "with-codec", codec(index = 0))]
	StackUnderflow,
	/// Trying to push into a stack over stack limit.
	#[cfg_attr(feature = "with-codec", codec(index = 1))]
	StackOverflow,
	/// Jump destination is invalid.
	#[cfg_attr(feature = "with-codec", codec(index = 2))]
	InvalidJump,
	/// An opcode accesses memory region, but the region is invalid.
	#[cfg_attr(feature = "with-codec", codec(index = 3))]
	InvalidRange,
	/// Encountered the designated invalid opcode.
	#[cfg_attr(feature = "with-codec", codec(index = 4))]
	DesignatedInvalid,
	/// Call stack is too deep (runtime).
	#[cfg_attr(feature = "with-codec", codec(index = 5))]
	CallTooDeep,
	/// Create opcode encountered collision (runtime).
	#[cfg_attr(feature = "with-codec", codec(index = 6))]
	CreateCollision,
	/// Create init code exceeds limit (runtime).
	#[cfg_attr(feature = "with-codec", codec(index = 7))]
	CreateContractLimit,

	/// Invalid opcode during execution or starting byte is 0xef. See [EIP-3541](https://github.com/ethereum/EIPs/blob/master/EIPS/eip-3541.md).
	#[cfg_attr(feature = "with-codec", codec(index = 15))]
	InvalidCode(Opcode),

	/// An opcode accesses external information, but the request is off offset
	/// limit (runtime).
	#[cfg_attr(feature = "with-codec", codec(index = 8))]
	OutOfOffset,
	/// Execution runs out of gas (runtime).
	#[cfg_attr(feature = "with-codec", codec(index = 9))]
	OutOfGas,
	/// Not enough fund to start the execution (runtime).
	#[cfg_attr(feature = "with-codec", codec(index = 10))]
	OutOfFund,

	/// PC underflowed (unused).
	#[allow(clippy::upper_case_acronyms)]
	#[cfg_attr(feature = "with-codec", codec(index = 11))]
	PCUnderflow,

	/// Attempt to create an empty account (runtime, unused).
	#[cfg_attr(feature = "with-codec", codec(index = 12))]
	CreateEmpty,

	/// Other normal errors.
	#[cfg_attr(feature = "with-codec", codec(index = 13))]
	Other(Cow<'static, str>),

	/// Nonce reached maximum value of 2^64-1
	/// <https://eips.ethereum.org/EIPS/eip-2681>
	#[cfg_attr(feature = "with-codec", codec(index = 14))]
	MaxNonce,

	/// `usize` casting overflow
	#[cfg_attr(feature = "with-codec", codec(index = 15))]
	UsizeOverflow,
	#[cfg_attr(feature = "with-codec", codec(index = 16))]
	CreateContractStartingWithEF,

	#[cfg_attr(feature = "with-codec", codec(index = 17))]
	EOFDecodeError(EofDecodeError),
}

impl From<ExitError> for ExitReason {
	fn from(s: ExitError) -> Self {
		Self::Error(s)
	}
}

/// Exit fatal reason.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ExitFatal {
	/// The operation is not supported.
	NotSupported,
	/// The trap (interrupt) is unhandled.
	UnhandledInterrupt,
	/// The environment explicitly set call errors as fatal error.
	CallErrorAsFatal(ExitError),

	/// Other fatal errors.
	Other(Cow<'static, str>),
}

impl From<ExitFatal> for ExitReason {
	fn from(s: ExitFatal) -> Self {
		Self::Fatal(s)
	}
}

/// EOF decode errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(
	feature = "with-codec",
	derive(scale_codec::Encode, scale_codec::Decode, scale_info::TypeInfo)
)]
#[cfg_attr(feature = "with-serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EofDecodeError {
	/// Missing input while processing EOF.
	MissingInput,
	/// Missing body while processing EOF.
	MissingBodyData,
	/// Body size is more than specified in the header.
	BodySizeMoreThanInHeader,
	/// Invalid types section data.
	InvalidTypesSectionData,
	/// Invalid types section size.
	InvalidTypesSectionSize,
	/// Invalid EOF magic number.
	InvalidEOFMagicNumber,
	/// Invalid EOF version.
	InvalidEOFVersion,
	/// Invalid number for types kind
	InvalidNumberForTypesKind,
	/// Invalid number for code kind
	InvalidNumberForCodeKind,
	/// Invalid terminal byte
	InvalidTerminalByte,
	/// Invalid data kind
	InvalidDataKind,
	/// Invalid kind after code
	InvalidKindAfterCode,
	/// Mismatch of code and types sizes.
	MismatchCodeAndTypesSize,
	/// There should be at least one size.
	SizesNotFound,
	/// Missing size.
	ShortInputForSizes,
	/// Code size can't be zero
	ZeroCodeSize,
	/// Invalid code number.
	TooManyCodeSections,
	/// Invalid number of code sections.
	InvalidNumberCodeSections,
	/// Invalid container number.
	InvalidNumberContainerSections,
	/// Invalid initcode size.
	InvalidEOFInitcodeSize,
}

impl From<EofDecodeError> for ExitError {
	fn from(e: EofDecodeError) -> Self {
		Self::EOFDecodeError(e)
	}
}
