use crate::consts;
use crate::Config;
use evm_core::utils::U64_MAX;
use evm_core::ExitError;
use primitive_types::{H256, U256};

pub fn call_extra_check(gas: U256, after_gas: u64, config: &Config) -> Result<(), ExitError> {
	if config.err_on_call_with_more_gas && U256::from(after_gas) < gas {
		Err(ExitError::OutOfGas)
	} else {
		Ok(())
	}
}

pub fn suicide_refund(already_removed: bool) -> i64 {
	if already_removed {
		0
	} else {
		i64::from(consts::R_SUICIDE)
	}
}

#[allow(clippy::collapsible_else_if)]
pub fn sstore_refund(original: H256, current: H256, new: H256, config: &Config) -> i64 {
	if config.sstore_gas_metering {
		if current == new {
			0
		} else {
			if original == current && new == H256::default() {
				config.refund_sstore_clears
			} else {
				let mut refund = 0;

				if original != H256::default() {
					if current == H256::default() {
						refund -= config.refund_sstore_clears;
					} else if new == H256::default() {
						refund += config.refund_sstore_clears;
					}
				}

				if original == new {
					let for_refund = if original == H256::default() {
						config.gas_sstore_set - config.gas_sload
					} else {
						config.gas_sstore_reset - config.gas_sload
					};
					refund += i64::try_from(for_refund).unwrap_or(i64::MAX);
				}

				refund
			}
		}
	} else {
		if current != H256::default() && new == H256::default() {
			config.refund_sstore_clears
		} else {
			0
		}
	}
}

pub fn create2_cost(len: U256) -> Result<u64, ExitError> {
	let base = U256::from(consts::G_CREATE);
	// ceil(len / 32.0)
	let sha_addup_base = len / U256::from(32)
		+ if len % U256::from(32) == U256::zero() {
			U256::zero()
		} else {
			U256::one()
		};
	let sha_addup = U256::from(consts::G_SHA3WORD)
		.checked_mul(sha_addup_base)
		.ok_or(ExitError::OutOfGas)?;
	let gas = base.checked_add(sha_addup).ok_or(ExitError::OutOfGas)?;

	if gas > U64_MAX {
		return Err(ExitError::OutOfGas);
	}

	Ok(gas.as_u64())
}

pub fn exp_cost(power: U256, config: &Config) -> Result<u64, ExitError> {
	if power == U256::zero() {
		Ok(u64::from(consts::G_EXP))
	} else {
		let gas = U256::from(consts::G_EXP)
			.checked_add(
				U256::from(config.gas_expbyte)
					.checked_mul(U256::from(crate::utils::log2floor(power) / 8 + 1))
					.ok_or(ExitError::OutOfGas)?,
			)
			.ok_or(ExitError::OutOfGas)?;

		if gas > U64_MAX {
			return Err(ExitError::OutOfGas);
		}

		Ok(gas.as_u64())
	}
}

pub fn verylowcopy_cost(len: U256) -> Result<u64, ExitError> {
	let wordd = len / U256::from(32);
	let is_wordr = (len % U256::from(32)) == U256::zero();

	let gas = U256::from(consts::G_VERYLOW)
		.checked_add(
			U256::from(consts::G_COPY)
				.checked_mul(if is_wordr { wordd } else { wordd + U256::one() })
				.ok_or(ExitError::OutOfGas)?,
		)
		.ok_or(ExitError::OutOfGas)?;

	if gas > U64_MAX {
		return Err(ExitError::OutOfGas);
	}

	Ok(gas.as_u64())
}

pub fn extcodecopy_cost(
	len: U256,
	is_cold: bool,
	delegated_designator_is_cold: Option<bool>,
	config: &Config,
) -> Result<u64, ExitError> {
	let wordd = len / U256::from(32);
	let is_wordr = (len % U256::from(32)) == U256::zero();
	let gas = U256::from(address_access_cost(
		is_cold,
		delegated_designator_is_cold,
		config.gas_ext_code,
		config,
	))
	.checked_add(
		U256::from(consts::G_COPY)
			.checked_mul(if is_wordr { wordd } else { wordd + U256::one() })
			.ok_or(ExitError::OutOfGas)?,
	)
	.ok_or(ExitError::OutOfGas)?;

	if gas > U64_MAX {
		return Err(ExitError::OutOfGas);
	}

	Ok(gas.as_u64())
}

pub fn log_cost(n: u8, len: U256) -> Result<u64, ExitError> {
	let gas = U256::from(consts::G_LOG)
		.checked_add(
			U256::from(consts::G_LOGDATA)
				.checked_mul(len)
				.ok_or(ExitError::OutOfGas)?,
		)
		.ok_or(ExitError::OutOfGas)?
		.checked_add(U256::from(consts::G_LOGTOPIC * u32::from(n)))
		.ok_or(ExitError::OutOfGas)?;

	if gas > U64_MAX {
		return Err(ExitError::OutOfGas);
	}

	Ok(gas.as_u64())
}

pub fn sha3_cost(len: U256) -> Result<u64, ExitError> {
	let wordd = len / U256::from(32);
	let is_wordr = (len % U256::from(32)) == U256::zero();

	let gas = U256::from(consts::G_SHA3)
		.checked_add(
			U256::from(consts::G_SHA3WORD)
				.checked_mul(if is_wordr { wordd } else { wordd + U256::one() })
				.ok_or(ExitError::OutOfGas)?,
		)
		.ok_or(ExitError::OutOfGas)?;

	if gas > U64_MAX {
		return Err(ExitError::OutOfGas);
	}

	Ok(gas.as_u64())
}

pub const fn sload_cost(is_cold: bool, config: &Config) -> u64 {
	if config.increase_state_access_gas {
		if is_cold {
			config.gas_sload_cold
		} else {
			config.gas_storage_read_warm
		}
	} else {
		config.gas_sload
	}
}

pub const fn storage_read_warm(config: &Config) -> u64 {
	config.gas_storage_read_warm
}

#[allow(clippy::collapsible_else_if)]
pub fn sstore_cost(
	original: H256,
	current: H256,
	new: H256,
	gas: u64,
	is_cold: bool,
	config: &Config,
) -> Result<u64, ExitError> {
	let gas_cost = if config.estimate {
		config.gas_sstore_set
	} else {
		if config.sstore_gas_metering {
			if config.sstore_revert_under_stipend && gas <= config.call_stipend {
				return Err(ExitError::OutOfGas);
			}

			if new == current {
				config.gas_sload
			} else {
				if original == current {
					if original == H256::zero() {
						config.gas_sstore_set
					} else {
						config.gas_sstore_reset
					}
				} else {
					config.gas_sload
				}
			}
		} else {
			if current == H256::zero() && new != H256::zero() {
				config.gas_sstore_set
			} else {
				config.gas_sstore_reset
			}
		}
	};
	Ok(
		// In EIP-2929 we charge extra if the slot has not been used yet in this transaction
		if is_cold {
			gas_cost + config.gas_sload_cold
		} else {
			gas_cost
		},
	)
}

pub fn suicide_cost(value: U256, is_cold: bool, target_exists: bool, config: &Config) -> u64 {
	let eip161 = !config.empty_considered_exists;
	let should_charge_topup = if eip161 {
		value != U256::zero() && !target_exists
	} else {
		!target_exists
	};

	let suicide_gas_topup = if should_charge_topup {
		config.gas_suicide_new_account
	} else {
		0
	};

	let mut gas = config.gas_suicide + suicide_gas_topup;
	if config.increase_state_access_gas && is_cold {
		gas += config.gas_account_access_cold;
	}
	gas
}

#[allow(clippy::fn_params_excessive_bools)]
pub fn call_cost(
	value: U256,
	is_cold: bool,
	delegated_designator_is_cold: Option<bool>,
	is_call_or_callcode: bool,
	is_call_or_staticcall: bool,
	new_account: bool,
	config: &Config,
) -> u64 {
	let transfers_value = value != U256::default();
	address_access_cost(
		is_cold,
		delegated_designator_is_cold,
		config.gas_call,
		config,
	) + xfer_cost(is_call_or_callcode, transfers_value)
		+ new_cost(is_call_or_staticcall, new_account, transfers_value, config)
}

pub const fn address_access_cost(
	is_cold: bool,
	delegated_designator_is_cold: Option<bool>,
	regular_value: u64,
	config: &Config,
) -> u64 {
	if config.increase_state_access_gas {
		let mut gas = if is_cold {
			config.gas_account_access_cold
		} else {
			config.gas_storage_read_warm
		};
		if config.has_authorization_list {
			if let Some(target_is_cold) = delegated_designator_is_cold {
				if target_is_cold {
					gas += config.gas_account_access_cold;
				} else {
					gas += config.gas_storage_read_warm;
				}
			}
		}
		gas
	} else {
		regular_value
	}
}

fn xfer_cost(is_call_or_callcode: bool, transfers_value: bool) -> u64 {
	if is_call_or_callcode && transfers_value {
		u64::from(consts::G_CALLVALUE)
	} else {
		0
	}
}

fn new_cost(
	is_call_or_staticcall: bool,
	new_account: bool,
	transfers_value: bool,
	config: &Config,
) -> u64 {
	let eip161 = !config.empty_considered_exists;
	if is_call_or_staticcall {
		if eip161 {
			if transfers_value && new_account {
				u64::from(consts::G_NEWACCOUNT)
			} else {
				0
			}
		} else if new_account {
			u64::from(consts::G_NEWACCOUNT)
		} else {
			0
		}
	} else {
		0
	}
}
