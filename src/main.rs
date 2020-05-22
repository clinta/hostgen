use clap::{App, Arg};
use hostgen::chain::IntoFlatEntryIterator;
use hostgen::entry::entries_from_dnsmasq_leases;
use hostgen::entry::{entries_from_val, EntryIterator, EntryIteratorFrom};
use itertools::Itertools;
use log::error;
use serde_yaml::Value;
use std::fs::File;
use std::io::BufRead;
use std::io::{self};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Host Config Generator")
        .version("0.2")
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
            Arg::with_name("leases")
                .short("dl")
                .long("leases")
                .value_name("FILE")
                .help("dnsmasq leases file")
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
                .possible_values(&["dnsmasq", "zone", "env"]),
        )
        .get_matches();

    let entries = ordered_values_of(&matches, "config", "leases")
        .filter_map(|(a, v)| match a {
            "config" => {
                let f = std::fs::File::open(v)
                    .on_err(|e| error!("unable to read {}: {}", v, e))
                    .ok()?;
                let data: Value = serde_yaml::from_reader(f)
                    .on_err(|e| error!("unable to parse yaml in {}: {}", v, e))
                    .ok()?;
                Some(EntryIteratorFrom::Val(entries_from_val(data)))
            }
            "leases" => {
                let f = std::fs::File::open(v)
                    .on_err(|e| error!("unable to read {}: {}", v, e))
                    .ok()?;
                let lines = io::BufReader::new(f).lines().filter_map(move |l| {
                    l.on_err(|e| error!("error readling line in {}: {}", v, e))
                        .ok()
                });
                Some(EntryIteratorFrom::DnsMasq(entries_from_dnsmasq_leases(
                    lines,
                )))
            }
            _ => None,
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

fn ordered_values_of<'a>(
    matches: &'a clap::ArgMatches,
    arg1: &'a str,
    arg2: &'a str,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    enumerate_values_of(matches, arg1)
        .merge(enumerate_values_of(matches, arg2))
        .map(|(_, arg, v)| (arg, v))
}

fn enumerate_values_of<'a>(
    matches: &'a clap::ArgMatches,
    arg: &'a str,
) -> impl Iterator<Item = (usize, &'a str, &'a str)> + 'a {
    let idxs = matches.indices_of(arg).unwrap_or_default();
    let vals = matches.values_of(arg).unwrap_or_default();
    idxs.zip(vals).map(move |(i, v)| (i, arg, v))
}
