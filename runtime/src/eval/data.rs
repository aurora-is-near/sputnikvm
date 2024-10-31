#![allow(clippy::module_name_repetitions)]
use super::Control;
use crate::{ExitError, Handler, Runtime};

pub fn data_load<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	todo!()
}

pub fn data_loadn<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	todo!()
}

pub fn data_size<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	todo!()
}

pub fn data_copy<H: Handler>(runtime: &Runtime, _handler: &mut H) -> Control<H> {
	require_eof!(runtime);
	todo!()
}
