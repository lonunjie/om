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
