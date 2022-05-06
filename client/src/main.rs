#![allow(dead_code)]
#![allow(unused_variables)]

use rand::RngCore;
//use solana_sdk::instruction::Instruction;
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::message::Message;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair, Signer};
use solana_sdk::system_instruction;
use solana_sdk::transaction::Transaction;
use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

// Perform up to N concurrent transactions

// The implementation will cycle between a minimum number of open accounts and a maximum number:
// As long as it is below the minimum number, it will have a greater chance to create accounts than to
// delete.  It will continue in this mode until it hits the maximum number.  Then it will switch and start
// deleting more accounts than it creates, until it hits the minimum number, and then it will start creating
// more.  In this way it will cycle between its minimum and maximum number of accounts.

// These values are tuned from actual compute units costs of commands run by the hammer on-chain program.  They
// should be as close as possible to the actual compute unit cost, and should err on the side of over-estimating
// costs if necessary.  These are hardcoded from observed values and could be made overridable by command line
// parameters if that ends up being useful.
const MAX_TX_COMPUTE_UNITS : u32 = 1_400_000;
const MAX_INSTRUCTION_COMPUTE_UNITS : u32 = 600_000;
const FAIL_COMMAND_COST : u32 = 1000;
const CPU_COMMAND_COST_PER_ITERATION : u32 = 5000;
const ALLOC_COMMAND_COST : u32 = 100;
const FREE_COMMAND_COST : u32 = 100;
const SYSVAR_COMMAND_COST : u32 = 500;

// These govern the shape of transactions; these could be made into command line parameters if that was useful
const FAIL_COMMAND_CHANCE : f32 = 0.001;
const MAX_ALLOC_BYTES : u32 = 1000;
const RECENT_BLOCKHASH_REFRESH_INTERVAL_SECS : u64 = 30;
const CONTENTION_ACCOUNT_COUNT : u32 = 20;

enum RpcSpec
{
    // Find RPC servers on gossip
    FindFromGossip // Use
}

struct RecentBlockhashFetcher
{
    recent_blockhash : Arc<Mutex<Option<Hash>>>
}

impl RecentBlockhashFetcher
{
    pub fn new(rpc_clients : Arc<Mutex<RpcClients>>) -> Self
    {
        let recent_blockhash : Arc<Mutex<Option<Hash>>> = Arc::new(Mutex::new(None));

        {
            let recent_blockhash = recent_blockhash.clone();

            std::thread::spawn(move || {
                loop {
                    let rpc_client = rpc_clients.lock().unwrap().get();
                    if let Some(next_recent_blockhash) = rpc_client.get_latest_blockhash().ok() {
                        // Hacky, but it's really hard to coordinate recent blockhash across rpc servers.  So just
                        // don't present a block hash until 1 second after it is fetched
                        std::thread::sleep(Duration::from_secs(1));
                        *(recent_blockhash.lock().unwrap()) = Some(next_recent_blockhash);
                        // Wait 30 seconds to fetch again
                        std::thread::sleep(Duration::from_secs(RECENT_BLOCKHASH_REFRESH_INTERVAL_SECS));
                    }
                }
            });
        }

        Self { recent_blockhash }
    }

    // Hacky, but it's really hard to coordinate recent blockhash across rpc servers.  So just don't present
    // a block hash until 5 seconds after it is fetched
    pub fn get(&mut self) -> Hash
    {
        loop {
            {
                let rb = self.recent_blockhash.lock().unwrap();

                if let Some(rb) = *rb {
                    return rb.clone();
                }
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

struct Args
{
    pub keys_dir : String,

    // If set, will find RPC servers from gossip and use the ones it finds in addition to any specified via
    // --rpc-server
    pub rpc_from_gossip : Option<String>,

    // Explicitly named RPC servers
    pub rpc_servers : Vec<String>,

    // Source of fee payer funds.  An external process must keep the balance up-to-date in this account.
    // The clients will pull SOL from this account as needed.  It should have enough SOL to support all of
    // the concurrent connections.
    pub funds_source : Keypair,

    // The set of program ids that are all duplicates of the test program.  Multiple can be used to better simulate
    // multiple programs running.
    pub program_ids : Vec<Pubkey>,

    // Total number of transactions to run before the "cleanup" step of deleting all accounts that were created.  If
    // None, will run indefinitely.  If running indefinitely, it would be the responsibility of an external program to
    // delete old accounts after this program has exited.
    pub total_transactions : Option<u64>,

    // Total number of threads to run at once, which implies a total number of concurrent transactions to run
    // at one time since each transaction is handled in a blocking manner.  Each thread will use its own
    // fee payer for maximum concurrency
    pub num_threads : u32
}

#[derive(Clone)]
struct Account
{
    pub address : Pubkey,

    seed : Vec<u8>,

    pub size : u16
}

// This structure keeps all state associated with transactions executed by this instance
struct Accounts
{
    pub map : HashMap<String, Arc<Account>>
}

impl Accounts
{
    pub fn new() -> Self
    {
        Self { map : HashMap::<String, Arc<Account>>::new() }
    }

    pub fn count(&self) -> usize
    {
        return self.map.len();
    }

    pub fn add_new_account(
        &mut self,
        account : Account
    )
    {
        self.add_existing_account(Arc::new(account));
    }

    pub fn add_existing_account(
        &mut self,
        rc : Arc<Account>
    )
    {
        let key = format!("{}", rc.address);

        self.map.insert(key, rc.clone());
    }

    pub fn get_random_account(
        &mut self,
        rng : &mut rand::rngs::ThreadRng
    ) -> Option<Arc<Account>>
    {
        self.random_key(rng).map(|k| self.map.get(&k).unwrap().clone())
    }

    pub fn take_random_account(
        &mut self,
        rng : &mut rand::rngs::ThreadRng
    ) -> Option<Arc<Account>>
    {
        self.random_key(rng).map(|k| self.map.remove(&k).unwrap())
    }

    fn random_key(
        &self,
        rng : &mut rand::rngs::ThreadRng
    ) -> Option<String>
    {
        let len = self.map.len();

        if len == 0 {
            None
        }
        else {
            Some(self.map.keys().nth((rng.next_u32() as usize) % len).unwrap().clone())
        }
    }
}

struct State
{
    pub accounts : Arc<Mutex<Accounts>>
}

struct CommandAccount
{
    pub address : Pubkey,

    pub is_write : bool,

    pub is_signer : bool
}

struct CommandAccounts
{
    accounts : HashMap<String, CommandAccount>
}

impl CommandAccounts
{
    pub fn new() -> Self
    {
        Self { accounts : HashMap::<String, CommandAccount>::new() }
    }

    pub fn count(&self) -> usize
    {
        self.accounts.len()
    }

    pub fn add_command_accounts(
        &mut self,
        other : &CommandAccounts
    )
    {
        for (key, command_account) in &other.accounts {
            self.add(&command_account.address, command_account.is_write, command_account.is_signer);
        }
    }

    pub fn add(
        &mut self,
        pubkey : &Pubkey,
        is_write : bool,
        is_signer : bool
    )
    {
        let key = pubkey_to_string(pubkey);

        if let Some(existing) = self.accounts.get_mut(&key) {
            if is_write {
                existing.is_write = true;
            }
            if is_signer {
                existing.is_signer = true;
            }
        }
        else {
            self.accounts.insert(key, CommandAccount { address : pubkey.clone(), is_write, is_signer });
        }
    }

    pub fn get_account_metas(&self) -> Vec<AccountMeta>
    {
        self.accounts
            .values()
            .map(|a| {
                if a.is_write {
                    AccountMeta::new(a.address.clone(), a.is_signer)
                }
                else {
                    AccountMeta::new_readonly(a.address.clone(), a.is_signer)
                }
            })
            .collect()
    }
}

struct RpcClients
{
    clients : Vec<Rc<RpcClient>>,

    last_index : usize
}

unsafe impl Send for RpcClients
{
}

impl RpcClients
{
    pub fn new(rpc_servers : Vec<String>) -> Self
    {
        Self {
            clients : rpc_servers
                .iter()
                .map(|s| Rc::new(RpcClient::new_with_commitment(s, CommitmentConfig::confirmed())))
                .collect(),
            last_index : 0
        }
    }

    // Obtains the next client that uses confirmed committment, in a round-robin fashion
    pub fn get(&mut self) -> Rc<RpcClient>
    {
        self.last_index = (self.last_index + 1) % self.clients.len();
        self.clients[self.last_index].clone()
    }
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

fn locked_println(
    lock : &Arc<Mutex<()>>,
    msg : String
)
{
    let _ = lock.lock();

    println!("{}", msg);
}

fn pubkey_to_string(pubkey : &Pubkey) -> String
{
    format!("{}", pubkey)
}

fn parse_args() -> Result<Args, String>
{
    let mut args = std::env::args();

    let mut keys_dir = None;

    let mut rpc_from_gossip = None;

    let mut rpc_servers = Vec::<String>::new();

    let mut funds_source = None;

    let mut program_ids = Vec::<Pubkey>::new();

    let mut total_transactions = None;

    let mut num_threads = None;

    args.nth(0);

    while let Some(arg) = args.nth(0) {
        match arg.as_str() {
            "--keys-dir" => {
                if keys_dir.is_some() {
                    return Err("Duplicate --keys-dir argument".to_string());
                }
                else {
                    keys_dir = Some(args.nth(0).ok_or("--keys-dir requires a value".to_string())?);
                }
            },

            "--rpc-from-gossip" => {
                if rpc_from_gossip.is_some() {
                    return Err("Duplicate --rpc-from-gossip argument".to_string());
                }
                if !rpc_servers.is_empty() {
                    return Err("Only one of --rpc-from-gossip and --rpc-server may be used".to_string());
                }
                let entrypoint = args.nth(0).ok_or("--rpc-from-gossip requires a value".to_string())?;
                rpc_from_gossip = Some(entrypoint);
            },

            "--rpc-server" => {
                if rpc_from_gossip.is_some() {
                    return Err("Only one of --rpc-server and --rpc-from-gossip may be used".to_string());
                }
                let rpc_server = args.nth(0).ok_or("--rpc-server requires a value".to_string())?;
                rpc_servers.push(rpc_server);
            },

            "--funds-source" => {
                if funds_source.is_some() {
                    return Err("Duplicate --funds-source argument".to_string());
                }
                let file = args.nth(0).ok_or("--funds-source requires a value".to_string())?;
                funds_source = Some(read_keypair_file(file.clone()).unwrap_or_else(|e| {
                    eprintln!("Failed to read {}", file);
                    std::process::exit(-1)
                }));
            },

            "--program-id" => {
                let program_id = make_pubkey(args.nth(0).as_ref().ok_or("--program-id requires a value".to_string())?)?;
                if program_ids.iter().find(|&e| e == &program_id).is_some() {
                    return Err(format!("Duplicate program id: {}", program_id));
                }
                program_ids.push(program_id);
            },

            "--total-transactions" => {
                if total_transactions.is_some() {
                    return Err("Duplicate --total-transactions argument".to_string());
                }
                else {
                    total_transactions = Some(
                        args.nth(0)
                            .ok_or("--total-transactions requires a value".to_string())?
                            .parse::<u64>()
                            .map_err(|e| e.to_string())?
                    )
                }
            },

            "--num-threads" => {
                if num_threads.is_some() {
                    return Err("Duplicate --num-threads argument".to_string());
                }
                else {
                    num_threads = Some(
                        args.nth(0)
                            .ok_or("--num-threads requires a value".to_string())?
                            .parse::<u32>()
                            .map_err(|e| e.to_string())?
                    )
                }
            },

            _ => return Err(format!("Invalid argument: {}", arg))
        }
    }

    let keys_dir = keys_dir.unwrap_or("./keys".to_string());

    if rpc_from_gossip.is_none() && rpc_servers.is_empty() {
        return Err("If --rpc-from-gossip is not set, then at least one RPC server specified via --rpc-server option \
                    is required"
            .to_string());
    }

    if funds_source.is_none() {
        return Err("--funds-source argument is required".to_string());
    }

    let funds_source = funds_source.unwrap();

    if program_ids.is_empty() {
        return Err("At least one program id specified via --program-id option is required".to_string());
    }

    let num_threads = num_threads.unwrap_or(8);

    if num_threads == 0 {
        return Err("--num-threads must takea nonzero argument".to_string());
    }

    Ok(Args { keys_dir, rpc_from_gossip, rpc_servers, funds_source, program_ids, total_transactions, num_threads })
}

fn write(
    w : &mut dyn std::io::Write,
    b : &[u8]
)
{
    w.write(b).expect("Internal fail to write");
}

fn add_create_account_command(
    fee_payer : &Pubkey,
    account : &Account,
    command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    command_accounts.add(fee_payer, true, true);
    command_accounts.add(&account.address, true, false);
    command_accounts.add(&Pubkey::new(&[0_u8; 32]), false, false);

    write(w, &[0_u8]);

    write(w, &fee_payer.to_bytes());

    write(w, &account.address.to_bytes());

    write(w, &account.seed);

    write(w, &account.size.to_le_bytes())
}

fn add_delete_account_command(
    fee_payer : &Pubkey,
    account : &Account,
    command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    command_accounts.add(fee_payer, true, true);
    command_accounts.add(&account.address, true, false);
    command_accounts.add(&Pubkey::new(&[0_u8; 32]), false, false);

    write(w, &[1_u8]);

    write(w, &fee_payer.to_bytes());
    write(w, &account.address.to_bytes());
}

fn add_cpu_command(
    loop_count : u32,
    _command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    write(w, &[2_u8]);

    write(w, &loop_count.to_le_bytes())
}

fn add_alloc_command(
    amount : u32,
    index : u8,
    _command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    write(w, &[3_u8]);

    write(w, &amount.to_le_bytes());

    write(w, &[index])
}

fn add_free_command(
    index : u8,
    _command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    write(w, &[4_u8]);

    write(w, &[index])
}

fn add_cpi_command(
    program_id : &Pubkey,
    accounts : &Vec<CommandAccount>,
    data : &Vec<u8>,
    seed : Option<Vec<u8>>,
    command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    for account in accounts {
        command_accounts.add(&account.address, account.is_write, account.is_signer);
    }

    write(w, &[5_u8]);

    write(w, &program_id.to_bytes());

    write(w, &[accounts.len() as u8]);

    for account in accounts {
        write(w, &account.address.to_bytes());

        write(w, &[if account.is_write { 1_u8 } else { 0_u8 }]);

        write(w, &[if account.is_signer { 1_u8 } else { 0_u8 }]);
    }

    write(w, &(data.len() as u16).to_le_bytes());

    write(w, &data.as_slice());

    if let Some(seed) = seed {
        write(w, &[seed.len() as u8]);
        write(w, &seed.as_slice())
    }
    else {
        write(w, &[0_u8])
    }
}

fn add_sysvar_command(
    _command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    write(w, &[6_u8])
}

fn add_fail_command(
    error_code : u8,
    _command_accounts : &mut CommandAccounts,
    w : &mut dyn std::io::Write
)
{
    write(w, &[7_u8]);

    write(w, &[error_code])
}

fn random_chance(
    rng : &mut rand::rngs::ThreadRng,
    pct : f32
) -> bool
{
    ((rng.next_u32() as f64) / (u32::MAX as f64)) < (pct as f64)
}

// Create an instruction that creates an account, deletes an account, or does some amount of other random stuff.
// Returns:
// (data_buffer,          // buffer of command data
//  actual_create_count,  // number of accounts created
//  actual_delete_count,  // number of accounts deleted
//  actual_compute_usage) // compute units used
fn make_command(
    rng : &mut rand::rngs::ThreadRng,
    fee_payer : &Pubkey,
    accounts : &mut Arc<Mutex<Accounts>>,
    allocated_indices : &mut HashSet<u8>,
    compute_budget : u32,
    command_accounts : &mut CommandAccounts
) -> (Vec<u8>, u32)
{
    let mut data = vec![];

    // Chance of straight up fail
    if random_chance(rng, FAIL_COMMAND_CHANCE) {
        add_fail_command(((rng.next_u32() % 255) + 1) as u8, command_accounts, &mut data);
        (data, FAIL_COMMAND_COST)
    }
    else {
        let mut v = vec![];
        if compute_budget >= CPU_COMMAND_COST_PER_ITERATION {
            v.push(0); // cpu
        }
        if (compute_budget >= ALLOC_COMMAND_COST) && (allocated_indices.len() < 256) {
            v.push(1); // alloc
        }
        if (compute_budget >= FREE_COMMAND_COST) && (allocated_indices.len() > 0) {
            v.push(2); // free
        }
        if compute_budget >= SYSVAR_COMMAND_COST {
            v.push(4); // sysvar
        }
        if v.len() == 0 {
            // No command can fit, so do nothing but use all compute budget
            return (data, compute_budget);
        }
        match v[(rng.next_u32() as usize) % v.len()] {
            0 => {
                // cpu
                let max_iterations = compute_budget / CPU_COMMAND_COST_PER_ITERATION;
                let iterations = (rng.next_u32() % max_iterations) + 1;
                add_cpu_command(iterations, command_accounts, &mut data);
                (data, iterations * CPU_COMMAND_COST_PER_ITERATION)
            },
            1 => {
                // alloc
                // Find first unallocated index
                let mut index = 0;
                loop {
                    if !allocated_indices.contains(&index) {
                        break;
                    }
                    index += 1;
                }
                add_alloc_command((rng.next_u32() % MAX_ALLOC_BYTES) + 1, index, command_accounts, &mut data);
                allocated_indices.insert(index);
                (data, ALLOC_COMMAND_COST)
            },
            2 => {
                // free
                let index = *allocated_indices.iter().nth((rng.next_u32() as usize) % allocated_indices.len()).unwrap();
                add_free_command(index, command_accounts, &mut data);
                allocated_indices.remove(&index);
                (data, FREE_COMMAND_COST)
            },
            _ => {
                // sysvar
                add_sysvar_command(command_accounts, &mut data);
                (data, SYSVAR_COMMAND_COST)
            }
        }
    }
}

fn transaction_thread_function(
    thread_number : u32,
    print_lock : Arc<Mutex<()>>,
    rpc_clients : Arc<Mutex<RpcClients>>,
    recent_blockhash_fetcher : Arc<Mutex<RecentBlockhashFetcher>>,
    program_ids : Vec<Pubkey>,
    funds_source : Keypair,
    mut accounts : Arc<Mutex<Accounts>>,
    total_transactions : Option<Arc<Mutex<u64>>>
)
{
    // Make a fee payer for this thread
    let fee_payer = Keypair::new();

    let fee_payer_pubkey = fee_payer.pubkey();

    //let fee_payer = Keypair::from_bytes(&funds_source.to_bytes()).expect("");

    //let fee_payer_pubkey = fee_payer.pubkey();

    // Make a random number generator for this thread
    let mut rng = rand::thread_rng();

    loop {
        if let Some(ref total_transactions) = total_transactions {
            let mut total_transactions = total_transactions.lock().unwrap();
            if *total_transactions == 0 {
                break;
            }
            *total_transactions -= 1;
        }

        let program_id = &program_ids[(rng.next_u32() as usize) % program_ids.len()];

        let mut allocated_indices = HashSet::<u8>::new();

        let mut compute_budget =
            (rng.next_u32() % (MAX_TX_COMPUTE_UNITS - CPU_COMMAND_COST_PER_ITERATION)) + CPU_COMMAND_COST_PER_ITERATION;

        // Vector to hold data for each individual command:
        // (data, command_accounts, compute_usage)
        let mut command_data = vec![];

        let mut total_data_size = 0;

        // Create the commands one by one
        while (total_data_size < 1100) && (compute_budget > 0) {
            let mut command_accounts = CommandAccounts::new();
            let (data, actual_compute_usage) = make_command(
                &mut rng,
                &fee_payer_pubkey,
                &mut accounts,
                &mut allocated_indices,
                compute_budget,
                &mut command_accounts
            );
            total_data_size += data.len();
            command_data.push((data, command_accounts, actual_compute_usage));
            compute_budget -= actual_compute_usage;
        }

        // Now turn some subsets of commands into CPI invocations
        //                        4 => {
        //                            // cpi
        //                            let sub_command_accounts = CommandAccounts::new();
        //                            let sub_data = Vec::<u8>::new();
        //                            let sub_other_count = ((rng.next_u32() as u8) % other_count) + 1;
        //                            let program_id =
        //                                program_ids.iter().nth((rng.next_u32() as usize) % program_ids.len()).unwrap();
        //                            other_count -= sub_other_count;
        //                            make_commands(
        //                                rng,
        //                                program_id,
        //                                program_ids,
        //                                fee_payer,
        //                                accounts,
        //                                allocated_indices,
        //                                0,
        //                                0,
        //                                sub_other_count,
        //                                &mut sub_command_accounts,
        //                                created_accounts,
        //                                deleted_accounts,
        //                                &mut sub_data
        //                            );
        //                            // Not doing signed cpi at the moment, that requires some sophisticated tracking of
        //                            // account seeds
        //                            add_cpi_command(
        //                                program_id,
        //                                &sub_command_accounts,
        //                                &sub_data,
        //                                None,
        //                                sub_command_accounts,
        //                                into
        //                            );
        //                            for account in sub_command_accounts {
        //                                command_accounts.add(account.pubkey, account.is_write, account.is_signer);
        //                            }
        //                        },

        // Group commands together into instructions
        let mut instructions = vec![];

        while command_data.len() > 0 {
            let mut instruction_compute_units = 0;
            let mut command_count = (rng.next_u32() % (command_data.len() as u32)) + 1;
            let mut data = vec![];
            let mut instruction_accounts = CommandAccounts::new();
            while command_count > 0 {
                let (command_data, command_accounts, compute_units) = command_data.remove(0);
                instruction_compute_units += compute_units;
                if instruction_compute_units > MAX_INSTRUCTION_COMPUTE_UNITS {
                    break;
                }
                data.extend(command_data);
                instruction_accounts.add_command_accounts(&command_accounts);
                command_count -= 1;
            }
            // Some commands may be empty, so it's possible for the command data to be empty
            if data.len() > 0 {
                // Add some random accounts to the instruction to create account contention, up to 12 accounts
                if instruction_accounts.count() < 12 {
                    let total_accounts = rng.next_u32() % ((12 - instruction_accounts.count()) as u32);
                    if total_accounts > 0 {
                        let read_only_accounts = rng.next_u32() % total_accounts;
                        let read_write_accounts = total_accounts - read_only_accounts;
                        for _ in 0..read_only_accounts {
                            if let Some(account) = accounts.lock().unwrap().get_random_account(&mut rng) {
                                instruction_accounts.add(&account.address, false, false);
                            }
                        }
                        for _ in 0..read_write_accounts {
                            if let Some(account) = accounts.lock().unwrap().get_random_account(&mut rng) {
                                instruction_accounts.add(&account.address, true, false);
                            }
                        }
                    }
                }
                instructions.push(Instruction::new_with_bytes(
                    program_id.clone(),
                    data.as_slice(),
                    instruction_accounts.get_account_metas()
                ));
            }
        }

        let rpc_client = rpc_clients.lock().unwrap().get();

        // Execute the transaction

        // First get recent blockhash
        let recent_blockhash = recent_blockhash_fetcher.lock().unwrap().get();

        // If the fee payer is low on funds, try to transfer funds from the funds source
        let balance = rpc_client.get_balance(&fee_payer_pubkey).unwrap_or(0);

        // When balance falls below 0.01 SOL, take 0.01 SOL from funds source
        if balance < (LAMPORTS_PER_SOL / 100) {
            transfer_lamports(
                &rpc_client,
                &funds_source,
                &funds_source,
                &fee_payer_pubkey,
                LAMPORTS_PER_SOL,
                &recent_blockhash
            );
        }

        let transaction =
            Transaction::new(&vec![&fee_payer], Message::new(&instructions, Some(&fee_payer_pubkey)), recent_blockhash);

        locked_println(
            &print_lock,
            format!(
                "Thread {}: Submitting transaction {}\n  Signature: {}",
                thread_number,
                base64::encode(bincode::serialize(&transaction).expect("encode")),
                transaction.signatures[0]
            )
        );

        let start = SystemTime::now();

        //match rpc_client.send_and_confirm_transaction(&transaction) {
        match rpc_client.send_transaction(&transaction) {
            Ok(_) => {
                let duration = SystemTime::now().duration_since(start).map(|d| d.as_millis()).unwrap_or(0);
                locked_println(&print_lock, format!("Thread {}: TX success in {} ms", thread_number, duration));
            },
            Err(e) => {
                let duration = SystemTime::now().duration_since(start).map(|d| d.as_millis()).unwrap_or(0);
                locked_println(&print_lock, format!("Thread {}: TX failed in {} ms: {}", thread_number, duration, e));
            }
        }
    }

    // Take back all SOL from the fee payer
    let rpc_client = rpc_clients.lock().unwrap().get();

    loop {
        if let Some(balance) = rpc_client.get_balance(&fee_payer_pubkey).ok() {
            if balance == 0 {
                break;
            }
            if funds_source.pubkey() == fee_payer.pubkey() {
                break;
            }
            match rpc_client.get_latest_blockhash() {
                Ok(recent_blockhash) => {
                    transfer_lamports(
                        &rpc_client,
                        &funds_source,
                        &fee_payer,
                        &funds_source.pubkey(),
                        balance,
                        &recent_blockhash
                    );
                },
                Err(_) => ()
            }
        }
    }
}

fn main()
{
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(-1)
    });

    if args.rpc_from_gossip.is_some() {
        eprintln!("--rpc-from-gossip is not supported yet");
        std::process::exit(-1);
    }

    //    if let Some(rpc_from_gossip) = args.rpc_from_gossip {
    //        // Discover RPC servers from gossip
    //        let entrypoint_addr = solana_net_utils::parse_host_port(&rpc_from_gossip).unwrap_or_else(|e| {
    //            eprintln!("Failed to parse --rpc-from-gossip address: {}", e);
    //            std::process::exit(-1)
    //        });
    //        println!("Finding cluster entry: {:?}", entrypoint_addr);
    //        let (gossip_nodes, _) = discover(
    //            None, // keypair
    //            Some(&entrypoint_addr),
    //            None,                               // num_nodes
    //            std::time::Duration::from_secs(60), // timeout
    //            None,                               // find_node_by_pubkey
    //            Some(&entrypoint_addr),             // find_node_by_gossip_addr
    //            None,                               // my_gossip_addr
    //            0,                                  // my_shred_version
    //            SocketAddrSpace::Unspecified
    //        )
    //        .unwrap_or_else(|err| {
    //            eprintln!("Failed to discover {} node: {:?}", entrypoint_addr, err);
    //            std::process::exit(-1);
    //        })
    //        .iter()
    //        .filter(|ci| ContactInfo::is_valid_address(ci.rpc));
    //
    //        println!("Found {} nodes", gossip_nodes.len());
    //
    //        gossip_nodes[0].rpc
    //    }

    let mut rng = rand::thread_rng();

    let rpc_clients = Arc::new(Mutex::new(RpcClients::new(args.rpc_servers.clone())));

    let program_ids = args.program_ids;

    // Recent blockhash is updated every 30 seconds
    let recent_blockhash_fetcher = Arc::new(Mutex::new(RecentBlockhashFetcher::new(rpc_clients.clone())));

    let accounts = Arc::new(Mutex::new(Accounts::new()));

    let funds_source = Keypair::from_bytes(&args.funds_source.to_bytes()).unwrap();

    // Create accounts, so that there is some chance of account contention
    let mut accounts_to_create = CONTENTION_ACCOUNT_COUNT;
    if accounts_to_create > (args.num_threads * 4) {
        accounts_to_create = args.num_threads * 4;
    }
    while accounts_to_create > 0 {
        println!("({} accounts remain to be created)", accounts_to_create);
        let rpc_client = rpc_clients.lock().unwrap().get();
        let mut create_count = accounts_to_create;
        if create_count > 8 {
            create_count = 8;
        }
        let mut command_accounts = CommandAccounts::new();
        let mut data = vec![];
        for _ in 0..create_count {
            // Generate a seed to create from
            let seed = &rng.next_u64().to_le_bytes()[0..7];
            let (address, bump_seed) = Pubkey::find_program_address(&[seed], &program_ids[0]);
            let mut seed = seed.to_vec();
            seed.push(bump_seed);
            let size = (rng.next_u32() % ((10 * 1024) + 1)) as u16;
            let account = Account { address, seed, size };
            add_create_account_command(&funds_source.pubkey(), &account, &mut command_accounts, &mut data);
            accounts.lock().unwrap().add_new_account(account);
        }

        let recent_blockhash = recent_blockhash_fetcher.lock().unwrap().get();

        let program_id = &program_ids[0];

        let instructions = vec![Instruction::new_with_bytes(
            program_id.clone(),
            data.as_slice(),
            command_accounts.get_account_metas()
        )];

        let transaction = Transaction::new(
            &vec![&funds_source],
            Message::new(&instructions, Some(&funds_source.pubkey())),
            recent_blockhash
        );

        println!(
            "Submitting transaction {}\n  Signature: {}",
            base64::encode(bincode::serialize(&transaction).expect("encode")),
            transaction.signatures[0]
        );

        match rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(_) => accounts_to_create -= create_count,
            Err(e) => println!("TX failed: {}", e)
        }
    }

    let iterations = args.total_transactions.map(|t| Arc::new(Mutex::new(t)));

    let print_lock = Arc::new(Mutex::new(()));

    let mut threads = vec![];

    for thread_number in 0..args.num_threads {
        let print_lock = print_lock.clone();
        let rpc_clients = rpc_clients.clone();
        let recent_blockhash_fetcher = recent_blockhash_fetcher.clone();
        let program_ids = program_ids.clone();
        let funds_source = Keypair::from_bytes(&funds_source.to_bytes()).expect("");
        let accounts = accounts.clone();
        let iterations = iterations.clone();

        threads.push(std::thread::spawn(move || {
            transaction_thread_function(
                thread_number,
                print_lock,
                rpc_clients,
                recent_blockhash_fetcher,
                program_ids,
                funds_source,
                accounts,
                iterations
            )
        }));
    }

    // Join the threads
    for j in threads {
        j.join().expect("Failed to join");
    }

    // Delete accounts that were created

    let mut accounts = accounts.lock().unwrap();

    loop {
        if accounts.count() == 0 {
            break;
        }

        println!("({} accounts remain to be deleted)", accounts.map.len());

        // Delete 8 at a time
        let mut v = vec![];

        for i in 0..8 {
            if let Some(account) = accounts.take_random_account(&mut rng) {
                v.push(account);
            }
            else {
                break;
            }
        }

        let program_id = &program_ids[0];

        let mut data = vec![];

        let mut command_accounts = CommandAccounts::new();

        for account in &v {
            add_delete_account_command(&funds_source.pubkey(), account, &mut command_accounts, &mut data);
        }

        let instructions = vec![Instruction::new_with_bytes(
            program_id.clone(),
            data.as_slice(),
            command_accounts.get_account_metas()
        )];

        //let rpc_client = rpc_clients.get_finalized();
        let rpc_client = rpc_clients.lock().unwrap().get();

        let recent_blockhash = recent_blockhash_fetcher.lock().unwrap().get();

        let transaction = Transaction::new(
            &vec![&funds_source],
            Message::new(&instructions, Some(&funds_source.pubkey())),
            recent_blockhash
        );

        println!(
            "Submitting transaction {}\n  Signature: {}",
            base64::encode(bincode::serialize(&transaction).expect("encode")),
            transaction.signatures[0]
        );

        match rpc_client.send_and_confirm_transaction(&transaction) {
            Ok(_) => (),
            Err(e) => {
                println!("TX failed: {}", e);
                // Failed transaction, so all accounts it tried to delete are back
                v.into_iter().for_each(|account| accounts.add_existing_account(account));
            }
        }
    }
}

fn transfer_lamports(
    rpc_client : &RpcClient,
    fee_payer : &Keypair,
    funds_source : &Keypair,
    target : &Pubkey,
    amount : u64,
    recent_blockhash : &Hash
)
{
    if *target == funds_source.pubkey() {
        return;
    }

    let transaction = Transaction::new(
        &vec![fee_payer, funds_source],
        Message::new(
            &vec![system_instruction::transfer(&funds_source.pubkey(), target, amount)],
            Some(&fee_payer.pubkey())
        ),
        recent_blockhash.clone()
    );

    // Ignore the result.  If take_funds fails, the check for balance will try again.
    let _ = rpc_client.send_and_confirm_transaction(&transaction);
}
