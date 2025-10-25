use litesvm::{
    LiteSVM,
    types::{FailedTransactionMetadata, TransactionResult},
};
use solana_account::Account;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::{
    get_associated_token_address,
    solana_program::{native_token::LAMPORTS_PER_SOL, program_pack::{IsInitialized, Pack}},
};
use spl_token_2022::state::{Account as TokenAccount, AccountState, Mint};

use crate::tests::constants::PROGRAM_ID;

pub fn setup() -> (LiteSVM, Keypair) {
    let mut litesvm = LiteSVM::new();

    litesvm
        .add_program_from_file(PROGRAM_ID.to_bytes(), "target/deploy/pinocchio_escrow.so")
        .unwrap();

    let default_payer = Keypair::new();
    litesvm
        .airdrop(&default_payer.pubkey(), LAMPORTS_PER_SOL * 100)
        .unwrap();

    (litesvm, default_payer)
}

fn pack_data<T: Pack>(state: T) -> Vec<u8> {
    let mut data = vec![0; T::LEN];
    T::pack(state, &mut data).unwrap();
    data
}

pub fn fetch_account<T: Pack + IsInitialized>(litesvm: &LiteSVM, pubkey: &Pubkey) -> T {
    let account = litesvm.get_account(pubkey).unwrap();
    T::unpack(&mut account.data.as_ref()).unwrap()
}

pub fn build_and_send_transaction(
    litesvm: &mut LiteSVM,
    signers: &[&Keypair],
    payer: &Pubkey,
    ixs: &[Instruction],
) -> TransactionResult {
    let tx = Transaction::new(
        signers,
        Message::new(ixs, Some(payer)),
        litesvm.latest_blockhash(),
    );
    litesvm.send_transaction(tx)
}

pub fn assert_error(tx_meta: FailedTransactionMetadata, error: &str) {
    assert!(tx_meta.meta.pretty_logs().contains(&error.to_string()));
}

pub fn init_wallet(litesvm: &mut LiteSVM, lamports: u64) -> Keypair {
    let wallet = Keypair::new();
    litesvm.airdrop(&wallet.pubkey(), lamports).unwrap();
    wallet
}

pub fn init_mint(litesvm: &mut LiteSVM, owner: Pubkey, decimals: u8, supply: u64) -> Pubkey {
    let mint = Keypair::new().pubkey();

    let mint_state = Mint {
        mint_authority: None.into(),
        supply,
        decimals,
        is_initialized: true,
        freeze_authority: None.into(),
    };

    let mint_data = pack_data(mint_state);
    let lamports = litesvm.minimum_balance_for_rent_exemption(Mint::LEN);

    litesvm
        .set_account(
            mint,
            Account {
                lamports,
                data: mint_data,
                owner,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();

    mint
}

pub fn init_ata(litesvm: &mut LiteSVM, mint: Pubkey, owner: Pubkey, amount: u64) -> Pubkey {
    let token_program = litesvm.get_account(&mint).unwrap().owner;
    let ata = get_associated_token_address(&owner, &mint);

    let ata_state = TokenAccount {
        mint,
        owner,
        amount,
        delegate: None.into(),
        state: AccountState::Initialized,
        is_native: None.into(),
        delegated_amount: 0,
        close_authority: None.into(),
    };

    let ata_data = pack_data(ata_state);
    let lamports = litesvm.minimum_balance_for_rent_exemption(TokenAccount::LEN);

    litesvm
        .set_account(
            ata,
            Account {
                lamports,
                data: ata_data,
                owner: token_program,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();

    ata
}
