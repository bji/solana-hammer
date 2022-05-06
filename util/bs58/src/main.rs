use std::io::Read;
use std::io::Write;

fn usage_exit(
    w : &mut dyn std::io::Write,
    exit_code : i32
) -> !
{
    write!(w, "\nUsage: bs58 -e [string] [< bytes]\n").expect("");
    write!(w, "         -- Encodes to base58 a value provided either on the command line or via stdin\n\n").expect("");
    write!(w, "       bs58 -d [encoded_string] [< encoded_string]\n").expect("");
    write!(w, "         -- Decodes from base58 a string provided either on the command line or via stdin\n\n")
        .expect("");

    std::process::exit(exit_code)
}

fn input_bytes(arg : Option<String>) -> Vec<u8>
{
    if let Some(arg) = arg {
        arg.as_bytes().into()
    }
    else {
        let mut r = std::io::stdin();
        let mut v = vec![];
        match r.read_to_end(&mut v) {
            Ok(_) => v,
            Err(e) => {
                eprintln!("Failed to read stdin: {}", e);
                std::process::exit(-1)
            }
        }
    }
}

fn main()
{
    let mut args = std::env::args();

    match args.nth(1).unwrap_or("".to_string()).as_str() {
        "-h" | "--help" | "help" => usage_exit(&mut std::io::stdout(), 0),
        "-e" => print!("{}", bs58::encode(input_bytes(args.nth(0))).into_string()),
        "-d" => {
            let v = bs58::decode(input_bytes(args.nth(0))).into_vec();
            match v {
                Ok(v) => {
                    std::io::stdout().write(v.as_slice()).expect("");
                },
                Err(e) => {
                    eprintln!("Invalid input: {}", e);
                    std::process::exit(-1);
                }
            }
        },
        _ => usage_exit(&mut std::io::stderr(), -1)
    }
}
