// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! ....

#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(not(feature = "std"), no_std)]

use srml_staking::{Trait as StakingTrait, Module};
use srml_support::traits::Currency;
use rstd::marker::PhantomData;
use parity_codec::Codec;
use primitives::traits::{SimpleArithmetic, MaybeSerializeDebug};

/// Pre-defined types
// pub mod misconduct;

mod fraction;
pub use fraction::Fraction;

type BalanceOf<T> = <<T as StakingTrait>::Currency as Currency<<T as system::Trait>::AccountId>>::Balance;

/// ..
struct MisconductModule<T>(PhantomData<T>);

impl<T: Misconduct + StakingTrait> MisconductModule<T> {
	/// Slash after rolling misconduct was reported.
	/// Returns misconduct level
	pub fn rolling_data(misconduct: &mut T, misbehaved: &[T::AccountId], validators: u64, session_index: u64) -> u8
		where T::Severity: Into<BalanceOf<T>> + From<BalanceOf<T>>
	{
		T::on_misconduct(misconduct, misbehaved, validators, session_index);
		let severity = misconduct.severity();

		for who in misbehaved {
			let balance = <Module<T>>::slashable_balance(who);
			let d = severity.denominator().into();
			let n = severity.numerator().into();
			let slash = (balance * d) / n;
			<Module<T>>::slash_validator(who, slash);
		}
		misconduct.as_misconduct_level(severity)
	}

	/// Report misconduct during an era
	pub fn era_data<AccountId>(misconduct: &mut T, misbehaved: &[T::AccountId], validators: u64, session_index: u64) {
		T::on_misconduct(misconduct, misbehaved, validators, session_index);
	}
}

impl<T: StakingTrait + OnEndEra> MisconductModule<T> {

	/// Slash in the end of era
	fn slash(end: &T) -> u8
		where T::Severity: Into<BalanceOf<T>> + From<BalanceOf<T>>
	{
		let severity = end.severity();
		let misbehaved = end.get_misbehaved();

		for who in &misbehaved {
			let balance = <Module<T>>::slashable_balance(who);
			let d = severity.denominator().into();
			let n = severity.numerator().into();
			let slash = (balance * d) / n;
			<Module<T>>::slash_validator(who, slash);
		}

		end.as_misconduct_level(severity)
	}
}

/// Base trait for representing misconducts
pub trait Misconduct: system::Trait {
	/// Severity represented as a fraction
	type Severity: SimpleArithmetic + Codec + Copy + MaybeSerializeDebug + Default;

	/// Estimate misconduct level (1, 2, 3 or 4) based on `severity`
	fn as_misconduct_level(&self, severity: Fraction<Self::Severity>) -> u8;

	/// Estimate new severity level after misconduct was reported
	fn on_misconduct(
		&mut self,
		misbehaved: &[Self::AccountId],
		total_validators: u64,
		session_index: u64
	);

	/// Get estimate of severity level
	fn severity(&self) -> Fraction<Self::Severity>;
}

/// Apply slashing in end of era
pub trait OnEndEra: Misconduct {
	/// Returns the misbehaved validators in the end on era
	fn get_misbehaved(&self) -> Vec<Self::AccountId>;
}

#[cfg(test)]
mod test {
	use super::*;
	use std::{collections::HashSet, cell::RefCell};
	use primitives::traits::{IdentityLookup, Convert, OpaqueKeys, OnInitialize};
	use primitives::testing::{Header, UintAuthorityId};
	use substrate_primitives::{H256, Blake2Hasher};
	use srml_staking::{EraIndex, Module as StakingModule};
	use srml_support::{impl_outer_origin, parameter_types, assert_ok, traits::Currency, EnumerableStorageMap};

	/// The AccountId alias in this test module.
	pub type AccountId = u64;
	pub type BlockNumber = u64;
	pub type Balance = u64;

	pub type Staking = Module<Test>;

	pub struct CurrencyToVoteHandler;

	impl Convert<u64, u64> for CurrencyToVoteHandler {
		fn convert(x: u64) -> u64 { x }
	}
	impl Convert<u128, u64> for CurrencyToVoteHandler {
		fn convert(x: u128) -> u64 {
			x as u64
		}
	}

	#[derive(Clone, PartialEq, Eq, Debug)]
	pub struct Test;

	impl_outer_origin!{
		pub enum Origin for Test {}
	}

	impl system::Trait for Test {
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = ::primitives::traits::BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
	}

	impl balances::Trait for Test {
		type Balance = u64;
		type OnFreeBalanceZero = Staking;
		type OnNewAccount = ();
		type Event = ();
		type TransactionPayment = ();
		type TransferPayment = ();
		type DustRemoval = ();
	}

	impl StakingTrait for Test {
		type Currency = balances::Module<Self>;
		type CurrencyToVote = CurrencyToVoteHandler;
		type OnRewardMinted = ();
		type Event = ();
		type Slash = ();
		type Reward = ();
		type SessionsPerEra = SessionsPerEra;
		type BondingDuration = BondingDuration;
	}

	impl session::Trait for Test {
		type OnSessionEnding = Staking;
		type Keys = UintAuthorityId;
		type ShouldEndSession = session::PeriodicSessions<Period, Offset>;
		type SessionHandler = ();
		type Event = ();
	}

	parameter_types! {
		pub const SessionsPerEra: session::SessionIndex = 3;
		pub const BondingDuration: EraIndex = 3;
	}

	parameter_types! {
		pub const Period: BlockNumber = 1;
		pub const Offset: BlockNumber = 0;
	}

	impl Misconduct for Test {
		type Severity = u64;

		fn as_misconduct_level(&self, severity: Fraction<Self::Severity>) -> u8 { unimplemented!() }

		fn on_misconduct(
			&mut self,
			misbehaved: &[AccountId],
			total_validators: u64,
			session_index: u64
		) {}

		fn severity(&self) -> Fraction<Self::Severity> { unimplemented!() }
	}

	#[test]
	fn it_works() {
		let mut misconduct = Test;
		let _ = MisconductModule::<Test>::rolling_data(&mut misconduct, &[], 0, 0);
		// let m = MisconductModule::slash(misconduct);
	}
}
