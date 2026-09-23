//! Serialisation choices the protocol leaves to the sender.
//!
//! A `getdata` for a plain transaction or block selects the form without witnesses, while the
//! witness inventory types select the form with them (BIP 144). The message crate encodes
//! whatever the value carries, so stripping is ours to do.

use bitcoin::block::Checked;
use bitcoin::{Block, Transaction, Witness};

/// A copy without witnesses, which encodes in the non-witness form.
pub fn transaction_without_witness(transaction: &Transaction) -> Transaction {
    let mut stripped = transaction.clone();
    for input in &mut stripped.inputs {
        input.witness = Witness::new();
    }
    stripped
}

/// A copy of a block whose transactions carry no witnesses.
pub fn block_without_witness(block: &Block<Checked>) -> Block {
    let transactions = block
        .transactions()
        .iter()
        .map(transaction_without_witness)
        .collect();
    Block::new_unchecked(*block.header(), transactions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::encoding::encode_to_vec;
    use bitcoin::ext::*;
    use bitcoin::{Network, constants::genesis_block};

    fn witness_transaction() -> Transaction {
        let mut transaction = genesis_block(Network::Bitcoin).transactions()[0].clone();
        transaction.inputs[0].witness = Witness::from_slice(&[[1u8; 32]]);
        transaction
    }

    #[test]
    fn stripping_changes_the_bytes_but_not_the_txid() {
        let transaction = witness_transaction();
        let stripped = transaction_without_witness(&transaction);

        assert_ne!(encode_to_vec(&transaction), encode_to_vec(&stripped));
        assert_eq!(stripped.compute_txid(), transaction.compute_txid());
        assert!(stripped.inputs[0].witness.is_empty());
    }

    #[test]
    fn a_block_keeps_its_header_and_transaction_order() {
        let block = genesis_block(Network::Bitcoin);
        let stripped = block_without_witness(&block);
        assert_eq!(stripped.block_hash(), block.block_hash());
        assert_eq!(
            stripped.assume_checked(None).transactions().len(),
            block.transactions().len()
        );
    }
}
