// Copyright 2018-2019 Parity Technologies (UK) Ltd.
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

//! Substrate CLI library.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

pub use cli::error;
pub mod chain_spec;
mod service;

use tokio::prelude::Future;
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};
pub use cli::{VersionInfo, IntoExit, NoCustom};
use substrate_service::{ServiceFactory, Roles as ServiceRoles};
use std::ops::Deref;
use log::info;

/// The chain specification option.
#[derive(Clone, Debug)]
pub enum ChainSpec {
	/// Whatever the current runtime is, with just Alice as an auth.
	Development,
	/// Whatever the current runtime is, with simple Alice/Bob auths.
	LocalTestnet,
	/// The Emberic Elm testnet.
	EmbericElm,
	/// Whatever the current runtime is with the "global testnet" defaults.
	StagingTestnet,
}

/// Get a chain config from a spec setting.
impl ChainSpec {
	pub(crate) fn load(self) -> Result<chain_spec::ChainSpec, String> {
		Ok(match self {
			ChainSpec::EmbericElm => chain_spec::emberic_elm_config()?,
			ChainSpec::Development => chain_spec::development_config(),
			ChainSpec::LocalTestnet => chain_spec::local_testnet_config(),
			ChainSpec::StagingTestnet => chain_spec::staging_testnet_config(),
		})
	}

	pub(crate) fn from(s: &str) -> Option<Self> {
		match s {
			"dev" => Some(ChainSpec::Development),
			"local" => Some(ChainSpec::LocalTestnet),
			"" | "elm" | "emberic-elm" => Some(ChainSpec::EmbericElm),
			"staging" => Some(ChainSpec::StagingTestnet),
			_ => None,
		}
	}
}

fn load_spec(id: &str) -> Result<Option<chain_spec::ChainSpec>, String> {
	Ok(match ChainSpec::from(id) {
		Some(spec) => Some(spec.load()?),
		None => None,
	})
}

pub use structopt::clap::App;
pub use cli::{GetLogFilter, AugmentClap, CoreParams};
use structopt::{StructOpt, clap::{AppSettings, SubCommand}};
#[derive(Clone, Debug)]
pub enum CustomCommand {
	MyCommand(MyCommandCmd),
	YourCommand(YourCommandCmd),
	None,
}

impl StructOpt for CustomCommand {
	fn clap<'a, 'b>() -> App<'a, 'b> {
		App::new("MyCommand")
			.subcommand(
			MyCommandCmd::augment_clap(SubCommand::with_name("my-command"))
				.about("my command"))
			.subcommand(
				YourCommandCmd::augment_clap(SubCommand::with_name("your-command"))
					.about("your command"))
	}

	fn from_clap(matches: &::structopt::clap::ArgMatches) -> Self {
		match matches.subcommand() {
			("my-command", Some(matches)) =>
				CustomCommand::MyCommand(MyCommandCmd::from_clap(matches)),
			("your-command", Some(matches)) =>
				CustomCommand::YourCommand(YourCommandCmd::from_clap(matches)),
			(_, Some(_)) => CustomCommand::None,
			(_, None) => CustomCommand::None,
		}
	}
}

impl GetLogFilter for CustomCommand {
	fn get_log_filter(&self) -> Option<String> {
		None
	}
}

#[derive(Debug, StructOpt, Clone)]
pub struct MyCommandCmd {
	#[structopt(long = "my-test")]
	pub my_test: Option<String>,
}

#[derive(Debug, StructOpt, Clone)]
pub struct YourCommandCmd {
	#[structopt(long = "your-test")]
	pub your_test: Option<String>,
}

/// Parse command line arguments into service configuration.
pub fn run<I, T, E>(args: I, exit: E, version: cli::VersionInfo) -> error::Result<()> where
	I: IntoIterator<Item = T>,
	T: Into<std::ffi::OsString> + Clone,
	E: IntoExit,
{
	cli::parse_and_execute::<service::Factory, CustomCommand, NoCustom, _, _, _, _, _>(
		load_spec, &version, "substrate-node", args, exit,
		|exit, _custom_args, config| {
			info!("{}", version.name);
			info!("  version {}", config.full_version());
			info!("  by Parity Technologies, 2017-2019");
			info!("Chain specification: {}", config.chain_spec.name());
			info!("Node name: {}", config.name);
			info!("Roles: {:?}", config.roles);
			let runtime = RuntimeBuilder::new().name_prefix("main-tokio-").build()
				.map_err(|e| format!("{:?}", e))?;
			let executor = runtime.executor();
			match config.roles {
				ServiceRoles::LIGHT => run_until_exit(
					runtime,
					service::Factory::new_light(config, executor).map_err(|e| format!("{:?}", e))?,
					exit
				),
				_ => run_until_exit(
					runtime,
					service::Factory::new_full(config, executor).map_err(|e| format!("{:?}", e))?,
					exit
				),
			}.map_err(|e| format!("{:?}", e))
		}
	).map(|x| {
		if let Some(x) = x {
			match x{
				CustomCommand::MyCommand(my_command_cmd) => println!("my command executed: {}", my_command_cmd.my_test.unwrap_or("".to_string())),
				CustomCommand::YourCommand(your_command_cmd) => println!("your command executed: {}", your_command_cmd.your_test.unwrap_or("".to_string())),
				CustomCommand::None => {},
			}
		}
	}
	).map_err(Into::into).map(|_| ())
}

fn run_until_exit<T, C, E>(
	mut runtime: Runtime,
	service: T,
	e: E,
) -> error::Result<()>
	where
	    T: Deref<Target=substrate_service::Service<C>>,
		C: substrate_service::Components,
		E: IntoExit,
{
	let (exit_send, exit) = exit_future::signal();

	let executor = runtime.executor();
	cli::informant::start(&service, exit.clone(), executor.clone());

	let _ = runtime.block_on(e.into_exit());
	exit_send.fire();

	// we eagerly drop the service so that the internal exit future is fired,
	// but we need to keep holding a reference to the global telemetry guard
	let _telemetry = service.telemetry();
	drop(service);

	// TODO [andre]: timeout this future #1318
	let _ = runtime.shutdown_on_idle().wait();

	Ok(())
}
