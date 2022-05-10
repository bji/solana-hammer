// Creates a gossip spy, fetches tpu info from gossip, writes it to a file.  The file is a bincode serialized
// version of this data structure:
// HashMap<String, SocketAddr>
// Which is a map from validator pubkey to the socket address of its tpu port

use solana_gossip::gossip_service::make_gossip_node;
use solana_sdk::signature::Keypair;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

struct Args
{
    pub gossip_entrypoint : SocketAddr,

    pub output_file : String
}

fn parse_args() -> Result<Args, String>
{
    let mut args = std::env::args();

    let mut gossip_entrypoint = None;

    let mut output_file = None;

    args.nth(0);

    while let Some(arg) = args.nth(0) {
        match arg.as_str() {
            "--gossip-entrypoint" => {
                if gossip_entrypoint.is_some() {
                    return Err("Duplicate --gossip-entrypoint".to_string());
                }
                let entrypoint = args.nth(0).ok_or("--gossip-entrypoint requires a value".to_string())?;
                gossip_entrypoint = Some(entrypoint);
            },

            "--output-file" => {
                if output_file.is_some() {
                    return Err("Duplicate --output-file".to_string());
                }
                let file = args.nth(0).ok_or("--output-file requires a value".to_string())?;
                output_file = Some(file);
            },

            _ => return Err(format!("Invalid argument: {}", arg))
        }
    }

    if let Some(gossip_entrypoint) = gossip_entrypoint {
        let gossip_entrypoint =
            solana_net_utils::parse_host_port(gossip_entrypoint.as_str()).map_err(|e| format!("{}", e))?;
        if let Some(output_file) = output_file {
            Ok(Args { gossip_entrypoint, output_file })
        }
        else {
            Err("--output-file argument is required".to_string())
        }
    }
    else {
        Err("--gossip-entrypoint argument is required".to_string())
    }
}

fn main()
{
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(-1)
    });

    let mut output_file = File::create(args.output_file).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(-1)
    });

    let mut validators = HashMap::<String, SocketAddr>::new();

    // Create a gossip service that will spy for leaders, and let it run forever; retain the "spy ref"
    // that is a reference to the info continuously gathered by gossip
    let exit = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (gossip_service, _, spy_ref) = make_gossip_node(
        Keypair::new(),
        Some(&args.gossip_entrypoint),
        &exit,
        None,
        0,
        false,
        solana_streamer::socket::SocketAddrSpace::Unspecified
    );

    let mut unchanged_iterations = 0;

    loop {
        let last_known_tpu_count = validators.len();

        validators = spy_ref.tpu_peers().into_iter().map(|ci| (format!("{}", ci.id), ci.tpu.clone())).collect();

        let known_tpu_count = validators.len();

        println!("{} validators", known_tpu_count);

        if known_tpu_count == last_known_tpu_count {
            unchanged_iterations += 1;
            if unchanged_iterations == 60 {
                break;
            }
            println!("Waiting for no additions for {} more seconds", (60 - unchanged_iterations));
        }
        else if known_tpu_count > last_known_tpu_count {
            unchanged_iterations = 0;
        }

        std::thread::sleep(Duration::from_secs(1));
    }

    exit.store(true, Ordering::Relaxed);

    gossip_service.join().unwrap();

    let _ = output_file.write(bincode::serialize(&validators).unwrap().as_slice());
}
