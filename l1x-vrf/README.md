# L1X VRF Module

* Provides Secp256K1 operations

## Modules

```rust
// Provides signature verification operations.
pub mod common;

// Provides Secret and Public Key generation operations.
pub mod secp_vrf;
```

## Usage

* Import following modules

```rust
use l1x_vrf::common::{verify, sign, verify_with_pubkey, sign_with_secret};
use l1x_vrf::secp_vrf::{SecretKey, PublicKey};
```
* For detailed usage please refer to doctests in the source code.