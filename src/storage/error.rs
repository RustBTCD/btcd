use bitcoin::BlockHash;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Db(#[from] rocksdb::Error),

    #[error("column family `{0}` is missing")]
    MissingColumnFamily(&'static str),

    #[error("corrupted {what}: {reason}")]
    Corrupted { what: &'static str, reason: String },

    #[error("block {block} does not extend the current tip {tip:?}")]
    NotExtendingTip {
        block: BlockHash,
        tip: Option<BlockHash>,
    },

    #[error("block {block} is not the current tip {tip:?}")]
    NotTip {
        block: BlockHash,
        tip: Option<BlockHash>,
    },

    #[error("undo data for block {0} not found")]
    MissingUndo(BlockHash),

    #[error("block spends {expected} inputs but {actual} spent coins were given")]
    SpentCoinsMismatch { expected: usize, actual: usize },
}

impl StorageError {
    pub(crate) fn corrupted(what: &'static str, reason: impl ToString) -> Self {
        Self::Corrupted {
            what,
            reason: reason.to_string(),
        }
    }
}
