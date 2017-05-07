#[macro_use]
extern crate clap;
extern crate microrpc;
extern crate serial;
extern crate rustyline;

mod util;

use microrpc::{Type, Value, Client, Procedure};

use clap::{Arg, ArgMatches};
use serial::prelude::*;
use rustyline::Editor;
use rustyline::error::ReadlineError;

use std::path::Path;
use std::io::{stderr, Read, Write};
use std::error::Error;
use std::time::Duration;

fn print_procs(procs: &[Procedure]) {
    println!("endpoint reports {} exported procedure(s)", procs.len());
    for p in procs {
        if p.parameter_types().is_empty() {
            print!("{}: (unary function)", p.id());
        } else {
            print!("{}: {}", p.id(), p.parameter_types().iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>()
                .join(", "));
        }

        if let Some(ret) = p.return_type() {
            println!(" -> {}", ret);
        } else {
            println!();
        }
    }
}

fn execute<C: Read + Write>(line: &str, conn: &mut Client<C>) -> Result<(), Box<Error>> {
    let mut parts = line.trim().split_whitespace();

    let cmd = match parts.next() {
        Some(cmd) => cmd,
        None => return Ok(()),  // ignore missing commands
    };

    match cmd {
        "list" => {
            if let Some(unexpected) = parts.next() {
                return Err(format!("unexpected argument to 'list' command: {}", unexpected).into());
            }

            let procs = conn.procedures()?;
            print_procs(procs);
        }
        "enumerate" => {
            if let Some(unexpected) = parts.next() {
                return Err(format!("unexpected argument to 'enumerate' command: {}", unexpected).into());
            }

            let procs = conn.enumerate()?;
            print_procs(procs);
        }
        "call" => {
            let call_usage = "call <procedure> <args...>";
            let proc_id = parts.next().ok_or(call_usage)?.parse::<u16>()?;

            let mut arg_values = Vec::new();
            {
                // To determine the type of value to parse the args into, look up the procedure
                // signature
                let p = conn.procedures()?.get(proc_id as usize).ok_or(microrpc::Error::ProcOutOfRange)?;

                for (arg_type, arg) in p.parameter_types().iter().zip(parts.by_ref()) {
                    arg_values.push(match *arg_type {
                        Type::U8 => Value::U8(arg.parse()?),
                        Type::U16 => Value::U16(arg.parse()?),
                    });
                }
            }

            if let Some(unexpected) = parts.next() {
                return Err(format!("unexpected trailing argument to 'call' command (too many \
                    arguments): {}", unexpected).into());
            }

            if let Some(ret) = conn.call(proc_id, &arg_values)? {
                println!("{}", ret);
            }
        }
        _ => return Err("unknown command".into()),
    }

    Ok(())
}

fn run(m: &ArgMatches) -> Result<(), Box<Error>> {
    let path = m.value_of("endpoint").unwrap();
    let path = Path::new(path);

    // Set up serial port
    let mut serial = serial::open(path)?;
    serial.set_timeout(Duration::from_secs(1))?;
    let mut settings = serial::SerialDevice::read_settings(&serial)?;
    settings.set_baud_rate(serial::Baud115200)?;
    settings.set_char_size(serial::CharSize::Bits8);
    settings.set_flow_control(serial::FlowNone);
    settings.set_parity(serial::ParityNone);
    settings.set_stop_bits(serial::Stop1);
    serial::SerialDevice::write_settings(&mut serial, &settings)?;

    let mut conn = Client::new(util::IoDebug(serial));
    let mut prompt = Editor::<()>::new();

    loop {
        let line = match prompt.readline("REPL> ") {
            Ok(line) => line,
            Err(ReadlineError::Eof) => return Ok(()),           // Ctrl+D
            Err(ReadlineError::Interrupted) => return Ok(()),   // Ctrl+C
            Err(e) => return Err(e.into()),
        };

        prompt.add_history_entry(&line);    // why is this not automatic?

        match execute(&line, &mut conn) {
            Ok(()) => {},
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

fn main() {
    include_str!("../Cargo.toml");
    let m = app_from_crate!(",\n")
        .arg(Arg::with_name("endpoint")
            .takes_value(true)
            .required(true)
            .help("The serial port to connect to"))
        .get_matches();

    match run(&m) {
        Ok(()) => {}
        Err(e) => {
            let mut stderr = stderr();
            writeln!(stderr, "error: {}", e).expect("failed to write to stderr");
        }
    }
}
