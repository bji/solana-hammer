This program is used to create simulated load on a Solana cluster.

It functions by:

- Utilizing one or more smart contracts all compiled from the same source which can be invoked to perform arbitrary
  simulated work
  -> The reason for more than one is to allow proper simulation of cross-program invoke instructions; and also to
     simulate more realistically the load on Solana validators that comes from running many programs concurrently

- Creating a pre-set number of "contention accounts" that can be used by hammer clients to produce simulated
  account contention for writable and readable accounts

- Running a client which spawns a number of threads, each of which creates transactions which force the on-chain
  hammer program(s) to simulate work, and sends those transactions to the cluster leader(s) as quickly as possible

The following techniques are used to try to ensure maximum parallelism of transactions executed:

- The hammer program is stored at more than one address so that multiple instances of the program can be executed
  concurrently

- The hammer client creates a separate fee payer per transaction-submitting thread so that transactions are not
  serialized due to writing to the same fee payer account

- The hammer client runs multiple threads concurrently, each one submitting transactions as fast as it can

The following tecniques are used to try to simulate resource contention on the validators:

- A randomized selection of accounts are included in each transaction as "writable" and a different set is included
  as "readable"



The steps for running the hammer against a cluster are:

1. Ensure that a "funding source" account exists on the cluster and holds a significant amount of SOL.  Thousands
   of SOL would be sufficient for most purposes; less than 100 SOL might work but may quickly be used up as the
   hammer runs.  The hammer operator must "own" this account because its key will be used for paying transaction
   fees.

2. Upload the hammer program to one or more accounts
   -> testnet currently has 10 instances of the hammer program uploaded at these addresses:
      HaMmeR5fCtC3HueYUJLnYub1CohpZiRDKEpRdPuvV5rU
      HaMMER3bVoAVwW6fWH9MuBshnAdWXD8HQknVP5fN3mXv
      HaMMEr59BZi5mNWiGjsaDp9oWwHTs2fJRvyQMzq11bZP
      HamMErCngYMP1e72woVRUsyDgSnUiaD4AT2PWKeEvK2z
      HamMERdzsXHxmK1mwyvrJv6v6oHTD3iEECvthZZqBGNJ
      HamMeREfcqTDgxki1Uo9hg3R1MxQUQ7RHFNWSDAWKmri
      HamMerkAdEp5oxMGpovbDYwMXj1qGJzRTxSq3Wbi7vXN
      HaMmerfoUTRW3TbatHsjMVj9a5kHa3sX3hQzjQApAzne
      HAMmERqnuAhXEptbYbDRtSLKzVWTwmDjBRL1ne1FywLf
      HAMmeRU5Df6V4uiEWWqcF6jEi4AByX744W6tiS1sDhik
   More can be uploaded if the operator of the hammer client desires to do so.  This requires:
   a. Creating 10 new keypairs that will be used to upload the program
   b. Building the program (using 'make'), which requires that these keypairs already exist in the 'keys'
      directory under the names "program_1.json", "program_2.json", etc.
      (So for example:
       $ for i in `seq 1 10`; do solana-keygen new -s --no-bip39-passphrase -o keys/program_$i.json
       $ make
      )
   c. Uploading these programs one by one
      (So for example:
       $ for i in `seq 1 10`; do solana -u t -k keys/admin.json program deploy --program-id keys/program_$i.json target/program_$i.so; done
      )

3. Ensure that there are sufficient "contention accounts" created per instance of the hammer program.  These
   accounts are what are used to create simulated account contention between transactions.  A program has been
   supplied to perform this duty; to build it:
   $ cargo build --manifest-path client/Cargo.toml --release
   To run it (for example, once per program):
   $ for i in `seq 1 10`; do ./client/target/release/make_contention_accounts --fee-payer ./keys/fee_payer.json --rpc-node https://api.testnet.solana.com --program-id ./keys/program_$i.json; done


Steps 1-3 need only be performed once to set up the hammer, and are not necessary at all unless the operator really
wants to run their own instances of the hammer program/contention accounts.


4. Ensure that a "recent" gathering of the TPU addresses of all validators in the cluster has been done.  This
   needs to be done once per few hours/days, to ensure that new validators added to the network are represented
   in the set.  This set is used by the hammer client to know what address to send transactions to for a given
   leader.  This is a separate command because it requires running a gossip "spy" client and a spy client is very
   network-intensive, so it's better to collect this infrequently-changing data periodically rather than run a
   spy all the time.  To make the collection command:

   $ cargo build --manifest-path client/Cargo.toml --release

   To run collection:

   $ ./client/target/release/fetch_tpu --gossip-entrypoint 139.178.68.207:8001 --output-file tpu.bin
   
   (this command takes minutes - 5 or so - to complete)

   Note that the gossip-entrypoint should be a valid entrypoint for your cluster; for example find one for
   testnet using:

   $ dig entrypoint.testnet.solana.com


Step 4 must be done periodically; once per several hours or days.  It will update the tpu.bin file in place, which
will be re-read once per 5 minutes to acquire whatever new contents have been written most recently by the
fetch_tpu command.


5. Run the hammer client to send hammer transactions to the cluster; for example:

   $ ./client/target/release/client                                \
         --tpu-file tpu.bin                                        \
         --rpc-server $RPC                                         \
         --funds-source ./keys/fee_payer.json                      \
         --program-id HAMmeRU5Df6V4uiEWWqcF6jEi4AByX744W6tiS1sDhik \
         --program-id HaMmeR5fCtC3HueYUJLnYub1CohpZiRDKEpRdPuvV5rU \
         --program-id HaMMER3bVoAVwW6fWH9MuBshnAdWXD8HQknVP5fN3mXv \
         --program-id HaMMEr59BZi5mNWiGjsaDp9oWwHTs2fJRvyQMzq11bZP \
         --program-id HamMErCngYMP1e72woVRUsyDgSnUiaD4AT2PWKeEvK2z \
         --program-id HamMERdzsXHxmK1mwyvrJv6v6oHTD3iEECvthZZqBGNJ \
         --program-id HamMeREfcqTDgxki1Uo9hg3R1MxQUQ7RHFNWSDAWKmri \
         --program-id HamMerkAdEp5oxMGpovbDYwMXj1qGJzRTxSq3Wbi7vXN \
         --program-id HaMmerfoUTRW3TbatHsjMVj9a5kHa3sX3hQzjQApAzne \
         --program-id HAMmERqnuAhXEptbYbDRtSLKzVWTwmDjBRL1ne1FywLf \
         --num-threads 20                                          \
         --total-transactions 10000000


The above will run 20 threads concurrently, and write a total of 10,000,000 transactions to the cluster.  Note
that this can use significant outbound UDP bandwidth, tens of Mbit per second at least.
