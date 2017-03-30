#[macro_use]
extern crate clap;
extern crate microrpc;
extern crate serial;

use microrpc::{Type, Value, Client};

use clap::{App, Arg, ArgMatches};
use serial::prelude::*;

use std::path::Path;
use std::io::{self, stderr, Write, Read};
use std::error::Error;
use std::time::Duration;
use std::thread::sleep;
use std::fmt;

struct HexDump<'a>(&'a [u8]);

impl<'a> fmt::Display for HexDump<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<_>>()
            .join(" "))
    }
}

struct IoDebug<C: Read + Write>(C);

impl<C: Read + Write> Read for IoDebug<C> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            Ok(bytes) => {
                println!("< {}", HexDump(&buf[0..bytes]));
                Ok(bytes)
            }
            Err(e) => Err(e)
        }
    }
}

impl<C: Read + Write> Write for IoDebug<C> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0.write(buf) {
            Ok(bytes) => {
                println!("> {}", HexDump(&buf[0..bytes]));
                Ok(bytes)
            }
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn run(m: &ArgMatches) -> Result<(), Box<Error>> {
    let path = m.value_of("endpoint").unwrap();
    let path = Path::new(path);

    // Set up serial port
    let mut serial = serial::open(path)?;
    serial.set_timeout(Duration::from_secs(5))?;
    let mut settings = serial::SerialDevice::read_settings(&serial)?;
    settings.set_baud_rate(serial::Baud115200)?;
    settings.set_char_size(serial::CharSize::Bits8);
    settings.set_flow_control(serial::FlowNone);
    settings.set_parity(serial::ParityNone);
    settings.set_stop_bits(serial::Stop1);
    serial::SerialDevice::write_settings(&mut serial, &settings)?;

    // When connecting to an Arduino, it will reset now. Wait for a bit until it's back up.
    sleep(Duration::from_secs(2));

    let mut conn = Client::new(IoDebug(serial));

    match m.subcommand() {
        ("list", Some(_)) => {
            let procs = conn.procedures()?;
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
        ("call", Some(m)) => {
            let proc_id = m.value_of("proc_id").unwrap().parse::<u16>().unwrap();
            let args = m.values_of("args").into_iter()
                .flat_map(|args_opt| args_opt)
                .collect::<Vec<_>>();

            let mut arg_values = Vec::new();
            {
                // To determine the type of value to parse the args into, look up the procedure
                // signature
                let p = conn.procedures()?.get(proc_id as usize).ok_or(microrpc::Error::ProcOutOfRange)?;

                if p.parameter_types().len() != args.len() {
                    Err(format!("the procedure takes {} arguments, {} provided",
                    p.parameter_types().len(),
                    args.len()))?
                }

                for (arg_type, arg) in p.parameter_types().iter().zip(args.iter()) {
                    arg_values.push(match *arg_type {
                        Type::U8 => Value::U8(arg.parse()?),
                        Type::U16 => Value::U16(arg.parse()?),
                    });
                }
            }

            if let Some(ret) = conn.call(proc_id, &arg_values)? {
                println!("{}", ret);
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

fn main() {
    include_str!("../Cargo.toml");
    let m = app_from_crate!(",\n")
        .subcommand(App::new("list")
            .help("List the exported procedures of the endpoint"))
        .subcommand(App::new("call")
            .help("Call a procedure identified by its ID")
            .arg(Arg::with_name("proc_id")
                .help("The ID of the procedure to call (run the `list` subcommand for a list)")
                .required(true)
                .validator(|id| id.parse::<u16>()
                    .map(|_| ())
                    .map_err(|e| e.to_string())))
            .arg(Arg::with_name("args")
                .help("Arguments to pass to the procedure")
                .multiple(true)))
        .arg(Arg::with_name("endpoint")
            .takes_value(true)
            .required(true)
            .help("The µRPC server endpoint to connect to (path to a file)"))
        .get_matches();

    match run(&m) {
        Ok(()) => {}
        Err(e) => {
            let mut stderr = stderr();
            writeln!(stderr, "error: {}", e).expect("failed to write to stderr");
        }
    }
}
