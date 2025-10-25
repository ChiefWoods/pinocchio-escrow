use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::find_program_address,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_associated_token_account::instructions::Create;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::{
    instructions::{InitializeAccount3, InitializeMint2},
    state::{Mint, TokenAccount as TokenAccountState},
};

use crate::{Escrow, PinocchioError};

pub trait AccountCheck {
    fn check(account: &AccountInfo) -> Result<(), ProgramError>;
}

pub struct SignerAccount;

impl AccountCheck for SignerAccount {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if !account.is_signer() {
            return Err(PinocchioError::NotSigner.into());
        }
        Ok(())
    }
}

pub struct SystemAccount;

impl AccountCheck for SystemAccount {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&pinocchio_system::ID) {
            return Err(ProgramError::InvalidAccountOwner.into());
        }

        Ok(())
    }
}

pub struct MintAccount;

impl AccountCheck for MintAccount {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&pinocchio_token::ID) {
            return Err(ProgramError::InvalidAccountOwner.into());
        }

        if account.data_len() != Mint::LEN {
            return Err(ProgramError::InvalidAccountData.into());
        }

        Ok(())
    }
}

pub trait MintInit {
    fn init(
        account: &AccountInfo,
        payer: &AccountInfo,
        decimals: u8,
        mint_authority: &[u8; 32],
        freeze_authority: Option<&[u8; 32]>,
    ) -> ProgramResult;
    fn init_if_needed(
        account: &AccountInfo,
        payer: &AccountInfo,
        decimals: u8,
        mint_authority: &[u8; 32],
        freeze_authority: Option<&[u8; 32]>,
    ) -> ProgramResult;
}

impl MintInit for MintAccount {
    fn init(
        account: &AccountInfo,
        payer: &AccountInfo,
        decimals: u8,
        mint_authority: &[u8; 32],
        freeze_authority: Option<&[u8; 32]>,
    ) -> ProgramResult {
        let lamports = Rent::get()?.minimum_balance(Mint::LEN);

        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: Mint::LEN as u64,
            owner: &pinocchio_token::ID,
        }
        .invoke()?;

        InitializeMint2 {
            mint: account,
            decimals,
            mint_authority,
            freeze_authority,
        }
        .invoke()
    }

    fn init_if_needed(
        account: &AccountInfo,
        payer: &AccountInfo,
        decimals: u8,
        mint_authority: &[u8; 32],
        freeze_authority: Option<&[u8; 32]>,
    ) -> ProgramResult {
        match Self::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, payer, decimals, mint_authority, freeze_authority),
        }
    }
}

pub struct TokenAccount;

impl AccountCheck for TokenAccount {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&pinocchio_token::ID) {
            return Err(ProgramError::InvalidAccountOwner.into());
        }

        if account
            .data_len()
            .ne(&TokenAccountState::LEN)
        {
            return Err(ProgramError::InvalidAccountData.into());
        }

        Ok(())
    }
}

pub trait TokenInit {
    fn init(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &[u8; 32],
    ) -> ProgramResult;
    fn init_if_needed(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &[u8; 32],
    ) -> ProgramResult;
}

impl TokenInit for TokenAccount {
    fn init(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &[u8; 32],
    ) -> ProgramResult {
        let lamports = Rent::get()?.minimum_balance(TokenAccountState::LEN);

        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: TokenAccountState::LEN as u64,
            owner: &pinocchio_token::ID,
        }
        .invoke()?;

        // Initialize the Token Account
        InitializeAccount3 {
            account,
            mint,
            owner,
        }
        .invoke()
    }

    fn init_if_needed(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &[u8; 32],
    ) -> ProgramResult {
        match Self::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, mint, payer, owner),
        }
    }
}

// TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb
pub const TOKEN_2022_PROGRAM_ID: [u8; 32] = [
    0x06, 0xdd, 0xf6, 0xe1, 0xee, 0x75, 0x8f, 0xde, 0x18, 0x42, 0x5d, 0xbc, 0xe4, 0x6c, 0xcd, 0xda,
    0xb6, 0x1a, 0xfc, 0x4d, 0x83, 0xb9, 0x0d, 0x27, 0xfe, 0xbd, 0xf9, 0x28, 0xd8, 0xa1, 0x8b, 0xfc,
];

const TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET: usize = 165;
pub const TOKEN_2022_MINT_DISCRIMINATOR: u8 = 0x01;
pub const TOKEN_2022_TOKEN_ACCOUNT_DISCRIMINATOR: u8 = 0x02;

pub struct Mint2022Account;

impl AccountCheck for Mint2022Account {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&TOKEN_2022_PROGRAM_ID) {
            return Err(ProgramError::InvalidAccountOwner.into());
        }

        let data = account.try_borrow_data()?;

        if data.len().ne(&Mint::LEN) {
            if data[TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET].ne(&TOKEN_2022_MINT_DISCRIMINATOR) {
                return Err(ProgramError::InvalidAccountData.into());
            }
        }

        Ok(())
    }
}

impl MintInit for Mint2022Account {
    fn init(
        account: &AccountInfo,
        payer: &AccountInfo,
        decimals: u8,
        mint_authority: &[u8; 32],
        freeze_authority: Option<&[u8; 32]>,
    ) -> ProgramResult {
        let lamports = Rent::get()?.minimum_balance(Mint::LEN);

        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: Mint::LEN as u64,
            owner: &TOKEN_2022_PROGRAM_ID,
        }
        .invoke()?;

        InitializeMint2 {
            mint: account,
            decimals,
            mint_authority,
            freeze_authority,
        }
        .invoke()
    }

    fn init_if_needed(
        account: &AccountInfo,
        payer: &AccountInfo,
        decimals: u8,
        mint_authority: &[u8; 32],
        freeze_authority: Option<&[u8; 32]>,
    ) -> ProgramResult {
        match Self::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, payer, decimals, mint_authority, freeze_authority),
        }
    }
}
pub struct TokenAccount2022Account;

impl AccountCheck for TokenAccount2022Account {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&TOKEN_2022_PROGRAM_ID) {
            return Err(ProgramError::InvalidAccountOwner.into());
        }

        let data = account.try_borrow_data()?;

        if data.len().ne(&TokenAccountState::LEN) {
            if data[TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET]
                .ne(&TOKEN_2022_TOKEN_ACCOUNT_DISCRIMINATOR)
            {
                return Err(ProgramError::InvalidAccountData.into());
            }
        }

        Ok(())
    }
}

impl TokenInit for TokenAccount2022Account {
    fn init(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &[u8; 32],
    ) -> ProgramResult {
        let lamports = Rent::get()?.minimum_balance(TokenAccountState::LEN);

        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: TokenAccountState::LEN as u64,
            owner: &TOKEN_2022_PROGRAM_ID,
        }
        .invoke()?;

        InitializeAccount3 {
            account,
            mint,
            owner,
        }
        .invoke()
    }

    fn init_if_needed(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &[u8; 32],
    ) -> ProgramResult {
        match Self::check(account) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, mint, payer, owner),
        }
    }
}

pub struct MintInterface;

impl AccountCheck for MintInterface {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&TOKEN_2022_PROGRAM_ID) {
            if account.owner().ne(&pinocchio_token::ID) {
                return Err(ProgramError::InvalidAccountOwner.into());
            } else {
                if account.data_len().ne(&Mint::LEN) {
                    return Err(ProgramError::InvalidAccountData.into());
                }
            }
        } else {
            let data = account.try_borrow_data()?;

            if data.len().ne(&Mint::LEN) {
                if data[TOKEN_2022_ACCOUNT_DISCRIMINATOR_OFFSET].ne(&TOKEN_2022_MINT_DISCRIMINATOR)
                {
                    return Err(ProgramError::InvalidAccountData.into());
                }
            }
        }

        Ok(())
    }
}

pub struct TokenAccountInterface;

impl AccountCheck for TokenAccountInterface {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&TOKEN_2022_PROGRAM_ID) {
            if account.owner().ne(&pinocchio_token::ID) {
                return Err(ProgramError::InvalidAccountOwner.into());
            } else {
                if account
                    .data_len()
                    .ne(&TokenAccountState::LEN)
                {
                    return Err(ProgramError::InvalidAccountData.into());
                }
            }
        } else {
            let data = account.try_borrow_data()?;

            if data.len().ne(&TokenAccountState::LEN) {
                return Err(ProgramError::InvalidAccountData.into());
            }
        }

        Ok(())
    }
}

pub trait AssociatedTokenAccountCheck {
    fn check(
        account: &AccountInfo,
        authority: &AccountInfo,
        mint: &AccountInfo,
        token_program: &AccountInfo,
    ) -> Result<(), ProgramError>;
}
pub struct AssociatedTokenAccount;

impl AssociatedTokenAccountCheck for AssociatedTokenAccount {
    fn check(
        account: &AccountInfo,
        authority: &AccountInfo,
        mint: &AccountInfo,
        token_program: &AccountInfo,
    ) -> Result<(), ProgramError> {
        TokenAccount::check(account)?;

        if find_program_address(
            &[authority.key(), token_program.key(), mint.key()],
            &pinocchio_associated_token_account::ID,
        )
        .0
        .ne(account.key())
        {
            return Err(PinocchioError::InvalidAddress.into());
        }

        Ok(())
    }
}

pub trait AssociatedTokenAccountInit {
    fn init(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &AccountInfo,
        system_program: &AccountInfo,
        token_program: &AccountInfo,
    ) -> ProgramResult;
    fn init_if_needed(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &AccountInfo,
        system_program: &AccountInfo,
        token_program: &AccountInfo,
    ) -> ProgramResult;
}

impl AssociatedTokenAccountInit for AssociatedTokenAccount {
    fn init(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &AccountInfo,
        system_program: &AccountInfo,
        token_program: &AccountInfo,
    ) -> ProgramResult {
        Create {
            funding_account: payer,
            account,
            wallet: owner,
            mint,
            system_program,
            token_program,
        }
        .invoke()
    }

    fn init_if_needed(
        account: &AccountInfo,
        mint: &AccountInfo,
        payer: &AccountInfo,
        owner: &AccountInfo,
        system_program: &AccountInfo,
        token_program: &AccountInfo,
    ) -> ProgramResult {
        match Self::check(account, payer, mint, token_program) {
            Ok(_) => Ok(()),
            Err(_) => Self::init(account, mint, payer, owner, system_program, token_program),
        }
    }
}

pub struct ProgramAccount;

impl AccountCheck for ProgramAccount {
    fn check(account: &AccountInfo) -> Result<(), ProgramError> {
        if account.owner().ne(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner.into());
        }

        if account.data_len().ne(&Escrow::LEN) {
            return Err(ProgramError::InvalidAccountData.into());
        }

        Ok(())
    }
}

pub trait ProgramAccountInit {
    fn init<'a, T: Sized>(
        payer: &AccountInfo,
        account: &AccountInfo,
        seeds: &[Seed<'a>],
        space: usize,
    ) -> ProgramResult;
}

impl ProgramAccountInit for ProgramAccount {
    fn init<'a, T: Sized>(
        payer: &AccountInfo,
        account: &AccountInfo,
        seeds: &[Seed<'a>],
        space: usize,
    ) -> ProgramResult {
        let lamports = Rent::get()?.minimum_balance(space);

        let signer = [Signer::from(seeds)];

        CreateAccount {
            from: payer,
            to: account,
            lamports,
            space: space as u64,
            owner: &crate::ID,
        }
        .invoke_signed(&signer)?;

        Ok(())
    }
}

pub trait AccountClose {
    fn close(account: &AccountInfo, destination: &AccountInfo) -> ProgramResult;
}

impl AccountClose for ProgramAccount {
    fn close(account: &AccountInfo, destination: &AccountInfo) -> ProgramResult {
        {
            let mut data = account.try_borrow_mut_data()?;
            data[0] = 0xff;
        }

        *destination.try_borrow_mut_lamports()? += *account.try_borrow_lamports()?;
        account.resize(1)?;
        account.close()
    }
}
