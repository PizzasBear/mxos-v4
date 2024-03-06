#!/bin/env sh

cargo b || exit "$?"
mkdir -p ./esp/efi/boot
cp ./target/x86_64-unknown-uefi/debug/mxos-v4.efi ./esp/efi/boot/bootx64.efi || exit "$?"

mkdir -p "$(date +"./logs/%Y-%m-%d/")"
ln -sf "$(date +"%Y-%m-%d/%H-%M-%S-%Z.log")" ./logs/last.log
qemu-system-x86_64 \
	-enable-kvm \
	-s \
	-m 8G \
	-drive format=raw,file=fat:rw:esp \
	-serial "file:$(date +"logs/%Y-%m-%d/%H-%M-%S-%Z.log")" \
	-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd \
	-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_VARS.fd
