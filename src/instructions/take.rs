use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::create_program_address,
};
use pinocchio_token::{
    instructions::{CloseAccount, Transfer},
    state::TokenAccount,
};

use crate::{
    AccountCheck, AccountClose, AssociatedTokenAccount, AssociatedTokenAccountCheck,
    AssociatedTokenAccountInit, Escrow, MintInterface, ProgramAccount, SignerAccount,
};

pub struct TakeAccounts<'a> {
    pub taker: &'a AccountInfo,
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub taker_ata_a: &'a AccountInfo,
    pub taker_ata_b: &'a AccountInfo,
    pub maker_ata_b: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub associated_token_account_program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for TakeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [
            taker,
            maker,
            escrow,
            mint_a,
            mint_b,
            vault,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            system_program,
            token_program,
            associated_token_account_program,
        ] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Basic Accounts Checks
        SignerAccount::check(taker)?;
        ProgramAccount::check(escrow)?;
        MintInterface::check(mint_a)?;
        MintInterface::check(mint_b)?;
        AssociatedTokenAccount::check(taker_ata_b, taker, mint_b, token_program)?;
        AssociatedTokenAccount::check(vault, escrow, mint_a, token_program)?;

        // Return the accounts
        Ok(Self {
            taker,
            maker,
            escrow,
            mint_a,
            mint_b,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            vault,
            system_program,
            token_program,
            associated_token_account_program,
        })
    }
}

pub struct Take<'a> {
    pub accounts: TakeAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Take<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let accounts = TakeAccounts::try_from(accounts)?;

        // Initialize necessary accounts
        AssociatedTokenAccount::init_if_needed(
            accounts.taker_ata_a,
            accounts.mint_a,
            accounts.taker,
            accounts.taker,
            accounts.system_program,
            accounts.token_program,
        )?;

        AssociatedTokenAccount::init_if_needed(
            accounts.maker_ata_b,
            accounts.mint_b,
            accounts.taker,
            accounts.maker,
            accounts.system_program,
            accounts.token_program,
        )?;

        Ok(Self { accounts })
    }
}

impl<'a> Take<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let data = self.accounts.escrow.try_borrow_data()?;
        let escrow = Escrow::load(&data)?;

        // Check if the escrow is valid
        let escrow_key = create_program_address(
            &[
                b"escrow",
                self.accounts.maker.key(),
                &escrow.seed.to_le_bytes(),
                &escrow.bump,
            ],
            &crate::ID,
        )?;
        if &escrow_key != self.accounts.escrow.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let seed_binding = escrow.seed.to_le_bytes();
        let bump_binding = escrow.bump;
        let escrow_seeds = [
            Seed::from(b"escrow"),
            Seed::from(self.accounts.maker.key().as_ref()),
            Seed::from(&seed_binding),
            Seed::from(&bump_binding),
        ];
        let signer = Signer::from(&escrow_seeds);

        let amount = {
            let vault = TokenAccount::from_account_info(self.accounts.vault)?;

            vault.amount()
        };

        // Transfer from the Vault to the Taker
        Transfer {
            from: self.accounts.vault,
            to: self.accounts.taker_ata_a,
            authority: self.accounts.escrow,
            amount,
        }
        .invoke_signed(&[signer.clone()])?;

        // Close the Vault
        CloseAccount {
            account: self.accounts.vault,
            destination: self.accounts.maker,
            authority: self.accounts.escrow,
        }
        .invoke_signed(&[signer.clone()])?;

        // Transfer from the Taker to the Maker
        Transfer {
            from: self.accounts.taker_ata_b,
            to: self.accounts.maker_ata_b,
            authority: self.accounts.taker,
            amount: escrow.receive,
        }
        .invoke()?;

        // Close the Escrow
        drop(data);
        ProgramAccount::close(self.accounts.escrow, self.accounts.maker)?;

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
    use spl_token_2022::state::Account as TokenAccount;

    use crate::tests::{
        constants::{
            ASSOCIATED_TOKEN_PROGRAM_ID, MINT_DECIMALS, PROGRAM_ID, SYSTEM_PROGRAM_ID,
            TOKEN_PROGRAM_ID,
        },
        pda::get_escrow_pda,
        utils::{
            build_and_send_transaction, fetch_account, init_ata, init_mint, init_wallet, setup,
        },
    };

    #[test]
    fn take() {
        let (litesvm, _default_payer) = &mut setup();

        let maker = init_wallet(litesvm, LAMPORTS_PER_SOL);
        let taker = init_wallet(litesvm, LAMPORTS_PER_SOL);
        let mint_a = init_mint(litesvm, TOKEN_PROGRAM_ID, MINT_DECIMALS, 1_000_000_000);
        let mint_b = init_mint(litesvm, TOKEN_PROGRAM_ID, MINT_DECIMALS, 1_000_000_000);
        let maker_ata_a = init_ata(litesvm, mint_a, maker.pubkey(), 1_000_000_000);
        let taker_ata_b = init_ata(litesvm, mint_b, taker.pubkey(), 1_000_000_000);

        let seed = 42u64;
        let receive_amount: u64 = 100_000_000;
        let give_amount: u64 = 500_000_000;

        let escrow_pda = get_escrow_pda(&maker.pubkey(), seed);
        let vault =
            get_associated_token_address_with_program_id(&escrow_pda, &mint_a, &TOKEN_PROGRAM_ID);

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
            data: [
                vec![0u8],
                seed.to_le_bytes().to_vec(),
                receive_amount.to_le_bytes().to_vec(),
                give_amount.to_le_bytes().to_vec(),
            ]
            .concat(),
        };

        let _ = build_and_send_transaction(litesvm, &[&maker], &maker.pubkey(), &[ix]);

        let taker_ata_a = get_associated_token_address_with_program_id(
            &taker.pubkey(),
            &mint_a,
            &TOKEN_PROGRAM_ID,
        );
        let maker_ata_b = get_associated_token_address_with_program_id(
            &maker.pubkey(),
            &mint_b,
            &TOKEN_PROGRAM_ID,
        );

        let pre_maker_ata_b_bal = 0;
        let pre_taker_ata_a_bal = 0;

        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(taker.pubkey(), true),
                AccountMeta::new(maker.pubkey(), false),
                AccountMeta::new(escrow_pda, false),
                AccountMeta::new_readonly(mint_a, false),
                AccountMeta::new_readonly(mint_b, false),
                AccountMeta::new(vault, false),
                AccountMeta::new(taker_ata_a, false),
                AccountMeta::new(taker_ata_b, false),
                AccountMeta::new(maker_ata_b, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM_ID, false),
            ],
            data: [vec![1u8]].concat(),
        };

        let _ = build_and_send_transaction(litesvm, &[&taker], &taker.pubkey(), &[ix]);

        let escrow_acc = litesvm.get_account(&escrow_pda);

        assert!(escrow_acc.is_none());

        let vault_acc = litesvm.get_account(&vault);

        assert!(vault_acc.is_none());

        let post_maker_ata_b_bal = fetch_account::<TokenAccount>(litesvm, &maker_ata_b).amount;

        assert_eq!(pre_maker_ata_b_bal, post_maker_ata_b_bal - receive_amount);

        let post_taker_ata_a_bal = fetch_account::<TokenAccount>(litesvm, &taker_ata_a).amount;

        assert_eq!(pre_taker_ata_a_bal, post_taker_ata_a_bal - give_amount);
    }
}
