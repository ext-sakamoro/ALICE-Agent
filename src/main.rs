use alice_agent::cli::{run, Cli};
use alice_agent::config::AgentConfig;
use clap::Parser;

fn main() {
    std::process::exit(run(
        &Cli::parse(),
        |k| std::env::var(k).ok(),
        AgentConfig::load(),
    ));
}
