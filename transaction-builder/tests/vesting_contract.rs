use std::convert::{TryFrom, TryInto};

use nimiq_keys::{Address, KeyPair, PrivateKey};
use nimiq_primitives::{account::AccountType, coin::Coin, networks::NetworkId};
use nimiq_test_log::test;
use nimiq_transaction::{SignatureProof, Transaction};
use nimiq_transaction_builder::{Recipient, TransactionBuilder};

#[test]
#[allow(unused_must_use)]
fn it_can_create_creation_transaction() {
    let mut data = [0u8; Address::SIZE + 8];
    let owner = Address::from([0u8; 20]);
    postcard::to_slice(&owner, &mut data).unwrap();
    postcard::to_slice(&100u64.to_be_bytes(), &mut data[Address::SIZE..]).unwrap();

    let mut transaction = Transaction::new_contract_creation(
        data.to_vec(),
        owner.clone(),
        AccountType::Basic,
        AccountType::Vesting,
        100.try_into().unwrap(),
        0.try_into().unwrap(),
        0,
        NetworkId::Dummy,
    );

    // Valid
    let mut recipient = Recipient::new_vesting_builder(owner.clone());
    recipient.with_steps(Coin::from_u64_unchecked(100), 0, 100, 1);

    let mut builder = TransactionBuilder::new();
    builder
        .with_sender(owner.clone())
        .with_recipient(recipient.generate().unwrap())
        .with_value(100.try_into().unwrap())
        .with_validity_start_height(0)
        .with_network_id(NetworkId::Dummy);
    let proof_builder = builder
        .generate()
        .expect("Builder should be able to create transaction");
    let proof_builder = proof_builder.unwrap_basic();
    assert_eq!(proof_builder.transaction, transaction);

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

    let mut recipient = Recipient::new_vesting_builder(owner.clone());
    recipient
        .with_start_time(100)
        .with_time_step(100)
        .with_step_amount(100.try_into().unwrap())
        .with_total_amount(100.try_into().unwrap());

    let mut builder = TransactionBuilder::new();
    builder
        .with_sender(owner.clone())
        .with_recipient(recipient.generate().unwrap())
        .with_value(100.try_into().unwrap())
        .with_validity_start_height(0)
        .with_network_id(NetworkId::Dummy);
    let proof_builder = builder
        .generate()
        .expect("Builder should be able to create transaction");
    let proof_builder = proof_builder.unwrap_basic();
    assert_eq!(proof_builder.transaction, transaction);

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
        &Coin::try_from(101).unwrap(),
        &mut data[Address::SIZE + 24..],
    )
    .unwrap();
    transaction.data = data.to_vec();
    transaction.recipient = transaction.contract_creation_address();

    let mut recipient = Recipient::new_vesting_builder(owner.clone());
    recipient
        .with_start_time(100)
        .with_time_step(100)
        .with_step_amount(100.try_into().unwrap())
        .with_total_amount(101.try_into().unwrap());

    let mut builder = TransactionBuilder::new();
    builder
        .with_sender(owner)
        .with_recipient(recipient.generate().unwrap())
        .with_value(100.try_into().unwrap())
        .with_validity_start_height(0)
        .with_network_id(NetworkId::Dummy);
    let proof_builder = builder
        .generate()
        .expect("Builder should be able to create transaction");
    let proof_builder = proof_builder.unwrap_basic();
    assert_eq!(proof_builder.transaction, transaction);
}

#[test]
fn it_can_create_outgoing_transactions() {
    let sender_priv_key: PrivateKey = postcard::from_bytes(
        &hex::decode("9d5bd02379e7e45cf515c788048f5cf3c454ffabd3e83bd1d7667716c325c3c0").unwrap(),
    )
    .unwrap();

    let key_pair = KeyPair::from(sender_priv_key);
    let mut tx = Transaction::new_basic(
        Address::from([1u8; 20]),
        Address::from([2u8; 20]),
        1.try_into().unwrap(),
        1000.try_into().unwrap(),
        1,
        NetworkId::Dummy,
    );
    tx.sender_type = AccountType::Vesting;

    let signature = key_pair.sign(&tx.serialize_content()[..]);
    let signature_proof = SignatureProof::from(key_pair.public, signature);
    tx.proof = postcard::to_allocvec(&signature_proof).unwrap();

    let mut builder = TransactionBuilder::new();
    builder
        .with_sender(Address::from([1u8; 20]))
        .with_sender_type(AccountType::Vesting)
        .with_recipient(Recipient::new_basic(Address::from([2u8; 20])))
        .with_value(1.try_into().unwrap())
        .with_fee(1000.try_into().unwrap())
        .with_validity_start_height(1)
        .with_network_id(NetworkId::Dummy);
    let proof_builder = builder
        .generate()
        .expect("Builder should be able to create transaction");
    let mut proof_builder = proof_builder.unwrap_basic();
    proof_builder.sign_with_key_pair(&key_pair);
    assert_eq!(proof_builder.generate().unwrap(), tx);
}
