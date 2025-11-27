use core::mem::size_of;
use pinocchio::{
    ProgramResult, account_info::AccountInfo, instruction::Seed, program_error::ProgramError,
    pubkey::find_program_address,
};
use pinocchio_token::instructions::Transfer;

use crate::{
    AccountCheck, AssociatedTokenAccount, AssociatedTokenAccountCheck, AssociatedTokenAccountInit,
    Escrow, MintInterface, ProgramAccount, ProgramAccountInit, SignerAccount,
};

pub struct MakeAccounts<'a> {
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub maker_ata_a: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub associated_token_account_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for MakeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [
            maker,
            escrow,
            mint_a,
            mint_b,
            maker_ata_a,
            vault,
            system_program,
            token_program,
            associated_token_account_program,
        ] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Basic Accounts Checks
        SignerAccount::check(maker)?;
        MintInterface::check(mint_a)?;
        MintInterface::check(mint_b)?;
        AssociatedTokenAccount::check(maker_ata_a, maker, mint_a, token_program)?;

        // Return the accounts
        Ok(Self {
            maker,
            escrow,
            mint_a,
            mint_b,
            maker_ata_a,
            vault,
            system_program,
            token_program,
            associated_token_account_program,
        })
    }
}

pub struct MakeInstructionData {
    pub seed: u64,
    pub receive: u64,
    pub amount: u64,
}

impl<'a> TryFrom<&'a [u8]> for MakeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != size_of::<u64>() * 3 {
            return Err(ProgramError::InvalidInstructionData);
        }

        let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let receive = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let amount = u64::from_le_bytes(data[16..24].try_into().unwrap());

        // Instruction Checks
        if amount == 0 {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self {
            seed,
            receive,
            amount,
        })
    }
}

pub struct Make<'a> {
    pub accounts: MakeAccounts<'a>,
    pub instruction_data: MakeInstructionData,
    pub bump: u8,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Make<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = MakeAccounts::try_from(accounts)?;
        let instruction_data = MakeInstructionData::try_from(data)?;

        // Initialize the Accounts needed
        let (_, bump) = find_program_address(
            &[
                b"escrow",
                accounts.maker.key(),
                &instruction_data.seed.to_le_bytes(),
            ],
            &crate::ID,
        );

        let seed_binding = instruction_data.seed.to_le_bytes();
        let bump_binding = [bump];
        let escrow_seeds = [
            Seed::from(b"escrow"),
            Seed::from(accounts.maker.key().as_ref()),
            Seed::from(&seed_binding),
            Seed::from(&bump_binding),
        ];

        ProgramAccount::init::<Escrow>(
            accounts.maker,
            accounts.escrow,
            &escrow_seeds,
            Escrow::LEN,
        )?;

        // Initialize the vault
        AssociatedTokenAccount::init(
            accounts.vault,
            accounts.mint_a,
            accounts.maker,
            accounts.escrow,
            accounts.system_program,
            accounts.token_program,
        )?;

        Ok(Self {
            accounts,
            instruction_data,
            bump,
        })
    }
}

impl<'a> Make<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        // Populate the escrow account
        let mut data = self.accounts.escrow.try_borrow_mut_data()?;
        let escrow = Escrow::load_mut(data.as_mut())?;

        escrow.set_inner(
            self.instruction_data.seed,
            *self.accounts.maker.key(),
            *self.accounts.mint_a.key(),
            *self.accounts.mint_b.key(),
            self.instruction_data.receive,
            [self.bump],
        );

        // Transfer tokens to vault
        Transfer {
            from: self.accounts.maker_ata_a,
            to: self.accounts.vault,
            authority: self.accounts.maker,
            amount: self.instruction_data.amount,
        }
        .invoke()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use solana_instruction::{AccountMeta, Instruction};
    use solana_signer::Signer;
    use spl_associated_token_account::{
        get_associated_token_address_with_program_id,
        solana_program::native_token::LAMPORTS_PER_SOL,
    };

    use crate::{
        Escrow,
        tests::{
            constants::{
                ASSOCIATED_TOKEN_PROGRAM_ID, MINT_DECIMALS, PROGRAM_ID, SYSTEM_PROGRAM_ID,
                TOKEN_PROGRAM_ID,
            },
            pda::get_escrow_pda,
            utils::{build_and_send_transaction, init_ata, init_mint, init_wallet, setup},
        },
    };

    #[test]
    fn make() {
        let (litesvm, _default_payer) = &mut setup();

        let maker = init_wallet(litesvm, LAMPORTS_PER_SOL);
        let mint_a = init_mint(litesvm, TOKEN_PROGRAM_ID, MINT_DECIMALS, 1_000_000_000);
        let mint_b = init_mint(litesvm, TOKEN_PROGRAM_ID, MINT_DECIMALS, 1_000_000_000);
        let maker_ata_a = init_ata(litesvm, mint_a, maker.pubkey(), 1_000_000_000);

        let seed = 42u64;
        let receive_amount: u64 = 100_000_000;
        let give_amount: u64 = 500_000_000;
        let escrow_pda = get_escrow_pda(&maker.pubkey(), seed);
        let vault =
            get_associated_token_address_with_program_id(&escrow_pda, &mint_a, &TOKEN_PROGRAM_ID);

        let data = [
            vec![0u8],
            seed.to_le_bytes().to_vec(),
            receive_amount.to_le_bytes().to_vec(),
            give_amount.to_le_bytes().to_vec(),
        ]
        .concat();
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(maker.pubkey(), true),
                AccountMeta::new(escrow_pda, false),
                AccountMeta::new_readonly(mint_a, false),
                AccountMeta::new_readonly(mint_b, false),
                AccountMeta::new(maker_ata_a, false),
                AccountMeta::new(vault, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM_ID, false),
            ],
            data,
        };

        let _ = build_and_send_transaction(litesvm, &[&maker], &maker.pubkey(), &[ix]);

        let escrow_acc = litesvm.get_account(&escrow_pda).unwrap();
        let escrow = Escrow::load(escrow_acc.data.as_ref()).unwrap();

        assert_eq!(escrow.seed, seed);
        assert_eq!(escrow.maker, maker.pubkey().to_bytes());
        assert_eq!(escrow.mint_a, mint_a.to_bytes());
        assert_eq!(escrow.mint_b, mint_b.to_bytes());
        assert_eq!(escrow.receive, receive_amount);
    }
}
