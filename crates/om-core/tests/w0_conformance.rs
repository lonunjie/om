// SPDX-License-Identifier: MPL-2.0
//! Wire objects Core produces must be W0-valid under the registry-driven validator of `om-codec`.
use om_codec::Registry;
use om_core::crypto::rng::insecure_test_rng::InsecureTestRng;
use om_core::crypto::store::Device;

#[test]
fn published_key_material_is_w0_valid() {
    let reg = Registry::load();
    let mut rng = InsecureTestRng::from_label(b"w0-conformance");
    let mut d = Device::new(&mut rng);
    let up = d.generate_one_time(2, 2, &mut rng);
    assert!(reg.check("AsyncKeyBundle", &up.bundle, false).is_ok());
    assert!(
        reg.check_body("AccountControlState", &d.acs().body())
            .is_ok()
    );
    assert!(
        reg.check_body("DeviceCredential", &d.credential().body())
            .is_ok()
    );
}

#[test]
fn sealed_delivery_objects_are_w0_valid_and_verify_against_the_device_credential() {
    use om_core::crypto::delivery;
    let reg = Registry::load();
    let mut rng = InsecureTestRng::from_label(b"w0-sealed");
    let d = Device::new(&mut rng);
    let mailbox = [7u8; 16];
    let (_seed, km) = delivery::new_key(&mailbox, 2_000_000_000, &mut rng);
    let rec = delivery::sign_key_record(&d.signer, &d.device_id, &km, [3; 32]);
    assert!(reg.check("SealedDeliveryKeyRecord", &rec, false).is_ok());
    let cred = d.credential();
    assert_eq!(
        delivery::open_key_record(&rec, &d.device_id, &cred.signing_pk).unwrap(),
        km
    );
    // A record from another device does not verify under this device's credential.
    let other = Device::new(&mut rng);
    assert!(delivery::open_key_record(&rec, &d.device_id, &other.credential().signing_pk).is_err());
    let wrapper = delivery::seal(
        &km.public_key,
        &km.key_id,
        delivery::CORE_ROUTE_URI,
        &mailbox,
        2_000_000_000,
        b"inner",
        &mut rng,
    )
    .unwrap();
    assert!(reg.check("SealedDeliveryEnvelope", &wrapper, false).is_ok());
}
