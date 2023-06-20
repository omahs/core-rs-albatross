use log::error;
use nimiq_keys::Address;
use nimiq_primitives::{account::AccountType, coin::Coin};
use serde::{Deserialize, Serialize};

use crate::{
    account::AccountTransactionVerification, SignatureProof, Transaction, TransactionError,
    TransactionFlags,
};

/// The verifier trait for a basic account. This only uses data available in the transaction.
pub struct VestingContractVerifier {}

impl AccountTransactionVerification for VestingContractVerifier {
    fn verify_incoming_transaction(transaction: &Transaction) -> Result<(), TransactionError> {
        assert_eq!(transaction.recipient_type, AccountType::Vesting);

        if !transaction
            .flags
            .contains(TransactionFlags::CONTRACT_CREATION)
        {
            error!(
                "Only contract creation is allowed for this transaction:\n{:?}",
                transaction
            );
            return Err(TransactionError::InvalidForRecipient);
        }

        if transaction.flags.contains(TransactionFlags::SIGNALING) {
            error!(
                "Signaling not allowed for this transaction:\n{:?}",
                transaction
            );
            return Err(TransactionError::InvalidForRecipient);
        }

        if transaction.recipient != transaction.contract_creation_address() {
            error!("Recipient address must match contract creation address for this transaction:\n{:?}",
                transaction);
            return Err(TransactionError::InvalidForRecipient);
        }

        let allowed_sizes = [Address::SIZE + 8, Address::SIZE + 24, Address::SIZE + 32];
        if !allowed_sizes.contains(&transaction.data.len()) {
            warn!(
                len = transaction.data.len(),
                ?transaction,
                "Invalid data length for this transaction",
            );
            return Err(TransactionError::InvalidData);
        }

        CreationTransactionData::parse(transaction).map(|_| ())
    }

    fn verify_outgoing_transaction(transaction: &Transaction) -> Result<(), TransactionError> {
        assert_eq!(transaction.sender_type, AccountType::Vesting);

        // Verify signature.
        let signature_proof: SignatureProof = postcard::from_bytes(&transaction.proof[..])?;

        if !signature_proof.verify(transaction.serialize_content().as_slice()) {
            warn!("Invalid signature for this transaction:\n{:?}", transaction);
            return Err(TransactionError::InvalidProof);
        }

        Ok(())
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct CreationTransactionData {
    pub owner: Address,
    #[serde(with = "postcard::fixint::be")]
    pub start_time: u64,
    #[serde(with = "postcard::fixint::be")]
    pub time_step: u64,
    pub step_amount: Coin,
    pub total_amount: Coin,
}

impl CreationTransactionData {
    pub fn parse(transaction: &Transaction) -> Result<Self, TransactionError> {
        let reader = &mut &transaction.data[..];
        let (owner, left_over) = postcard::take_from_bytes(reader)?;

        if transaction.data.len() == Address::SIZE + 8 {
            // Only timestamp: vest full amount at that time
            let (time_step, _) = postcard::take_from_bytes::<[u8; 8]>(left_over)?;
            Ok(CreationTransactionData {
                owner,
                start_time: 0,
                time_step: u64::from_be_bytes(time_step),
                step_amount: transaction.value,
                total_amount: transaction.value,
            })
        } else if transaction.data.len() == Address::SIZE + 24 {
            let (start_time, left_over) = postcard::take_from_bytes::<[u8; 8]>(left_over)?;
            let (time_step, left_over) = postcard::take_from_bytes::<[u8; 8]>(left_over)?;
            let (step_amount, _) = postcard::take_from_bytes(left_over)?;
            Ok(CreationTransactionData {
                owner,
                start_time: u64::from_be_bytes(start_time),
                time_step: u64::from_be_bytes(time_step),
                step_amount,
                total_amount: transaction.value,
            })
        } else if transaction.data.len() == Address::SIZE + 32 {
            // Create a vesting account with some instantly vested funds or additional funds considered.
            let (start_time, left_over) = postcard::take_from_bytes::<[u8; 8]>(left_over)?;
            let (time_step, left_over) = postcard::take_from_bytes::<[u8; 8]>(left_over)?;
            let (step_amount, left_over) = postcard::take_from_bytes(left_over)?;
            let (total_amount, _) = postcard::take_from_bytes(left_over)?;
            Ok(CreationTransactionData {
                owner,
                start_time: u64::from_be_bytes(start_time),
                time_step: u64::from_be_bytes(time_step),
                step_amount,
                total_amount,
            })
        } else {
            Err(TransactionError::InvalidData)
        }
    }

    pub fn to_tx_data(&self) -> Result<Vec<u8>, TransactionError> {
        let mut data = postcard::to_allocvec(&self.owner)?;

        if self.step_amount == self.total_amount {
            if self.start_time == 0 {
                data.append(&mut postcard::to_allocvec(&self.time_step.to_be_bytes())?);
            } else {
                data.append(&mut postcard::to_allocvec(&self.start_time.to_be_bytes())?);
                data.append(&mut postcard::to_allocvec(&self.time_step.to_be_bytes())?);
                data.append(&mut postcard::to_allocvec(&self.step_amount)?);
            }
        } else {
            data.append(&mut postcard::to_allocvec(&self.start_time.to_be_bytes())?);
            data.append(&mut postcard::to_allocvec(&self.time_step.to_be_bytes())?);
            data.append(&mut postcard::to_allocvec(&self.step_amount)?);
            data.append(&mut postcard::to_allocvec(&self.total_amount)?);
        }
        Ok(data)
    }
}
