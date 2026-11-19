// SPDX-License-Identifier: Apache-2.0
//! Raw test-key primitives for the independent Python encoder. No protocol encoding is done here.

use om_core::crypto::pqxdh::x25519_pub;
use om_core::crypto::sig::DeviceSigner;
use om_core::crypto::{hex, unhex};
use std::io::{self, Read};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed = args.next().expect("test seed or --x25519");
    if seed == "--x25519" {
        let seed = unhex(&args.next().unwrap()).unwrap().try_into().unwrap();
        assert!(args.next().is_none());
        println!("{}", hex(&x25519_pub(&seed)));
        return;
    }
    let seeds = unhex(&seed).unwrap();
    assert_eq!(seeds.len(), 64);
    let signer = DeviceSigner {
        ed_seed: seeds[..32].try_into().unwrap(),
        mldsa_seed: seeds[32..].try_into().unwrap(),
    };
    let result = if let Some(randomness) = args.next() {
        let mut preimage = Vec::new();
        io::stdin().read_to_end(&mut preimage).unwrap();
        signer.sign_with(&preimage, unhex(&randomness).unwrap().try_into().unwrap())
    } else {
        signer.public()
    };
    assert!(args.next().is_none());
    println!("{}", hex(&result));
}
