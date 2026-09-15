use bitcoin::TxOut;
use bitcoin::consensus::encode::{deserialize_partial, serialize};

use super::{Result, StorageError};

/// Bitcoin Core `MAX_SCRIPT_SIZE`.
pub const MAX_SCRIPT_SIZE: usize = 10_000;

/// Bytes of the packed `height << 1 | is_coinbase` code at the start of a coin.
const CODE_LEN: usize = 4;
/// Smallest possible encoded coin: code, 8-byte amount, 1-byte empty script length.
const MIN_COIN_LEN: usize = CODE_LEN + 8 + 1;

/// An unspent transaction output together with the context needed to spend it.
///
/// `height` and `is_coinbase` are required for coinbase maturity and BIP68 relative locks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coin {
    pub output: TxOut,
    pub height: u32,
    pub is_coinbase: bool,
}

impl Coin {
    /// Outputs that can never be spent do not enter the UTXO set.
    /// Mirrors Bitcoin Core `CScript::IsUnspendable`.
    pub fn is_unspendable(output: &TxOut) -> bool {
        output.script_pubkey.is_op_return() || output.script_pubkey.len() > MAX_SCRIPT_SIZE
    }

    /// Layout: `height << 1 | is_coinbase` as u32 LE, then the consensus-encoded `TxOut`.
    pub fn to_bytes(&self) -> Vec<u8> {
        coin_bytes(&self.output, self.height, self.is_coinbase)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (coin, used) = read_coin(bytes)?;
        if used != bytes.len() {
            return Err(StorageError::corrupted("coin", "trailing bytes"));
        }
        Ok(coin)
    }
}

/// Encodes a coin from borrowed parts, so callers holding a `&TxOut` need not build a [`Coin`].
pub(crate) fn coin_bytes(output: &TxOut, height: u32, is_coinbase: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(MIN_COIN_LEN + output.script_pubkey.len());
    write_coin(&mut bytes, output, height, is_coinbase);
    bytes
}

fn write_coin(bytes: &mut Vec<u8>, output: &TxOut, height: u32, is_coinbase: bool) {
    debug_assert!(height < 1 << 31, "height does not fit the packed coin code");
    let code = (height << 1) | u32::from(is_coinbase);
    bytes.extend_from_slice(&code.to_le_bytes());
    bytes.extend_from_slice(&serialize(output));
}

/// Parses one coin from the start of `bytes`. Returns the coin and the number of bytes used.
fn read_coin(bytes: &[u8]) -> Result<(Coin, usize)> {
    let code = bytes
        .get(..CODE_LEN)
        .ok_or_else(|| StorageError::corrupted("coin", "shorter than its height code"))?;
    let code = u32::from_le_bytes(code.try_into().expect("slice is CODE_LEN bytes"));
    let (output, used) = deserialize_partial::<TxOut>(&bytes[CODE_LEN..])
        .map_err(|e| StorageError::corrupted("coin", e))?;
    let coin = Coin {
        output,
        height: code >> 1,
        is_coinbase: code & 1 == 1,
    };
    Ok((coin, CODE_LEN + used))
}

/// Coins spent by a block, in the order of its non-coinbase inputs.
///
/// Outpoints are not stored: they are recovered from the block's inputs, as Bitcoin Core does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockUndo(pub Vec<Coin>);

impl BlockUndo {
    /// Layout: coin count as u32 LE, then each coin in the [`Coin::to_bytes`] layout.
    pub fn to_bytes(&self) -> Vec<u8> {
        undo_bytes(&self.0)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let count = bytes
            .get(..4)
            .ok_or_else(|| StorageError::corrupted("undo data", "shorter than its coin count"))?;
        let count = u32::from_le_bytes(count.try_into().expect("slice is 4 bytes")) as usize;

        // Cap the preallocation by what the bytes could hold, so a corrupted count cannot
        // trigger a huge allocation.
        let mut coins = Vec::with_capacity(count.min(bytes.len() / MIN_COIN_LEN));
        let mut pos = 4;
        for _ in 0..count {
            let (coin, used) = read_coin(&bytes[pos..])?;
            coins.push(coin);
            pos += used;
        }
        if pos != bytes.len() {
            return Err(StorageError::corrupted("undo data", "trailing bytes"));
        }
        Ok(BlockUndo(coins))
    }
}

/// Encodes coins as undo data without first copying them into a [`BlockUndo`].
pub(crate) fn undo_bytes(coins: &[Coin]) -> Vec<u8> {
    let count = u32::try_from(coins.len()).expect("a block spends fewer than 2^32 coins");
    let mut bytes = count.to_le_bytes().to_vec();
    for coin in coins {
        write_coin(&mut bytes, &coin.output, coin.height, coin.is_coinbase);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{Amount, ScriptBuf};

    fn coin(height: u32, is_coinbase: bool) -> Coin {
        Coin {
            output: TxOut {
                value: Amount::from_sat(12_345),
                script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
            },
            height,
            is_coinbase,
        }
    }

    #[test]
    fn coin_roundtrip() {
        for c in [
            coin(0, false),
            coin(840_000, true),
            coin((1 << 31) - 1, true),
        ] {
            assert_eq!(Coin::from_bytes(&c.to_bytes()).unwrap(), c);
        }
    }

    #[test]
    fn coin_rejects_truncated_and_trailing_bytes() {
        let bytes = coin(7, false).to_bytes();
        assert!(Coin::from_bytes(&bytes[..bytes.len() - 1]).is_err());
        assert!(Coin::from_bytes(&bytes[..2]).is_err());
        assert!(Coin::from_bytes(&[bytes.as_slice(), &[0]].concat()).is_err());
    }

    #[test]
    fn undo_roundtrip() {
        for undo in [
            BlockUndo::default(),
            BlockUndo(vec![coin(1, true), coin(2, false)]),
        ] {
            assert_eq!(BlockUndo::from_bytes(&undo.to_bytes()).unwrap(), undo);
        }
    }

    #[test]
    fn undo_rejects_bad_count_and_trailing_bytes() {
        let bytes = BlockUndo(vec![coin(1, true), coin(2, false)]).to_bytes();
        assert!(BlockUndo::from_bytes(&bytes[..bytes.len() - 1]).is_err());
        assert!(BlockUndo::from_bytes(&[bytes.as_slice(), &[0]].concat()).is_err());

        let mut huge_count = bytes.clone();
        huge_count[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(BlockUndo::from_bytes(&huge_count).is_err());
    }

    #[test]
    fn unspendable_outputs() {
        let op_return = TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::from_bytes(vec![0x6a]),
        };
        let too_big = TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::from_bytes(vec![0x51; MAX_SCRIPT_SIZE + 1]),
        };
        assert!(Coin::is_unspendable(&op_return));
        assert!(Coin::is_unspendable(&too_big));
        assert!(!Coin::is_unspendable(&coin(0, false).output));
    }
}
