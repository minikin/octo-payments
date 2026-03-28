use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum PaymentEngineError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Missing amount for tx{0}")]
    MissingAmount(u32),
}
