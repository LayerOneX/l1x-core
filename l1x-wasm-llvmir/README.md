# WASM to LLVM IR code translator


The l1x-wasm-llvmir code translator converts L1X smart contract WASM binaries into LLVM IR code format. This translation streamlines the process of porting code from WASM to LLVM IR, and eventually to eBPF code, enabling smooth execution within the L1X VM's eBPF runtime. L1X is an L1X Prioprietary tech.

Translates a WASM file to L1X VM llvmir representation. Used by `cargo-l1x`

**Requirements:**
```
llvm-15
```

**Build:**
```bash
cargo build
```

**Test**
```bash
cargo test
```

**Run:**
```bash
cargo run some.wasm -o some.ll
```

**Create eBPF object file:**

*Require installed `llvm-17`*

```bash
./build_ebpf.sh some.ll
```

---
