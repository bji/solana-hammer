// Creates a number of accounts by issuing "create account" commands to a hammer program.  This is to seed the
// hammer program with a number of accounts that can be used as "contention accounts" during client executions
// of a hammer.

use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Signer};
use solana_sdk::transaction::Transaction;

struct Args
{
    pub fee_payer : String,

    pub rpc_node : String,

    pub program_id : String,

    pub account_count : u32
}

fn parse_args() -> Result<Args, String>
{
    let mut args = std::env::args();

    let mut fee_payer = None;

    let mut rpc_node = None;

    let mut program_id = None;

    let mut account_count = None;

    args.nth(0);

    while let Some(arg) = args.nth(0) {
        match arg.as_str() {
            "--fee-payer" => {
                if fee_payer.is_some() {
                    return Err("Duplicate --fee-payer".to_string());
                }
                fee_payer = Some(args.nth(0).ok_or("--fee-payer requires a value".to_string())?);
            },

            "--rpc-node" => {
                if rpc_node.is_some() {
                    return Err("Duplicate --rpc-node".to_string());
                }
                rpc_node = Some(args.nth(0).ok_or("--rpc-node requires a value".to_string())?);
            },

            "--program-id" => {
                if program_id.is_some() {
                    return Err("Duplicate --program-id".to_string());
                }
                program_id = Some(args.nth(0).ok_or("--program-id requires a value".to_string())?);
            },

            "--account-count" => {
                if account_count.is_some() {
                    return Err("Duplicate --account-count".to_string());
                }
                account_count = Some(
                    args.nth(0)
                        .ok_or("--output-file requires a value".to_string())?
                        .parse::<u32>()
                        .map_err(|e| e.to_string())?
                );
            },

            _ => return Err(format!("Invalid argument: {}", arg))
        }
    }

    if fee_payer.is_none() {
        return Err("--fee-payer argument is required".to_string());
    }

    if rpc_node.is_none() {
        return Err("--rpc-node argument is required".to_string());
    }

    if program_id.is_none() {
        return Err("--program-id argument is required".to_string());
    }

    if account_count.is_none() {
        account_count = Some(100);
    }

    let fee_payer = fee_payer.unwrap();
    let rpc_node = rpc_node.unwrap();
    let program_id = program_id.unwrap();
    let account_count = account_count.unwrap();

    Ok(Args { fee_payer, rpc_node, program_id, account_count })
}

fn make_pubkey(s : &str) -> Result<Pubkey, String>
{
    let mut bytes = [0_u8; 32];

    match bs58::decode(s).into_vec().map_err(|e| format!("{}", e)) {
        Ok(v) => {
            if v.len() == 32 {
                bytes.copy_from_slice(v.as_slice());
                return Ok(Pubkey::new(&bytes));
            }
        },
        Err(_) => ()
    }

    // Couldn't decode it as base58, try reading it as a file
    let key = std::fs::read_to_string(s).map_err(|e| format!("Failed to read key file '{}': {}", s, e))?;

    let mut v : Vec<&str> = key.split(",").into_iter().collect();

    if v.len() < 2 {
        return Err("Short key file".to_string());
    }

    v[0] = &v[0][1..];
    let last = v.last().unwrap().clone();
    v.pop();
    v.push(&last[..(last.len() - 1)]);

    let v : Vec<u8> = v.into_iter().map(|s| u8::from_str_radix(s, 10).unwrap()).collect();

    let dalek_keypair = ed25519_dalek::Keypair::from_bytes(v.as_slice())
        .map_err(|e| format!("Invalid key file '{}' contents: {}", s, e))?;
    Ok(Pubkey::new(&dalek_keypair.public.to_bytes()))
}

fn write(
    w : &mut dyn std::io::Write,
    b : &[u8]
)
{
    w.write(b).expect("Internal fail to write");
}

fn main()
{
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(-1)
    });

    let fee_payer = read_keypair_file(args.fee_payer.clone()).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", args.fee_payer, e);
        std::process::exit(-1)
    });

    let rpc_client = RpcClient::new_with_commitment(args.rpc_node, CommitmentConfig::confirmed());

    let program_id = make_pubkey(args.program_id.as_str()).unwrap_or_else(|e| {
        eprintln!("Failed to make pubkey from {}: {}", args.program_id, e);
        std::process::exit(-1)
    });

    let mut account_count = args.account_count;

    let mut account_index = 0_u64;

    loop {
        if account_count == 0 {
            break;
        }
        println!("({} accounts remain to be created)", account_count);
        // Create up to 8 at a time
        let mut data = vec![];
        let mut accounts = vec![fee_payer.pubkey(), Pubkey::new(&[0_u8; 32])];
        let create_count = std::cmp::min(account_count, 8);
        for _ in 0..create_count {
            let seed = &account_index.to_le_bytes()[0..7];
            account_index += 1;
            let (address, bump_seed) = Pubkey::find_program_address(&[seed], &program_id);
            let mut seed = seed.to_vec();
            seed.push(bump_seed);
            let size = 100_u16;
            write(&mut data, &[0_u8]);
            write(&mut data, &fee_payer.pubkey().to_bytes());
            write(&mut data, &address.to_bytes());
            write(&mut data, &seed);
            write(&mut data, &size.to_le_bytes());
            accounts.push(address);
        }

        let instructions = vec![Instruction::new_with_bytes(
            program_id.clone(),
            data.as_slice(),
            accounts.into_iter().map(|pk| AccountMeta::new(pk, false)).collect()
        )];

        let recent_blockhash = match rpc_client.get_latest_blockhash() {
            Ok(recent_blockhash) => recent_blockhash,
            Err(err) => {
                eprintln!("Failed to get recent blockhash: {}", err);
                continue;
            }
        };

        let transaction = Transaction::new(
            &vec![&fee_payer],
            Message::new(&instructions, Some(&fee_payer.pubkey())),
            recent_blockhash
        );

        println!(
            "Submitting transaction {} to RPC\n  Signature: {}",
            base64::encode(bincode::serialize(&transaction).expect("encode")),
            transaction.signatures[0]
        );

        match rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(_) => (),
            Err(e) => println!("TX failed: {}", e)
        }

        // Don't try again -- if failed, it was likely due to an account already existing, which is not really
        // an error
        account_count -= create_count;
    }
}
