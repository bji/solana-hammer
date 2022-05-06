
#include "solana_sdk.h"

#define DO_LOGGING 0

// Functions missing from solana_sdk.h
extern uint64_t sol_get_clock_sysvar(void *ret);
extern uint64_t sol_get_epoch_schedule_sysvar(void *ret);
extern uint64_t sol_get_rent_sysvar(void *ret);

typedef struct
{
    // Special admin user key -- can delete accounts of any owner
    SolPubkey admin_pubkey;
    
    // This is the program id.  It is the account address that actually stores this program.
    SolPubkey self_program_id;

    // This is the system program id
    SolPubkey system_program_id;

    // The following just pads the program size out, so that larger programs can be tested too.  Supplied
    // by a compiler symbol definition: PROGRAM_PADDING_SIZE
    uint8_t padding[PROGRAM_PADDING_SIZE];

} _Constants;

const _Constants Constants =
{
    // admin
    // tadZvEsVYdx96ua5mYedHgmzFmyqfNRsVo3rieBP8dU
    { 0x0d, 0x36, 0xa5, 0xb9, 0x86, 0x86, 0x4d, 0x5e, 0xbc, 0xca, 0x83, 0xdd, 0xf6, 0x02, 0xda, 0xcf,
      0x95, 0xda, 0x11, 0xe3, 0x76, 0xe4, 0xea, 0xf4, 0x89, 0x84, 0x56, 0x34, 0x2b, 0x28, 0x37, 0x2f },
    
    // self_program_id
    // Specified by compiler symbol definition: SELF_PROGRAM_ID_BYTES
    { SELF_PROGRAM_ID_BYTES },
    
    // system_program_id
    // 11111111111111111111111111111111
    { 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
      0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 }
    
};

// solana_sdk misses "const" in many places, so de-const to avoid compiler warnings.  The Constants instance
// is in a read-only data section and instructions which modify it actually have no effect.
#define Constants (* ((_Constants *) &Constants))


// Program that does nothing other than try to stress out the Solana network
//
// Behaves according to instructions in the input transaction:
//
// 0. Create accounts
// 1. Delete accounts
// 2. Do simulated CPU work
// 3. Alloc memory
// 4. Free memory
// 5. Cross-program invoke
// 6. Access sysvars
// 7. Fail early

typedef enum
{
    Command_CreateAccount   = 0,
    Command_DeleteAccount   = 1,
    Command_CPU             = 2,
    Command_Alloc           = 3,
    Command_Free            = 4,
    Command_CPI             = 5,
    Command_Sysvar          = 6,
    Command_Fail            = 7
    
} Command;


typedef enum
{
    Error_InvalidTransaction           = 10000,
    Error_InvalidCommand               = 10001,
    Error_InvalidAccounts              = 10002,
    Error_InvalidData                  = 10003,
    Error_InvalidAccount               = 10004,
    Error_NotAccountOwner              = 10005,
    Error_PermissionDenied             = 10006,
    Error_InsufficientFunds            = 10007
    
} Error;


// Forward declarations

// Gets the rent exempt minimum for an account of the given size.  This is not 100% precise; it will round up in some
// cases, and if the account_size is really huge (much larger than practical, 32 petabytes or more), will report a
// value too small.
static uint64_t get_rent_exempt_minimum(uint64_t account_size);
// Creates an account at a Program Derived Address, where the address is derived from the program id and a set
// of seed bytes.
static uint64_t create_pda(SolPubkey *new_account_key, SolSignerSeed *seeds, int seeds_count,
                           SolPubkey *funding_account_key, SolPubkey *owner_account_key,
                           uint64_t funding_lamports, uint64_t space,
                           // All cross-program invocation must pass all account infos through, it's
                           // the only sane way to cross-program invoke
                           SolAccountInfo *transaction_accounts, int transaction_accounts_len);
// The following all return bytes used < 10000 for success, >= 10000 for failure
static uint64_t do_CreateAccount(SolParameters *params, const uint8_t *data, uint64_t data_len);
static uint64_t do_DeleteAccount(SolParameters *params, const uint8_t *data, uint64_t data_len);
static uint64_t do_CPU(SolParameters *params, const uint8_t *data, uint64_t data_len);
static uint64_t do_Alloc(SolParameters *params, const uint8_t *data, uint64_t data_len, void **alloc_array);
static uint64_t do_Free(SolParameters *params, const uint8_t *data, uint64_t data_len, void **alloc_array);
static uint64_t do_CPI(SolParameters *params, const uint8_t *data, uint64_t data_len);
static uint64_t do_Sysvar(SolParameters *params, const uint8_t *data, uint64_t data_len);
static uint64_t do_Fail(SolParameters *params, const uint8_t *data, uint64_t data_len);


uint64_t entrypoint(const uint8_t *input)
{
    SolParameters params;

    // At most 20 accounts are supported for any command.
    SolAccountInfo account_info[20];
    params.ka = account_info;

    // Deserialize parameters.  Must succeed.
    if (!sol_deserialize(input, &params, sizeof(account_info) / sizeof(account_info[0]))) {
        return Error_InvalidTransaction;
    }

    // Fixed array of alloc'd addresses
    void *alloc_array[256];

    // handle instructions one by one
    const uint8_t *data = params.data;
    uint64_t data_len = params.data_len;

    while (data_len) {
        uint8_t command = *data;
        data++, data_len--;
        
        uint64_t retval;

        switch (command) {
        case Command_CreateAccount:
            retval = do_CreateAccount(&params, data, data_len);
            break;
            
        case Command_DeleteAccount:
            retval = do_DeleteAccount(&params, data, data_len);
            break;
            
        case Command_CPU:
            retval = do_CPU(&params, data, data_len);
            break;
            
        case Command_Alloc:
            retval = do_Alloc(&params, data, data_len, alloc_array);
            break;
            
        case Command_Free:
            retval = do_Free(&params, data, data_len, alloc_array);
            break;
            
        case Command_CPI:
            retval = do_CPI(&params, data, data_len);
            break;

        case Command_Sysvar:
            retval = do_Sysvar(&params, data, data_len);
            break;
            
        case Command_Fail:
            retval = do_Fail(&params, data, data_len);
            break;

        default:
            retval = Error_InvalidCommand;
            break;
        }

        if (retval < Error_InvalidTransaction) {
            data += retval, data_len -= retval;
        }
        else {
            return retval;
        }
    }

    return 0;
}


// Data structure stored in the Rent sysvar
typedef struct
{
    uint64_t lamports_per_byte_year;

    uint8_t exemption_threshold[8];

    uint8_t burn_percent;

} Rent;


static uint64_t get_rent_exempt_minimum(uint64_t account_size)
{
    // Get the rent sysvar value
    Rent rent;
    uint64_t result = sol_get_rent_sysvar(&rent);

    // Unfortunately the exemption threshold is in f64 format.  This makes it super difficult to work with since BPF
    // doesn't have floating point instructions.  So do manual computation using the bits of the floating point value.
    uint64_t u = * (uint64_t *) rent.exemption_threshold;
    uint64_t exp = ((u >> 52) & 0x7FF);

    if (result || // sol_get_rent_sysvar failed, so u and exp are bogus
        (u & 0x8000000000000000ULL) || // negative exemption_threshold
        ((exp == 0) || (exp == 0x7FF))) { // subnormal values
        // Unsupported and basically nonsensical rent exemption threshold.  Just use some hopefully sane default based
        // on historical values that were true for 2021/2022: lamports_per_byte_year = 3480, exemption_threshold = 2
        // years
        return (account_size + 128) * 3480 * 2;
    }

    // 128 bytes are added for account overhead
    uint64_t min = (account_size + 128) * rent.lamports_per_byte_year;
        
    if (exp >= 1023) {
        min *= (1 << (exp - 1023));
    }
    else {
        min /= (1 << (1023 - exp));
    }
    
    uint64_t fraction = u & 0x000FFFFFFFFFFFFFULL;
    
    // Reduce fraction to 10 bits, to avoid overflow.  Keep track of whether or not to round up.
    bool round_up = (fraction & 0x3FFFFFFFFFFULL);
    
    fraction >>= 42;
    if (round_up) {
        fraction += 1;
    }
    uint64_t original_fraction = fraction;
    fraction *= min;
    fraction /= 0x3FF;
    
    return min + fraction;
}


// To be used as data to pass to the system program when invoking CreateAccount
typedef struct __attribute__((__packed__))
{
    uint32_t instruction_code;
    uint64_t lamports;
    uint64_t space;
    SolPubkey owner;
} util_CreateAccountData;


// Creates an account at a Program Derived Address, where the address is derived from the program id and a set
// of seed bytes.
static uint64_t create_pda(SolPubkey *new_account_key, SolSignerSeed *seeds, int seeds_count,
                           SolPubkey *funding_account_key, SolPubkey *owner_account_key,
                           uint64_t funding_lamports, uint64_t space,
                           // All cross-program invocation must pass all account infos through, it's
                           // the only sane way to cross-program invoke
                           SolAccountInfo *transaction_accounts, int transaction_accounts_len)
{
    SolInstruction instruction;

    SolAccountMeta account_metas[] = 
        // First account to pass to CreateAccount is the funding_account
        { { /* pubkey */ funding_account_key, /* is_writable */ true, /* is_signer */ true },
          // Second account to pass to CreateAccount is the new account to be created
          { /* pubkey */ new_account_key, /* is_writable */ true, /* is_signer */ true } };

    util_CreateAccountData data = { 0, funding_lamports, space, *owner_account_key };

    instruction.program_id = (SolPubkey *) &(Constants.system_program_id);
    instruction.accounts = account_metas;
    instruction.account_len = sizeof(account_metas) / sizeof(account_metas[0]);
    instruction.data = (uint8_t *) &data;
    instruction.data_len = sizeof(data);

    SolSignerSeeds signer_seeds = { seeds, seeds_count };
    
    return sol_invoke_signed(&instruction, transaction_accounts, transaction_accounts_len, &signer_seeds, 1);
}


static uint64_t do_CreateAccount(SolParameters *params, const uint8_t *data, uint64_t data_len)
{
    // Accounts:
    // 1. [WRITE, SIGNER] -- funding account
    // 2. [WRITE] -- account to create
    // 3. [] -- system program id
    // Data is:
    // [32 bytes] -- funding_account
    // [32 bytes] -- account to create
    // [8 bytes] -- PDA seed
    // [2 bytes] -- Account size in bytes
    if (data_len < 74) {
        return Error_InvalidData;
    }
    SolPubkey funding_account_pubkey;
    SolPubkey create_account_address;
    sol_memcpy(&funding_account_pubkey, data, 32);
    data += 32;
    sol_memcpy(&create_account_address, data, 32);
    data += 32;
    SolSignerSeed seed = { data, 8 };
    // The account is created with an additional 32 bytes to store the owner pubkey
    uint16_t account_size = (* (uint16_t *) &(data[8]));
#if DO_LOGGING
    sol_log("create");
    sol_log_pubkey(&funding_account_pubkey);
    sol_log_pubkey(&create_account_address);
    sol_log_64(0, 0, 0, 0, account_size);
#endif
    account_size = 10;
    uint64_t result = create_pda(&create_account_address, &seed, 1, &funding_account_pubkey,
                                 &(Constants.self_program_id), get_rent_exempt_minimum(account_size),
                                 account_size, params->ka, params->ka_num);
    if (result) {
        return result;
    }

    // 74 bytes of data were used
    return 74;
}


static uint64_t do_DeleteAccount(SolParameters *params, const uint8_t *data, uint64_t data_len)
{
    // Accounts:
    // 1. [WRITE, SIGNER] -- funding account
    // 2. [WRITE] -- account to delete
    // Data is:
    // [32 bytes] -- funding account (received SOL)
    // [32 bytes] -- account to delete
    if (data_len < 64) {
        return Error_InvalidData;
    }

    SolPubkey funding_account_pubkey;
    SolPubkey delete_account_address;
    sol_memcpy(&funding_account_pubkey, data, 32);
    data += 32;
    sol_memcpy(&delete_account_address, data, 32);
    data += 32;
    
#if DO_LOGGING
    sol_log("delete");
    sol_log_pubkey(&delete_account_address);
#endif

    // Find the funding account in params
    SolAccountInfo *funding_account = 0;
    for (uint64_t i = 0; i < params->ka_num; i++) {
        SolAccountInfo *account_info = &(params->ka[i]);
        if (account_info->is_writable && SolPubkey_same(account_info->key, &funding_account_pubkey)) {
            funding_account = account_info;
            break;
        }
    }

    if (!funding_account) {
        return Error_InvalidAccounts;
    }

    // Find the delete account in params
    SolAccountInfo *delete_account = 0;
    for (uint64_t i = 0; i < params->ka_num; i++) {
        SolAccountInfo *account_info = &(params->ka[i]);
        if (account_info->is_writable && SolPubkey_same(account_info->key, &delete_account_address)) {
            delete_account = account_info;
            break;
        }
    }
    
    if (!delete_account) {
        return Error_InvalidAccounts;
    }

    delete_account->data_len = 0;
    *(funding_account->lamports) += *(delete_account->lamports);
    *(delete_account->lamports) = 0;

    // 64 bytes of data were used
    return 64;
}
 

static uint64_t do_CPU(SolParameters *params, const uint8_t *data, uint64_t data_len)
{
    // Data is:
    // [4 bytes] -- number of loops
    if (data_len < 4) {
        return Error_InvalidData;
    }
    uint32_t loops = * (uint32_t *) data;
    
#if DO_LOGGING
    sol_log("cpu");
    sol_log_64(0, 0, 0, 0, loops);
#endif
    
    uint32_t loop_seed;
    SolSignerSeed seeds[2] = {
        { (uint8_t *) &(Constants.self_program_id), sizeof(Constants.self_program_id) },
        { (uint8_t *) &loop_seed, sizeof(loop_seed) }
    };
    for (uint32_t i = 0; i < loops; i++) {
        loop_seed = i;
        SolPubkey junk1;
        uint8_t junk2;
        sol_try_find_program_address(seeds, 2, &(Constants.self_program_id), &junk1, &junk2);
    }

    // 4 bytes of data used
    return 4;
}

    
static uint64_t do_Alloc(SolParameters *params, const uint8_t *data, uint64_t data_len, void **alloc_array)
{
    // Data is:
    // [4 bytes] -- size of alloc
    // [1 byte] -- alloc index
    if (data_len < 5) {
        return Error_InvalidData;
    }
    uint32_t amount = * (uint32_t *) data;

    uint8_t index = data[32];

#if DO_LOGGING
    sol_log("alloc");
    sol_log_64(0, 0, 0, index, amount);
#endif
    
    alloc_array[index] = sol_calloc(amount, 1);

    // 5 bytes of data used
    return 5;
}


static uint64_t do_Free(SolParameters *params, const uint8_t *data, uint64_t data_len, void **alloc_array)
{
    // Data is:
    // [1 byte] -- alloc index
    if (data_len < 1) {
        return Error_InvalidData;
    }
    uint8_t index = data[0];
    
#if DO_LOGGING
    sol_log("free");
    sol_log_64(0, 0, 0, 0, index);
#endif
    
    sol_free(alloc_array[index]);

    // 1 byte of data used
    return 1;
}


static uint64_t do_CPI(SolParameters *params, const uint8_t *data, uint64_t data_len)
{
    const uint8_t *original_data = data;
    
    // Accounts are whatever the CPI call wants them to be
    // Data is:
    // [32 bytes] -- program id to invoke
    // [1 byte] -- number of account metas
    // [n instances of:
    //   [32 bytes] -- account meta pubkey
    //   [1 byte] -- is writable
    //   [1 byte] -- is signer
    // ]
    // [2 bytes] -- instruction data count
    // [n bytes] -- instruction data
    // [1 byte] -- seed length (0 for no seed, so no invoke_signed)
    // [n bytes] -- seed
    if (data_len < 32) {
        return Error_InvalidData;
    }
    SolPubkey *program_id = (SolPubkey *) data;
    data += 32, data_len -= 32;
    if (data_len < 1) {
        return Error_InvalidData;
    }
    uint8_t account_metas_count = *data;
    data +=1, data_len -= 1;
    SolAccountMeta *account_metas =
        (SolAccountMeta *) sol_calloc(256 * sizeof(SolAccountMeta), 1); // maximum number possible
    for (uint8_t i = 0; i < account_metas_count; i++) {
        if (data_len < 34) {
            return Error_InvalidData;
        }
        account_metas[i].pubkey = (SolPubkey *) data;
        data += 32, data_len -= 32;
        account_metas[i].is_writable = data[0] ? true : false;
        account_metas[i].is_signer = data[1] ? true : false;
        data += 2, data_len -= 2;
    }
    if (data_len < 2) {
        return Error_InvalidData;
    }
    uint16_t instruction_data_count = * (uint16_t *) data;
    data += 2, data_len -=2;
    if (data_len < instruction_data_count) {
        return Error_InvalidData;
    }
    uint8_t *instruction_data = (uint8_t *) data;
    data += instruction_data_count, data_len -= instruction_data_count;
    uint8_t seed_length = *data;
    data += 1, data_len -= 1;
    if (data_len < seed_length) {
        return Error_InvalidData;
    }
    
    SolInstruction instruction =
        { program_id, account_metas, account_metas_count, instruction_data, instruction_data_count };

    if (seed_length) {
        // Seed, so invoke signed
        SolSignerSeed seed = { data, seed_length };
        
        SolSignerSeeds seeds = { &seed, 1 };
        
        uint64_t retval = sol_invoke_signed(&instruction, params->ka, params->ka_num, &seeds, 1);
        if (retval) {
            // Must add 10000 to ensure that it's not within the "success range" of returned byte count
            return retval + 10000;
        }
        
        data += seed_length;
    }
    else {
        // No seed, so just invoke
        uint64_t retval = sol_invoke(&instruction, params->ka, params->ka_num);
        if (retval) {
            // Must add 10000 to ensure that it's not within the "success range" of returned byte count
            return retval + 10000;
        }
    }

    // Return number of bytes of data that were used
    return ((uint64_t) data) - ((uint64_t) original_data);
}


static uint64_t do_Sysvar(SolParameters *params, const uint8_t *data, uint64_t data_len)
{
    // No accounts
    // No data

#if DO_LOGGING
    sol_log("sysvar");
#endif

    // Just use a big enough data blob
    uint8_t buffer[1024];

    uint64_t retval = sol_get_clock_sysvar(&buffer);
    if (retval) {
        // Must add 10000 to ensure that it's not within the "success range" of returned byte count
        return retval + 10000;
    }

    retval = sol_get_epoch_schedule_sysvar(&buffer);
    if (retval) {
        // Must add 10000 to ensure that it's not within the "success range" of returned byte count
        return retval + 10000;
    }

    retval = sol_get_rent_sysvar(&buffer);
    if (retval) {
        // Must add 10000 to ensure that it's not within the "success range" of returned byte count
        return retval + 10000;
    }

    // 0 bytes of data were used
    return 0;
}


static uint64_t do_Fail(SolParameters *params, const uint8_t *data, uint64_t data_len)
{
    // Data is:
    // [8 bytes] -- return code

    if (data_len < 8) {
        return Error_InvalidData;
    }

#if DO_LOGGING
    sol_log("fail");
    sol_log_64(0, 0, 0, 0, (* (uint64_t *) data) + 10000);
#endif

    // Must add 10000 to ensure that it's not within the "success range" of returned byte count
    return (* (uint64_t *) data) + 10000;
}
