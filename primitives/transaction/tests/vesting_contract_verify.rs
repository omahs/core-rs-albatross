use nimiq_keys::{Address, KeyPair, PrivateKey};
use nimiq_primitives::{
    account::AccountType, coin::Coin, networks::NetworkId, transaction::TransactionError,
};
use nimiq_transaction::{
    account::{vesting_contract::CreationTransactionData, AccountTransactionVerification},
    SignatureProof, Transaction, TransactionFlags,
};

const OWNER_KEY: &str = "9d5bd02379e7e45cf515c788048f5cf3c454ffabd3e83bd1d7667716c325c3c0";

fn key_pair() -> KeyPair {
    KeyPair::from(postcard::from_bytes::<PrivateKey>(&hex::decode(OWNER_KEY).unwrap()).unwrap())
}

#[test]
#[allow(unused_must_use)]
fn it_can_verify_creation_transaction() {
    let mut data = [0u8; Address::SIZE + 8];
    let owner = Address::from([0u8; 20]);
    postcard::to_slice(&owner, &mut data).unwrap();
    postcard::to_slice(&100u64.to_be_bytes(), &mut data[Address::SIZE..]).unwrap();

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
    transaction.data = data.to_vec();

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
    let mut data = [0u8; Address::SIZE + 24];
    let sender = Address::from([0u8; 20]);
    postcard::to_slice(&sender, &mut data).unwrap();
    postcard::to_slice(&100u64.to_be_bytes(), &mut data[Address::SIZE..]).unwrap();
    postcard::to_slice(&100u64.to_be_bytes(), &mut data[Address::SIZE + 8..]).unwrap();
    postcard::to_slice(
        &Coin::try_from(100).unwrap(),
        &mut data[Address::SIZE + 16..],
    )
    .unwrap();
    transaction.data = data.to_vec();
    transaction.recipient = transaction.contract_creation_address();
    assert_eq!(
        AccountType::verify_incoming_transaction(&transaction),
        Ok(())
    );

    // Valid
    let mut data = [0u8; Address::SIZE + 32];
    let sender = Address::from([0u8; 20]);
    postcard::to_slice(&sender, &mut data).unwrap();
    postcard::to_slice(&100u64.to_be_bytes(), &mut data[Address::SIZE..]).unwrap();
    postcard::to_slice(&100u64.to_be_bytes(), &mut data[Address::SIZE + 8..]).unwrap();
    postcard::to_slice(
        &Coin::try_from(100).unwrap(),
        &mut data[Address::SIZE + 16..],
    )
    .unwrap();
    postcard::to_slice(
        &Coin::try_from(100).unwrap(),
        &mut data[Address::SIZE + 24..],
    )
    .unwrap();
    transaction.data = data.to_vec();
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
    transaction.data = postcard::to_allocvec(&data).unwrap();
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
            postcard::Error::DeserializeUnexpectedEnd
        ))
    ));

    let signature = key_pair.sign(&tx.serialize_content()[..]);
    let signature_proof = SignatureProof::from(key_pair.public, signature);
    tx.proof = postcard::to_allocvec(&signature_proof).unwrap();

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
