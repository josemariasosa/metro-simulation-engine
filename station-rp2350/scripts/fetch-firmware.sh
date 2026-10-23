#!/usr/bin/env bash
set -euo pipefail

# Pinned upstream Embassy commit
EMBASSY_COMMIT="26c3ef9f8d0e2e167d25381f3a1cd158353e0873"

BASE="https://raw.githubusercontent.com/embassy-rs/embassy/${EMBASSY_COMMIT}/cyw43-firmware"

for file in \
    43439A0.bin \
    43439A0_clm.bin \
    nvram_rp2040.bin
do
    curl -fL \
        "${BASE}/${file}" \
        -o "firmware/${file}"
done