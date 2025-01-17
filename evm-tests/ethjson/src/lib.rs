// Copyright 2015-2020 Parity Technologies (UK) Ltd.
// This file is part of Open Ethereum.

// Open Ethereum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Open Ethereum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Open Ethereum.  If not, see <http://www.gnu.org/licenses/>.

//! JSON deserialization library

#![warn(missing_docs)]
#![allow(clippy::too_long_first_doc_paragraph)]

pub mod bytes;
pub mod hash;
pub mod maybe;
pub mod spec;
pub mod state;
pub mod transaction;
pub mod uint;
pub mod vm;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;
