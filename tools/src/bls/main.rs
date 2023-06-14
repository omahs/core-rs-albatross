use nimiq_bls::{PublicKey, SecretKey};
use nimiq_utils::key_rng::SecureGenerate;

fn main() {
    let secret_key = SecretKey::generate_default_csprng();
    let public_key = PublicKey::from_secret(&secret_key);

    println!("# Public Key:");
    println!();
    let x: Vec<u8> = postcard::to_allocvec(&public_key).unwrap(); // need to apply a little bit of force to make it a slice
    println!("{}", hex::encode(x));
    println!();
    println!("# Secret Key:");
    println!();
    println!(
        "{}",
        hex::encode(postcard::to_allocvec(&secret_key).unwrap())
    );
    println!();
    println!("# Proof Of Knowledge:");
    println!();
    println!("{}", hex::encode(secret_key.sign(&public_key).compress()));
}
