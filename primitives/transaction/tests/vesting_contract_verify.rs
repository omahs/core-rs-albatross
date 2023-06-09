use nimiq_keys::{Address, KeyPair, PrivateKey};
use nimiq_primitives::{
    account::AccountType, coin::Coin, networks::NetworkId, transaction::TransactionError,
};
use nimiq_transaction::{
    account::{vesting_contract::CreationTransactionData, AccountTransactionVerification},
    SignatureProof, Transaction, TransactionFlags,
};
use serde::{Deserialize, Serialize, SerializingError};

const OWNER_KEY: &str = "9d5bd02379e7e45cf515c788048f5cf3c454ffabd3e83bd1d7667716c325c3c0";

fn key_pair() -> KeyPair {
    KeyPair::from(PrivateKey::deserialize_from_vec(&hex::decode(OWNER_KEY).unwrap()).unwrap())
}

#[test]
#[allow(unused_must_use)]
fn it_can_verify_creation_transaction() {
    let mut data: Vec<u8> = Vec::with_capacity(Address::SIZE + 8);
    let owner = Address::from([0u8; 20]);
    Serialize::serialize(&owner, &mut data);
    Serialize::serialize(&100u64, &mut data);

    let mut transaction = Transaction::new_contract_creation(
        vec![],
        owner,
        AccountType::Basic,
        AccountType::Vesting,
        100.try_into().unwrap(),
        0.try_into().unwrap(),
        0,
        NetworkId::UnitAlbatross,
    );

    // Invalid data
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Err(TransactionError::InvalidData)
    );
    transaction.data = data;

    // Invalid recipient
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Err(TransactionError::InvalidForRecipient)
    );
    transaction.recipient = transaction.contract_creation_address();

    // Valid
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Ok(())
    );

    // Invalid transaction flags
    transaction.flags = TransactionFlags::empty();
    transaction.recipient = transaction.contract_creation_address();
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Err(TransactionError::InvalidForRecipient)
    );
    transaction.flags = TransactionFlags::CONTRACT_CREATION;

    // Valid
    let mut data: Vec<u8> = Vec::with_capacity(Address::SIZE + 24);
    let sender = Address::from([0u8; 20]);
    Serialize::serialize(&sender, &mut data);
    Serialize::serialize(&100u64, &mut data);
    Serialize::serialize(&100u64, &mut data);
    Serialize::serialize(&Coin::try_from(100).unwrap(), &mut data);
    transaction.data = data;
    transaction.recipient = transaction.contract_creation_address();
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Ok(())
    );

    // Valid
    let mut data: Vec<u8> = Vec::with_capacity(Address::SIZE + 32);
    let sender = Address::from([0u8; 20]);
    Serialize::serialize(&sender, &mut data);
    Serialize::serialize(&100u64, &mut data);
    Serialize::serialize(&100u64, &mut data);
    Serialize::serialize(&Coin::try_from(100).unwrap(), &mut data);
    Serialize::serialize(&Coin::try_from(100).unwrap(), &mut data);
    transaction.data = data;
    transaction.recipient = transaction.contract_creation_address();
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Ok(())
    );

    // step amount > total amount
    let data = CreationTransactionData {
        owner: Address::from([0u8; 20]),
        start_time: 100,
        time_step: 0,
        step_amount: Coin::try_from(1000).unwrap(),
        total_amount: Coin::try_from(100).unwrap(),
    };
    transaction.data = data.serialize_to_vec();
    transaction.recipient = transaction.contract_creation_address();
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Ok(())
    );
}

#[test]
fn it_can_verify_outgoing_transactions() {
    let key_pair = key_pair();

    let mut tx = Transaction::new_basic(
        Address::from([1u8; 20]),
        Address::from([2u8; 20]),
        1.try_into().unwrap(),
        1000.try_into().unwrap(),
        1,
        NetworkId::UnitAlbatross,
    );
    tx.sender_type = AccountType::Vesting;

    assert!(matches!(
        AccountType::verify_outgoing_transaction(&tx),
        Err(TransactionError::InvalidSerialization(
            SerializingError::IoError(_)
        ))
    ));

    let signature = key_pair.sign(&tx.serialize_content()[..]);
    let signature_proof = SignatureProof::from(key_pair.public, signature);
    tx.proof = signature_proof.serialize_to_vec();

    assert_eq!(AccountType::verify_outgoing_transaction(&tx), Ok(()));

    tx.proof[22] = tx.proof[22] % 250 + 1;
    assert_eq!(
        AccountType::verify_outgoing_transaction(&tx),
        Err(TransactionError::InvalidProof)
    );

    tx.proof[22] = tx.proof[22] % 251 + 3;
    // Proof is not a valid point, so Deserialize will result in an error.
    assert_eq!(
        AccountType::verify_outgoing_transaction(&tx),
        Err(TransactionError::InvalidProof)
    );
}
