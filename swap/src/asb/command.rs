use crate::jude::Amount;
use anyhow::Result;
use std::path::PathBuf;

#[derive(structopt::StructOpt, Debug)]
pub struct Arguments {
    #[structopt(
        long = "config",
        help = "Provide a custom path to the configuration file. The configuration file must be a toml file.",
        parse(from_os_str)
    )]
    pub config: Option<PathBuf>,

    #[structopt(subcommand)]
    pub cmd: Command,
}

#[derive(structopt::StructOpt, Debug)]
#[structopt(name = "jude_btc-swap", about = "jude BTC atomic swap")]
pub enum Command {
    Start {
        #[structopt(long = "max-sell-jude", help = "The maximum amount of jude the ASB is willing to sell.", default_value="0.0", parse(try_from_str = parse_jude))]
        max_sell: Amount,
    },
    History,
}

fn parse_jude(str: &str) -> Result<Amount> {
    let amount = Amount::parse_jude(str)?;
    Ok(amount)
}
