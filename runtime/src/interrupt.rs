use crate::{Handler, Runtime};

/// Interrupt resolution.
pub enum Resolve<'a, H: Handler> {
	/// Create interrupt resolution.
	Create(H::CreateInterrupt, ResolveCreate<'a>),
	/// EOF Create interrupt resolution.
	EOFCreate(H::EOFCreateInterrupt, ResolveEOFCreate<'a>),
	/// Call interrupt resolution.
	Call(H::CallInterrupt, ResolveCall<'a>),
}

/// Create interrupt resolution.
pub struct ResolveCreate<'a> {
	_runtime: &'a mut Runtime,
}

impl<'a> ResolveCreate<'a> {
	pub(crate) fn new(runtime: &'a mut Runtime) -> Self {
		Self { _runtime: runtime }
	}
}

/// EOF Create interrupt resolution.
pub struct ResolveEOFCreate<'a> {
	_runtime: &'a mut Runtime,
}

impl<'a> ResolveEOFCreate<'a> {
	// TODO: resolve clippy
	#[allow(dead_code)]
	pub(crate) fn new(runtime: &'a mut Runtime) -> Self {
		Self { _runtime: runtime }
	}
}

/// Call interrupt resolution.
pub struct ResolveCall<'a> {
	_runtime: &'a mut Runtime,
}

impl<'a> ResolveCall<'a> {
	pub(crate) fn new(runtime: &'a mut Runtime) -> Self {
		Self { _runtime: runtime }
	}
}
