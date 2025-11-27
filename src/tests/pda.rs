use solana_pubkey::Pubkey;

use crate::tests::constants::{ESCROW_SEED, PROGRAM_ID};

pub fn get_escrow_pda(maker: &Pubkey, seed: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[ESCROW_SEED, maker.as_ref(), seed.to_le_bytes().as_ref()],
        &PROGRAM_ID,
    )
    .0
}
