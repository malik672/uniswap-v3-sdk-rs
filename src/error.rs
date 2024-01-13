use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Should not be zero")]
    IsZero,

    #[error("diiferent chain id")]
    ChainIdIsDifferent,

    #[error("last pool does not involve specific token in the output")]
    InvolvesToken,

    #[error("Token not present in current pool")]
    TokenNotInPool,
}
