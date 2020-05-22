use clap::{App, Arg};
use hostgen::chain::IntoFlatEntryIterator;
use hostgen::entry::{entries_from_val, EntryIterator};
use log::error;
use serde_yaml::Value;
use std::fs::File;
use std::io::{self};

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
                .default_value("hosts.yaml")
                .takes_value(true)
                .multiple(true),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("output file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("format")
            .short("f")
            .long("format")
            .takes_value(true)
            .required(true)
            .possible_values(&["dnsmasq", "zone", "env"])
        )
        .get_matches();

    let entries = matches
        .values_of("config")
        .unwrap()
        .filter_map(|config| {
            let f = std::fs::File::open(config)
                .on_err(|e| error!("unable to read {}: {}", config, e))
                .ok()?;
            let data: Value = serde_yaml::from_reader(f)
                .on_err(|e| error!("unable to parse yaml in {}: {}", config, e))
                .ok()?;
            Some(entries_from_val(data))
        })
        .flatten_entries();

    let entries = {
        match matches.value_of("format") {
            Some("dnsmasq") => entries.as_dnsmasq_reservations(),
            Some("zone") => entries.as_zone_records(),
            Some("env") => entries.as_env_vars(),
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

trait OnErr<T, E> {
    fn on_err<F: Fn(&E)>(self, f: F) -> Self;
}

impl<T, E> OnErr<T, E> for Result<T, E> {
    fn on_err<F: Fn(&E)>(self, f: F) -> Self {
        if let Err(e) = &self {
            f(e);
        }
        self
    }
}
