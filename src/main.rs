#![feature(ip)]

use hostgen::entry::EntryIterator;
use clap::{App, Arg, SubCommand};
use serde_yaml::Value;
use std::fs::File;
use std::io::{self};
use hostgen::entry;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Host Config Generator")
        .version("0.1")
        .author("Clint Armstrong <clint@clintarmstrong.net>")
        .about("Generates dnsmasq and zonec configs")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("config file")
                .takes_value(true)
                .index(1),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("output file")
                .takes_value(true),
        )
        .subcommand(SubCommand::with_name("dnsmasq").about("generates dnsmasq hosts"))
        .subcommand(SubCommand::with_name("zone").about("generates zone entries"))
        .get_matches();

    let f = std::fs::File::open(matches.value_of("config").unwrap_or("hosts.yaml"))?;
    let data: Value = serde_yaml::from_reader(f)?;

    let entries = entry::entries_from_val(data);
    let entries = {
        match matches.subcommand_name() {
            Some("dnsmasq") => entries.as_dnsmasq_reservations(),
            Some("zone") => entries.as_zone_records(),
            _ => return Ok(()),
        }
    };

    if let Some(output) = matches.value_of("output") {
        let mut writer = File::create(output)?;
        entries.write(&mut writer)?;
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        entries.write(&mut writer)?;
    }

    Ok(())
}
