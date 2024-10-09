use hex::ToHex;
use serde::{Deserialize, Serialize};
use serde_json;
use std::time::SystemTime;

use ff::Field;
use zk_engine::nova::{
    provider::{PallasEngine, VestaEngine},
    traits::{circuit::TrivialCircuit, snark::default_ck_hint, Engine},
    PublicParams, RecursiveSNARK,
};

use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use sha2::{self, Digest};
use zk_engine::precompiles::signing::SigningCircuit;

#[derive(Serialize, Deserialize, Debug)]
struct Position {
    latitude: f64,
    longitude: f64,
    timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct SignedPosition {
    position: Position,
    signature: String,
    public_key: String,
}

const SECRET_KEY: &'static str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

type E1 = PallasEngine;
type E2 = VestaEngine;

fn main() {
    // Simulate inputs
    let secret_key_hex = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let secret_key = hex::decode(secret_key_hex).unwrap();

    let latitude = 48.8566;
    let longitude = 2.3522;
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // build and sign a Position object, to be sent
    let position = Position {
        latitude,
        longitude,
        timestamp,
    };

    let hash = hash_position(&position);

    // create signing circuit
    type C1 = SigningCircuit<<E1 as Engine>::Scalar>;
    type C2 = TrivialCircuit<<E2 as Engine>::Scalar>;

    let circuit_primary = C1::new(hash, secret_key);
    let circuit_secondary = C2::default();

    // produce public parameters
    println!("Producing public parameters...");
    let pp = PublicParams::<E1>::setup(
        &circuit_primary,
        &circuit_secondary,
        &*default_ck_hint(),
        &*default_ck_hint(),
    )
    .unwrap();

    // produce a recursive SNARK
    println!("Generating a RecursiveSNARK...");
    let mut recursive_snark: RecursiveSNARK<E1> = RecursiveSNARK::<E1>::new(
        &pp,
        &circuit_primary,
        &circuit_secondary,
        &[<E1 as Engine>::Scalar::zero(); 4], // Matching the arity
        &[<E2 as Engine>::Scalar::zero()],
    )
    .unwrap();

    recursive_snark
        .prove_step(&pp, &circuit_primary, &circuit_secondary)
        .unwrap();

    // verify the recursive SNARK
    println!("Verifying a RecursiveSNARK...");
    let res = recursive_snark.verify(
        &pp,
        1,
        &[<E1 as Engine>::Scalar::ZERO; 4], // Matching the arity
        &[<E2 as Engine>::Scalar::ZERO],
    );
    println!("RecursiveSNARK::verify: {:?}", res.is_ok(),);
    let (signature, _) = res.unwrap();
    let mut signature_bytes: [u8; 64] = [0; 64];
    for (i, signature_part) in signature.into_iter().enumerate() {
        let part: [u8; 32] = signature_part.into();
        signature_bytes[i * 16..(i + 1) * 16].copy_from_slice(&part[0..16]);
    }
    println!("Signature : {:?}", signature_bytes);

    let signed_position = sign_coordinates(latitude, longitude, timestamp);
    println!(
        "Expected  : {:?}",
        hex::decode(signed_position.signature).unwrap()
    );
}

#[no_mangle]
fn sign_coordinates(latitude: f64, longitude: f64, timestamp: u64) -> SignedPosition {
    // convert hex encoded secret key to bytes
    let secret_key_bytes = hex::decode(&SECRET_KEY).expect("Invalid hex");
    let secret_key_slice = secret_key_bytes.as_slice();

    let position = Position {
        latitude,
        longitude,
        timestamp,
    };

    // serialize position for hashing purpose
    let payload = serde_json::to_string(&position).expect("JSON serialization");

    // hash payload
    let result = hash_message(&payload);
    // let result = hash_message("Hello, world!");
    let hash = result.to_vec();

    // sign hash
    let (secret_key, public_key) = create_key_pair_from_bytes(secret_key_slice);
    let sig = sign_hash_slice(&secret_key, &hash);

    // serialize signature and public key - needed as ecdsa::Signature does not implement Serialize
    let serialized_signature = sig.serialize_compact().encode_hex::<String>();
    let serialized_public_key = public_key.serialize().encode_hex::<String>();

    SignedPosition {
        position,
        signature: serialized_signature,
        public_key: serialized_public_key,
    }
}

fn create_key_pair_from_bytes(secret_bytes: &[u8]) -> (SecretKey, PublicKey) {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(secret_bytes).expect("32 bytes");
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    (secret_key, public_key)
}

fn hash_position(position: &Position) -> Vec<u8> {
    let payload = serde_json::to_string(&position).expect("JSON serialization");
    let result = hash_message(&payload);
    result.to_vec()
}

fn hash_message(message: &str) -> Box<[u8]> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(message.as_bytes());
    hasher.finalize().as_slice().into()
}

fn sign_hash_slice(secret_key: &SecretKey, hash: &[u8]) -> secp256k1::ecdsa::Signature {
    let message = Message::from_digest_slice(&hash).expect("32 bytes");
    let secp = Secp256k1::new();
    secp.sign_ecdsa(&message, &secret_key)
}

/* fn verify_signature(
    public_key: &PublicKey,
    sig: &secp256k1::ecdsa::Signature,
    hash: &[u8],
) -> bool {
    let secp = Secp256k1::new();
    let message = Message::from_digest_slice(&hash).expect("32 bytes");
    secp.verify_ecdsa(&message, &sig, &public_key).is_ok()
}

fn deser_pubkey(pubkey_str: &str) -> PublicKey {
    PublicKey::from_slice(<[u8; 33]>::from_hex(&pubkey_str).unwrap().as_ref()).expect("33 bytes")
}
 */
