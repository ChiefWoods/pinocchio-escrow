use pinocchio::program_error::ProgramError;

impl From<PinocchioError> for ProgramError {
    fn from(e: PinocchioError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

pub enum PinocchioError {
    NotSigner,
    InvalidAddress,
}
