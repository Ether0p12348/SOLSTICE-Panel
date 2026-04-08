#!/bin/bash
cargo build --release

APP=solstice-panel
ARCH=aarch64
VER=1.0.0
OUT_DIR=dist
PKG_NAME="${APP}-${ARCH}-${VER}"
PKG_DIR="${OUT_DIR}/${PKG_NAME}"

rm -rf "${PKG_DIR}"
mkdir -p "${PKG_DIR}"

install -m 755 target/release/solstice-panel "${PKG_DIR}/solstice-panel"
cp -r config "${PKG_DIR}/config"
cp -r assets "${PKG_DIR}/assets"
[ -f LICENSE.md ] && cp LICENSE.md "${PKG_DIR}/LICENSE.md"
[ -f README.md ] && cp README.md "${PKG_DIR}/README.md"

tar -C "${OUT_DIR}" -czf "${PKG_DIR}.tar.gz" "${PKG_NAME}"
sha256sum "${PKG_DIR}.tar.gz" > "${PKG_DIR}.tar.gz.sha256"