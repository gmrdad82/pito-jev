#[path = "support/stand_in.rs"]
mod stand_in;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [port, key] = args.as_slice() else {
        return usage();
    };
    let Ok(port) = port.parse() else {
        return usage();
    };
    match stand_in::start(port, key) {
        Ok(address) => {
            eprintln!("stand_in: answering on http://{address}/api/v1/systemone");
            loop {
                std::thread::park();
            }
        }
        Err(err) => {
            eprintln!("stand_in: {err}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: stand_in <PORT> <KEY>");
    ExitCode::from(2)
}
