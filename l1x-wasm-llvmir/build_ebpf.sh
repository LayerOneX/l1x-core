#!/bin/bash
set -e
set -x
set -u

SOURCE="${1}"
VERSIONED="${SOURCE%.*}.versioned.ll"
TARGET="${SOURCE%.*}.o"
BASE64_TARGET="${SOURCE%.*}.base64"
LLC="llc-17"
STRIP="llvm-strip-17"

cp "${SOURCE}" "${VERSIONED}"
echo '@_OBJECT_VERSION = global i64 1, section "_version", align 1' >> "${VERSIONED}"
echo '@_EXPECTED_RUNTIME_VERSION = global i64 2, section "_version", align 1' >> "${VERSIONED}"

# MAC OS: wasm-llvmir tool adds a comma before a section name for unknown reason.
# This is a workaround until it's fixed in wasm-llvmir
sed -i -e 's/section ",_memory"/section "_memory"/g; s/section ",_init_memory"/section "_init_memory"/g' "${VERSIONED}"

# $ llc-17 -march=bpf -mattr=help
# Available CPUs for this target:

#   generic - Select the generic processor.
#   probe   - Select the probe processor.
#   v1      - Select the v1 processor.
#   v2      - Select the v2 processor.
#   v3      - Select the v3 processor.

# Available features for this target:

#   alu32    - Enable ALU32 instructions.
#   dummy    - unused feature.
#   dwarfris - Disable MCAsmInfo DwarfUsesRelocationsAcrossSections.

# Use +feature to enable a feature, or -feature to disable it.
# For example, llc -mcpu=mycpu -mattr=+feature1,-feature2

# https://chromium.googlesource.com/external/github.com/llvm/llvm-project/+/refs/heads/upstream/release/17.x/llvm/lib/Target/BPF/BPFSubtarget.cpp
#   if (CPU == "v3") {
#    HasJmpExt = true;
#    HasJmp32 = true;
#    HasAlu32 = true;
#    return;
#  }
${LLC} -march=bpf -mcpu=v3 -filetype=obj --nozero-initialized-in-bss -bpf-stack-size 4096 "${VERSIONED}" -o "${TARGET}"
${STRIP} -x "${TARGET}"
base64 -w0 "${TARGET}" > "${BASE64_TARGET}"
# opt -O2 -S -o hello_ebpf.opt.ll --always-inline hello_ebpf.ll
# llc -march=bpf -filetype=obj hello_ebpf.opt.ll -o hello_ebpf.o
# objcopy -I elf64-little -O binary hello_ebpf.o hello_ebpf.bin
