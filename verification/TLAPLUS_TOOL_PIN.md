# TLA+ tool pin for OM verification

For reproducible V-002 model checking, OM currently pins the command-line TLA+ tools to **v1.7.4 (Xenophanes)** from the official `tlaplus/tlaplus` GitHub release.

Expected upstream SHA-1 for `tla2tools.jar`:

```text
bee4a54f3ee3d4afc347c3240ec2d9e93b075104
```

The JAR is **not vendored** in this checkpoint. Verification CI should fetch/pin it outside the normative architecture archive, verify its digest, then run for example:

```sh
java -cp /path/to/tla2tools.jar tlc2.TLC \
  -config verification/tla/AccountControlState.cfg \
  verification/tla/AccountControlState.tla
```

Tooling is evidence infrastructure, not a runtime dependency of OM.
